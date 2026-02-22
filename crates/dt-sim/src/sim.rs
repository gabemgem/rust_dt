//! The `Sim` struct and its tick loop.

use std::collections::HashMap;

use dt_agent::{AgentRngs, AgentStore};
use dt_behavior::{BehaviorModel, ContactEvent, ContactKind, Intent, SimContext};
use dt_core::{AgentId, NodeId, SimClock, SimConfig, Tick};
use dt_mobility::{MobilityEngine, MobilityStore};
use dt_schedule::{ActivityPlan, WakeQueue};
use dt_spatial::{RoadNetwork, Router};

use crate::{SimObserver, SimResult};

// ── Per-agent inputs assembled before the intent phase ────────────────────────

/// Data pre-collected for one woken agent before the (potentially parallel)
/// intent phase.  Building this sequentially keeps the intent phase
/// side-effect-free.
struct AgentInputs {
    /// Messages waiting in the queue for this agent (drained this tick).
    messages: Vec<(AgentId, Vec<u8>)>,
    /// Agents co-located at the same road node this tick.
    contacts: Vec<ContactEvent>,
}

// ── Sim ───────────────────────────────────────────────────────────────────────

/// The main simulation runner.
///
/// `Sim<B, R>` holds all simulation state and drives the four-phase tick loop:
///
/// 1. **Arrivals**: agents reaching their destination are marked stationary
///    and re-inserted into the wake queue via their activity plan.
/// 2. **Wake**: drain agents scheduled for this tick.
/// 3. **Intent phase** (optionally parallel with the `parallel` feature):
///    - Call [`BehaviorModel::replan`] for each woken agent.
///    - Deliver any pending messages via [`BehaviorModel::on_message`].
///    - Report co-located agents via [`BehaviorModel::on_contacts`].
/// 4. **Apply phase** (sequential, ascending `AgentId` for determinism):
///    - `WakeAt(t)`         → insert into wake queue.
///    - `TravelTo{..}`      → start journey; push arrival tick.
///    - `SendMessage{..}`   → store in message queue for recipient's next wake.
///
/// Create via [`SimBuilder`][crate::SimBuilder].
pub struct Sim<B: BehaviorModel, R: Router> {
    /// Global configuration (total ticks, seed, tick duration, …).
    pub config: SimConfig,

    /// Simulation clock — tracks the current tick and maps to wall time.
    pub clock: SimClock,

    /// Read-only agent state (SoA arrays).  Behavior models access this
    /// through `SimContext`.
    pub agents: AgentStore,

    /// Per-agent deterministic RNGs, separated for the split-borrow pattern.
    pub rngs: AgentRngs,

    /// Per-agent activity plans, indexed by `AgentId`.
    pub plans: Vec<ActivityPlan>,

    /// Sparse wake queue (`BTreeMap<Tick, Vec<AgentId>>`).
    pub wake_queue: WakeQueue,

    /// Mobility engine: routes `TravelTo` intents and tracks movement state.
    pub mobility: MobilityEngine<R>,

    /// The behavior model.  Called once per woken agent per tick.
    pub behavior: B,

    /// Road network.  Required for `TravelTo` intents; use
    /// [`RoadNetwork::empty()`] if no routing is needed.
    pub network: RoadNetwork,

    /// Pending messages keyed by recipient `AgentId`.
    ///
    /// Messages sent via `Intent::SendMessage` accumulate here during the
    /// apply phase.  They are drained (and `on_message` called) the next
    /// time the recipient wakes.
    pub message_queue: HashMap<AgentId, Vec<(AgentId, Vec<u8>)>>,
}

impl<B: BehaviorModel, R: Router> Sim<B, R> {
    // ── Public API ────────────────────────────────────────────────────────

    /// Run the simulation from the current tick to `config.end_tick()`.
    ///
    /// Calls observer hooks at every tick boundary.  Use
    /// [`NoopObserver`][crate::NoopObserver] if you don't need callbacks.
    pub fn run<O: SimObserver>(&mut self, observer: &mut O) -> SimResult<()> {
        loop {
            let now = self.clock.current_tick;
            if now >= self.config.end_tick() {
                break;
            }

            observer.on_tick_start(now);
            let woken = self.process_tick(now)?;
            observer.on_tick_end(now, woken);
            if self.config.output_interval_ticks > 0
                && now.0.is_multiple_of(self.config.output_interval_ticks)
            {
                observer.on_snapshot(now, &self.mobility.store, &self.agents);
            }

            self.clock.advance();
        }
        observer.on_sim_end(self.clock.current_tick);
        Ok(())
    }

