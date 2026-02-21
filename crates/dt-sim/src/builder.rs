//! Fluent builder for constructing a [`Sim`].

use std::collections::HashMap;

use dt_agent::{AgentRngs, AgentStore};
use dt_behavior::BehaviorModel;
use dt_core::{AgentId, NodeId, Tick, SimConfig};
use dt_mobility::MobilityEngine;
use dt_schedule::{ActivityPlan, WakeQueue};
use dt_spatial::{RoadNetwork, Router};

use crate::{Sim, SimError, SimResult};

/// Fluent builder for [`Sim<B, R>`].
///
/// # Required inputs
///
/// - [`SimConfig`] — total ticks, seed, tick duration, …
/// - [`AgentStore`] + [`AgentRngs`] — from [`dt_agent::AgentStoreBuilder`]
/// - `B: BehaviorModel` — the behavior implementation
/// - `R: Router` — the routing algorithm (e.g. [`dt_spatial::DijkstraRouter`])
///
/// # Optional inputs (have defaults)
///
/// | Method                   | Default                     |
/// |--------------------------|-----------------------------|
/// | `.plans(v)`              | All-empty `ActivityPlan`s   |
/// | `.network(n)`            | `RoadNetwork::empty()`      |
/// | `.initial_positions(v)`  | All `NodeId::INVALID`       |
///
/// # Example
///
/// ```rust,ignore
/// let (store, rngs) = AgentStoreBuilder::new(n, seed).build();
/// let mut sim = SimBuilder::new(config, store, rngs, NoopBehavior, DijkstraRouter)
///     .plans(plans)
///     .network(network)
///     .initial_positions(positions)
///     .build()?;
/// sim.run(&mut NoopObserver)?;
/// ```
pub struct SimBuilder<B: BehaviorModel, R: Router> {
    config:    SimConfig,
    agents:    AgentStore,
    rngs:      AgentRngs,
    plans:     Option<Vec<ActivityPlan>>,
    network:   Option<RoadNetwork>,
    positions: Option<Vec<NodeId>>,
    behavior:  B,
    router:    R,
}

impl<B: BehaviorModel, R: Router> SimBuilder<B, R> {
    /// Create a builder with all required inputs.
    pub fn new(
        config:   SimConfig,
        agents:   AgentStore,
        rngs:     AgentRngs,
        behavior: B,
        router:   R,
    ) -> Self {
        Self {
            config,
            agents,
            rngs,
            plans:     None,
            network:   None,
            positions: None,
            behavior,
            router,
        }
    }

    /// Supply per-agent activity plans (must be length `agent_count`).
    ///
    /// If not called, all agents get `ActivityPlan::empty()` and are never
    /// auto-woken by the schedule — you must use `Intent::WakeAt` instead.
    pub fn plans(mut self, plans: Vec<ActivityPlan>) -> Self {
        self.plans = Some(plans);
        self
    }

    /// Supply the road network used for routing `TravelTo` intents.
    ///
    /// If not called, an empty network is used; any `TravelTo` intent will
    /// fail with a routing error (non-fatal: the agent simply stays put).
    pub fn network(mut self, network: RoadNetwork) -> Self {
        self.network = Some(network);
        self
    }

    /// Supply the initial position (road node) for each agent.
    ///
    /// Must be length `agent_count`.  Agents with `NodeId::INVALID` are not
    /// placed on the network and will fail `TravelTo` until a behavior model
    /// places them elsewhere.
    pub fn initial_positions(mut self, positions: Vec<NodeId>) -> Self {
        self.positions = Some(positions);
        self
    }

    /// Validate inputs, build the wake queue and mobility engine, and return
    /// a ready-to-run [`Sim`].
    pub fn build(self) -> SimResult<Sim<B, R>> {
        let agent_count = self.agents.count;

        // ── Validate and resolve optional inputs ──────────────────────────
        let plans = match self.plans {
            Some(p) => {
                if p.len() != agent_count {
                    return Err(SimError::AgentCountMismatch {
                        expected: agent_count,
                        got:      p.len(),
                        what:     "activity plans",
                    });
                }
                p
            }
            None => vec![ActivityPlan::empty(); agent_count],
        };

        let positions = match self.positions {
            Some(p) => {
                if p.len() != agent_count {
                    return Err(SimError::AgentCountMismatch {
                        expected: agent_count,
                        got:      p.len(),
                        what:     "initial positions",
                    });
                }
                p
            }
            None => vec![NodeId::INVALID; agent_count],
        };

        let network = self.network.unwrap_or_else(RoadNetwork::empty);

        // ── Build initial wake queue from plans ───────────────────────────
        let wake_queue = WakeQueue::build_from_plans(&plans, Tick(0));

        // ── Build mobility engine and place agents ────────────────────────
        let mut mobility = MobilityEngine::new(self.router, agent_count);
        for (i, &node) in positions.iter().enumerate() {
            if node != NodeId::INVALID {
                mobility.place(AgentId(i as u32), node, Tick(0));
            }
        }

        Ok(Sim {
            clock:         self.config.make_clock(),
            config:        self.config,
            agents:        self.agents,
            rngs:          self.rngs,
            plans,
            wake_queue,
            mobility,
            behavior:      self.behavior,
            network,
            message_queue: HashMap::new(),
        })
    }
}
