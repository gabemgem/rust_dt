//! Plain data row types written by output backends.

/// A snapshot of one agent's mobility state at a given tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSnapshotRow {
    pub agent_id:         u32,
    pub tick:             u64,
    /// The node the agent is at (or departed from if in transit).
    /// `u32::MAX` means the agent has never been placed on the network.
    pub departure_node:   u32,
    pub in_transit:       bool,
    /// Destination node while in transit; `u32::MAX` if stationary.
    pub destination_node: u32,
}

/// Summary statistics for one simulation tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickSummaryRow {
    pub tick:           u64,
    pub unix_time_secs: i64,
    pub woken_agents:   u64,
}