    /// Run exactly `n` ticks from the current position (ignores `end_tick`).
    ///
    /// Useful for tests and incremental stepping.
    pub fn run_ticks<O: SimObserver>(&mut self, n: u64, observer: &mut O) -> SimResult<()> {
        for _ in 0..n {
            let now = self.clock.current_tick;
            observer.on_tick_start(now);
            let woken = self.process_tick(now)?;
            observer.on_tick_end(now, woken);
            if self.config.output_interval_ticks > 0
                && now.0.is_multiple_of(self.config.output_interval_ticks)
            {
                observer.on_snapshot(now, &self.mobility.store, &self.agents);
            }
            self.clock.advance();
        }
        Ok(())
    }

    // ── Core tick processing ──────────────────────────────────────────────

    fn process_tick(&mut self, now: Tick) -> SimResult<usize> {
        // ── Phase 0: process mobility arrivals ────────────────────────────
        //
        // Agents that arrive this tick are marked stationary and re-inserted
        // into the wake queue so they can re-plan from their new position.
        let arrived: Vec<(AgentId, _)> = self.mobility.tick_arrivals(now);
        for (agent, _dest) in arrived {
            if let Some(wake) = self.plans[agent.index()].next_wake_tick(now) {
                self.wake_queue.push(wake, agent);
            }
        }

        // ── Phase 1: drain the wake queue ─────────────────────────────────
        let woken = match self.wake_queue.drain_tick(now) {
            None    => return Ok(0),
            Some(w) => w,
        };
        let woken_count = woken.len();

        // ── Phase 2: build spatial contact index ──────────────────────────
        //
        // O(N) scan of all agent positions → NodeId → Vec<AgentId>.
        // Only stationary, placed agents are included.  Built once per tick
        // and reused for all woken agents' contact lookups.
        let contact_index = build_contact_index(&self.mobility.store);

        // ── Phase 3: pre-collect per-agent inputs (sequential) ────────────
        //
        // Drain each woken agent's pending messages and build their contact
        // list BEFORE the intent phase so the intent phase (which may run in
        // parallel) only reads immutable data.
        //
        // Messages sent *this tick* (during the apply phase below) will be
        // delivered at the recipient's *next* wake — not this one.
        let inputs: Vec<AgentInputs> = woken
            .iter()
            .map(|&agent| {
                let messages = self.message_queue.remove(&agent).unwrap_or_default();
                let contacts = contacts_for_agent(
                    agent, &contact_index, &self.mobility.store, now,
                );
                AgentInputs { messages, contacts }
            })
            .collect();

        // ── Phase 4: intent phase (produce) ───────────────────────────────
        let intents = self.compute_intents(now, &woken, inputs);

        // ── Phase 5: apply phase (consume) ────────────────────────────────
        //
        // Intents arrive in ascending AgentId order (BTreeMap drain).
        // Sequential application in this order makes results deterministic
        // even when the intent phase ran in parallel.
        for (agent, agent_intents) in intents {
            self.apply_intents(agent, agent_intents, now)?;
        }

        Ok(woken_count)
    }

