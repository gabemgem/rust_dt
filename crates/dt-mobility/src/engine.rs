//! High-level mobility engine: routes `TravelTo` intents and advances agents.

use dt_core::{AgentId, NodeId, Tick, TransportMode};
use dt_spatial::{RoadNetwork, Router};

use crate::{MobilityError, MobilityStore, MovementState};

/// Wraps a [`Router`] and [`MobilityStore`] to provide a simple intent-driven
/// mobility API used by dt-sim.
///
/// # Type parameter
///
/// `R` must implement [`Router`] (e.g. [`dt_spatial::DijkstraRouter`]).
/// Swap it at compile time for a different routing algorithm with no runtime
/// overhead.
pub struct MobilityEngine<R: Router> {
    /// The routing algorithm.
    pub router: R,

    /// All per-agent movement state and route cache.
    pub store: MobilityStore,
}

impl<R: Router> MobilityEngine<R> {
    /// Create a new engine with all agents stationary at `NodeId::INVALID`.
    pub fn new(router: R, agent_count: usize) -> Self {
        Self {
            router,
            store: MobilityStore::new(agent_count),
        }
    }

    /// Teleport `agent` to `node` without routing (initial placement).
    pub fn place(&mut self, agent: AgentId, node: NodeId, tick: Tick) {
        self.store.states[agent.index()] = MovementState::stationary(node, tick);
    }

    /// Start `agent` travelling to `destination`.
    ///
    /// Looks up the agent's current node, computes a route via `router`, and
    /// records the movement in the store.  Returns the `arrival_tick` to be
    /// inserted into the `WakeQueue`, or an error if routing fails or the
    /// agent is already in transit.
    pub fn begin_travel(
        &mut self,
        agent:              AgentId,
        destination:        NodeId,
        mode:               TransportMode,
        now:                Tick,
        tick_duration_secs: u32,
        network:            &RoadNetwork,
    ) -> Result<Tick, MobilityError> {
        let state = &self.store.states[agent.index()];
        if state.in_transit {
            return Err(MobilityError::AlreadyInTransit(agent));
        }
        let from = state.departure_node;
        if from == NodeId::INVALID {
            return Err(MobilityError::NotPlaced(agent));
        }

        // Split borrow: borrow router and store as separate fields.
        let router  = &self.router;
        self.store
            .begin_travel(agent, from, destination, mode, now, tick_duration_secs, router, network)
            .map_err(MobilityError::Routing)
    }

    /// Advance all agents whose `arrival_tick <= now`.
    ///
    /// Returns `(AgentId, NodeId)` for every agent that arrived this tick so
    /// the caller can update `AgentStore.node_id` and re-insert them into the
    /// `WakeQueue`.
    pub fn tick_arrivals(&mut self, now: Tick) -> Vec<(AgentId, NodeId)> {
        // Collect arriving agents first (immutable scan) then mutate.
        let arriving: Vec<AgentId> = self.store.states
            .iter()
            .enumerate()
            .filter(|(_, s)| s.in_transit && s.arrival_tick <= now)
            .map(|(i, _)| AgentId(i as u32))
            .collect();

        arriving
            .into_iter()
            .map(|agent| {
                let dest = self.store.arrive(agent, now);
                (agent, dest)
            })
            .collect()
    }

    /// Interpolated visual position for `agent` at `now`.
    ///
    /// Returns `(departure_node, destination_node, progress)` where `progress`
    /// is in `[0.0, 1.0]`.  Visualization tools blend between the two nodes'
    /// `GeoPoint`s using this fraction.
    pub fn visual_position(&self, agent: AgentId, now: Tick) -> (NodeId, NodeId, f32) {
        let state = &self.store.states[agent.index()];
        (state.departure_node, state.destination_node, state.progress(now))
    }
}
