//! Per-agent movement state.

use dt_core::{NodeId, Tick};

/// The movement state for a single agent.
///
/// An agent is either **stationary** (at a node, `in_transit = false`) or
/// **in transit** (travelling between two nodes, `in_transit = true`).
///
/// The simulation uses a **teleport-at-arrival** model: the agent logically
/// stays at `departure_node` until `arrival_tick`, then instantly appears at
/// `destination_node`.  The stored route allows visualization tools to
/// interpolate a smooth position between ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovementState {
    /// `true` while the agent is travelling to `destination_node`.
    pub in_transit: bool,

    /// The node the agent departed from (or is currently at if `!in_transit`).
    pub departure_node: NodeId,

    /// The node the agent is heading to.  Equals `departure_node` when
    /// `!in_transit`.
    pub destination_node: NodeId,

    /// Tick at which the journey began.  Equals `arrival_tick` when
    /// `!in_transit`.
    pub departure_tick: Tick,

    /// Tick at which the agent will arrive at `destination_node`.  Equals
    /// `departure_tick` when `!in_transit`.
    pub arrival_tick: Tick,
}

impl MovementState {
    /// Construct a stationary state at `node` at time `tick`.
    #[inline]
    pub fn stationary(node: NodeId, tick: Tick) -> Self {
        Self {
            in_transit:       false,
            departure_node:   node,
            destination_node: node,
            departure_tick:   tick,
            arrival_tick:     tick,
        }
    }

    /// Fraction of the journey completed at `now`, in `[0.0, 1.0]`.
    ///
    /// Returns `1.0` for stationary agents or when `now >= arrival_tick`.
    pub fn progress(&self, now: Tick) -> f32 {
        if !self.in_transit || self.arrival_tick <= self.departure_tick {
            return 1.0;
        }
        let elapsed = now.0.saturating_sub(self.departure_tick.0) as f32;
        let total   = (self.arrival_tick.0 - self.departure_tick.0) as f32;
        (elapsed / total).min(1.0)
    }
}
