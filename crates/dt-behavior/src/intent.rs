//! Agent intents — the actions an agent can request during replanning.

use dt_core::{AgentId, NodeId, Tick, TransportMode};

/// An action that an agent wants to perform during the current tick.
///
/// Intents are produced by [`BehaviorModel::replan`][crate::BehaviorModel::replan]
/// and consumed by the simulation loop (dt-sim) and mobility engine (dt-mobility).
///
/// Multiple intents may be returned per agent per tick; the caller is
/// responsible for resolving any conflicts (e.g. two `TravelTo` requests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Agent wants to travel to `destination` via `mode`.
    ///
    /// dt-mobility will compute a route and record an `arrival_tick`.
    TravelTo {
        destination: NodeId,
        mode:        TransportMode,
    },

    /// Agent wants to be woken again at `tick` for re-planning.
    ///
    /// Inserted into the `WakeQueue` by the simulation loop.
    WakeAt(Tick),

    /// Agent wants to deliver a message to `to`.
    ///
    /// The simulation loop routes it to `BehaviorModel::on_message` on the
    /// recipient's next wake tick.
    SendMessage {
        to:      AgentId,
        payload: Vec<u8>,
    },
}
