//! Read-only simulation state passed to every behavior callback.

use dt_agent::AgentStore;
use dt_core::Tick;
use dt_schedule::ActivityPlan;

/// A read-only snapshot of the simulation state passed to every
/// [`BehaviorModel`][crate::BehaviorModel] callback.
///
/// `SimContext` is built once per tick by dt-sim and shared (immutably) across
/// all agent callbacks during the intent phase.  No heap allocation happens
/// between ticks: the same struct is reused and its `tick` field advanced.
///
/// # Lifetimes
///
/// All borrows live for the duration of one tick's intent phase.  dt-sim
/// never allows mutable access to these structures while `SimContext` is live.
pub struct SimContext<'a> {
    /// Current simulation tick.
    pub tick: Tick,

    /// How many wall-clock seconds one tick represents.
    ///
    /// Useful for computing durations: `n_ticks * tick_duration_secs`.
    pub tick_duration_secs: u32,

    /// Read-only view of every agent's SoA state arrays.
    pub agents: &'a AgentStore,

    /// Per-agent activity plans, indexed by `AgentId`.
    ///
    /// `plans[agent.index()]` is the plan for that agent; absent agents have
    /// `ActivityPlan::empty()`.
    pub plans: &'a [ActivityPlan],
}

impl<'a> SimContext<'a> {
    /// Build a new context for a single tick.
    #[inline]
    pub fn new(
        tick:               Tick,
        tick_duration_secs: u32,
        agents:             &'a AgentStore,
        plans:              &'a [ActivityPlan],
    ) -> Self {
        Self { tick, tick_duration_secs, agents, plans }
    }
}
