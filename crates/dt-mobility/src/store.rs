//! The `MobilityStore` — per-agent movement state and sparse route cache.

use std::collections::HashMap;

use dt_core::{AgentId, NodeId, Tick, TransportMode};
use dt_spatial::{Route, Router, SpatialError};

use crate::MovementState;

/// Holds movement state for every agent plus sparse routes for agents in
/// transit.
///
/// The `states` vector is indexed by `AgentId` and is always length
/// `agent_count`.  The `routes` map is sparse — only agents currently in
/// transit have an entry.  Routes are removed on arrival.
pub struct MobilityStore {
    /// Per-agent movement state, indexed by `AgentId`.
    pub states: Vec<MovementState>,

    /// Sparse route cache: `AgentId → Route` for agents currently in transit.
    pub routes: HashMap<AgentId, Route>,
}

impl MobilityStore {
    /// Create a store with all agents stationary at `NodeId::INVALID`, tick 0.
    pub fn new(agent_count: usize) -> Self {
        let invalid_state = MovementState::stationary(NodeId::INVALID, Tick(0));
        Self {
            states: vec![invalid_state; agent_count],
            routes: HashMap::new(),
        }
    }

    /// Begin travel for `agent` from `from` to `to` using `router`.
    ///
    /// Computes the route, sets `in_transit = true`, and stores the route in
    /// the sparse map.  Returns the `arrival_tick` so the caller can insert it
    /// into the `WakeQueue`.
    ///
    /// # Errors
    ///
    /// Returns `SpatialError` if the router cannot find a path.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_travel<R: Router>(
        &mut self,
        agent:              AgentId,
        from:               NodeId,
        to:                 NodeId,
        mode:               TransportMode,
        now:                Tick,
        tick_duration_secs: u32,
        router:             &R,
        network:            &dt_spatial::RoadNetwork,
    ) -> Result<Tick, SpatialError> {
        let route        = router.route(network, from, to, mode)?;
        let travel_ticks = route.travel_ticks(tick_duration_secs);
        let arrival_tick = Tick(now.0 + travel_ticks.max(1)); // arrive at least 1 tick later

        self.states[agent.index()] = MovementState {
            in_transit:       true,
            departure_node:   from,
            destination_node: to,
            departure_tick:   now,
            arrival_tick,
        };
        self.routes.insert(agent, route);

        Ok(arrival_tick)
    }

    /// Complete travel for `agent`, returning the destination node.
    ///
    /// Marks the agent as stationary at `destination_node` and removes the
    /// cached route.  Should be called when `now >= state.arrival_tick`.
    pub fn arrive(&mut self, agent: AgentId, now: Tick) -> NodeId {
        let dest = self.states[agent.index()].destination_node;
        self.states[agent.index()] = MovementState::stationary(dest, now);
        self.routes.remove(&agent);
        dest
    }

    /// Current progress fraction for `agent` at `now` (see
    /// [`MovementState::progress`]).
    #[inline]
    pub fn progress(&self, agent: AgentId, now: Tick) -> f32 {
        self.states[agent.index()].progress(now)
    }

    /// Returns `true` if `agent` is currently in transit.
    #[inline]
    pub fn in_transit(&self, agent: AgentId) -> bool {
        self.states[agent.index()].in_transit
    }
}
