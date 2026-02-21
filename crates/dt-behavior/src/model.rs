//! The `BehaviorModel` trait — the main extension point for user code.

use dt_core::{AgentId, AgentRng};

use crate::{ContactEvent, Intent, SimContext};

/// Pluggable agent behavior.
///
/// Implement this trait to define how agents decide what to do each tick.
/// All methods receive a read-only [`SimContext`] and a mutable per-agent
/// [`AgentRng`] so behavior is deterministic regardless of thread ordering.
///
/// # Required methods
///
/// Only [`replan`][Self::replan] is required.  The contact and message hooks
/// have no-op defaults so simple models don't need to implement them.
///
/// # Thread safety
///
/// The simulation loop may call `replan` for many agents in parallel via
/// Rayon, so implementations must be `Send + Sync`.  State that varies per
/// agent must live in `AgentStore` (accessed read-only through `ctx.agents`),
/// not in the model itself.
///
/// # Example
///
/// ```rust,ignore
/// struct FollowSchedule;
///
/// impl BehaviorModel for FollowSchedule {
///     fn replan(&self, agent: AgentId, ctx: &SimContext, rng: &mut AgentRng) -> Vec<Intent> {
///         let plan = &ctx.plans[agent.index()];
///         match plan.current_activity(ctx.tick) {
///             Some(act) => vec![Intent::TravelTo {
///                 destination: act.destination.node_id().unwrap_or_default(),
///                 mode: TransportMode::Car,
///             }],
///             None => vec![],
///         }
///     }
/// }
/// ```
pub trait BehaviorModel: Send + Sync + 'static {
    /// Called once per agent per tick when the agent wakes.
    ///
    /// Return a list of [`Intent`]s describing what the agent wants to do.
    /// An empty `Vec` means "do nothing"; the agent remains at its current
    /// location until it is woken again.
    fn replan(
        &self,
        agent: AgentId,
        ctx:   &SimContext<'_>,
        rng:   &mut AgentRng,
    ) -> Vec<Intent>;

    /// Called when one or more contacts were observed for this agent.
    ///
    /// Default: returns no intents (contacts are ignored).
    fn on_contacts(
        &self,
        _agent:    AgentId,
        _contacts: &[ContactEvent],
        _ctx:      &SimContext<'_>,
        _rng:      &mut AgentRng,
    ) -> Vec<Intent> {
        vec![]
    }

    /// Called when another agent sent this agent a message via
    /// [`Intent::SendMessage`].
    ///
    /// Default: returns no intents (messages are ignored).
    fn on_message(
        &self,
        _agent:   AgentId,
        _from:    AgentId,
        _payload: &[u8],
        _ctx:     &SimContext<'_>,
        _rng:     &mut AgentRng,
    ) -> Vec<Intent> {
        vec![]
    }
}