    /// Compute intents for all woken agents.
    ///
    /// Calls `replan`, `on_message`, and `on_contacts` for each agent.
    /// With the `parallel` Cargo feature, all three calls run on Rayon's
    /// thread pool.
    fn compute_intents(
        &mut self,
        now:    Tick,
        woken:  &[AgentId],
        inputs: Vec<AgentInputs>,
    ) -> Vec<(AgentId, Vec<Intent>)> {
        // Explicit field borrows so the borrow checker sees disjoint access.
        let agents   = &self.agents;
        let plans    = self.plans.as_slice();
        let tick_dur = self.config.tick_duration_secs;
        let behavior = &self.behavior;
        let rngs     = &mut self.rngs;

        let ctx = SimContext::new(now, tick_dur, agents, plans);

        #[cfg(not(feature = "parallel"))]
        {
            woken
                .iter()
                .zip(inputs)
                .map(|(&agent, input)| {
                    let rng = rngs.get_mut(agent);
                    let mut intents = behavior.replan(agent, &ctx, rng);

                    for (from, payload) in input.messages {
                        intents.extend(behavior.on_message(agent, from, &payload, &ctx, rng));
                    }
                    if !input.contacts.is_empty() {
                        intents.extend(behavior.on_contacts(agent, &input.contacts, &ctx, rng));
                    }

                    (agent, intents)
                })
                .collect()
        }

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;

            // `get_many_mut` returns disjoint &mut refs indexed by unique AgentIds.
            // SAFETY precondition: woken list has unique IDs (BTreeMap drain).
            let rng_refs = rngs.get_many_mut(woken);

            woken
                .par_iter()
                .zip(rng_refs.into_par_iter())
                .zip(inputs.into_par_iter())
                .map(|((&agent, rng), input)| {
                    let mut intents = behavior.replan(agent, &ctx, rng);

                    for (from, payload) in input.messages {
                        intents.extend(behavior.on_message(agent, from, &payload, &ctx, rng));
                    }
                    if !input.contacts.is_empty() {
                        intents.extend(behavior.on_contacts(agent, &input.contacts, &ctx, rng));
                    }

                    (agent, intents)
                })
                .collect()
        }
    }

    /// Apply a single agent's intents during the sequential write phase.
    fn apply_intents(
        &mut self,
        agent:   AgentId,
        intents: Vec<Intent>,
        now:     Tick,
    ) -> SimResult<()> {
        for intent in intents {
            match intent {
                // ── WakeAt: re-insert agent into wake queue ────────────────
                Intent::WakeAt(tick) => {
                    if tick > now {
                        self.wake_queue.push(tick, agent);
                    }
                    // Silently ignore WakeAt(tick <= now) to prevent infinite
                    // loops from badly-written behavior models.
                }

                // ── TravelTo: start a journey via the mobility engine ──────
                Intent::TravelTo { destination, mode } => {
                    match self.mobility.begin_travel(
                        agent,
                        destination,
                        mode,
                        now,
                        self.config.tick_duration_secs,
                        &self.network,
                    ) {
                        Ok(arrival_tick) => {
                            self.wake_queue.push(arrival_tick, agent);
                        }
                        Err(_e) => {
                            // Routing failure is non-fatal: agent stays put.
                            // TODO: surface via a dedicated observer hook.
                        }
                    }
                }

                // ── SendMessage: store for recipient's next wake ───────────
                //
                // Messages are buffered here and delivered (via on_message)
                // the next time the recipient is woken.  The recipient is NOT
                // auto-woken; they receive the message at their natural next
                // wake tick (from their plan or a prior WakeAt intent).
                Intent::SendMessage { to, payload } => {
                    self.message_queue
                        .entry(to)
                        .or_default()
                        .push((agent, payload));
                }
            }
        }
        Ok(())
    }
}

// ── Contact index helpers ─────────────────────────────────────────────────────

/// Build a `NodeId → Vec<AgentId>` index of all stationary, placed agents.
///
/// In-transit agents and agents at `NodeId::INVALID` are excluded.
/// Time complexity: O(agent_count).
fn build_contact_index(store: &MobilityStore) -> HashMap<NodeId, Vec<AgentId>> {
    let mut index: HashMap<NodeId, Vec<AgentId>> = HashMap::new();
    for (i, state) in store.states.iter().enumerate() {
        if !state.in_transit && state.departure_node != NodeId::INVALID {
            index
                .entry(state.departure_node)
                .or_default()
                .push(AgentId(i as u32));
        }
    }
    index
}

/// Return all agents co-located with `agent` at its current node.
///
/// Returns an empty vec if `agent` is in transit, unplaced, or alone.
fn contacts_for_agent(
    agent:  AgentId,
    index:  &HashMap<NodeId, Vec<AgentId>>,
    store:  &MobilityStore,
    now:    Tick,
) -> Vec<ContactEvent> {
    let state = &store.states[agent.index()];
    if state.in_transit || state.departure_node == NodeId::INVALID {
        return vec![];
    }
    let node = state.departure_node;
    match index.get(&node) {
        None => vec![],
        Some(agents_at_node) => agents_at_node
            .iter()
            .filter(|&&other| other != agent)
            .map(|&other| ContactEvent {
                other,
                location: node,
                tick:     now,
                kind:     ContactKind::Node,
            })
            .collect(),
    }
}
