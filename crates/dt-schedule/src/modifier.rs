//! `ScheduleModifier` — hook for stochastic schedule deviations.
//!
//! The framework calls `ScheduleModifier::modify` each time an agent finishes
//! an activity and is about to start the next planned one.  Returning `Some`
//! replaces the planned activity; returning `None` keeps it as-is.
//!
//! # Examples of application-defined modifiers
//!
//! - **Detour**: replace `AtWork` with a short `Shopping` stop first.
//! - **Skip**: occasionally skip `Errand` and go straight home.
//! - **Visit friend**: insert an unplanned `SocialVisit` with a sampled NodeId.
//! - **Late departure**: delay the start of `AtWork` by 1–3 ticks.
//!
//! Modifiers are designed to be *composable*: chain them with
//! [`ChainedModifier`] to combine multiple independent stochastic rules.

use dt_core::{AgentId, AgentRng};

use crate::ScheduledActivity;

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Hook called when an agent finishes an activity and is about to execute
/// the next one from its plan.
///
/// # Contract
///
/// - Must be deterministic given the same `rng` state.
/// - Must not block or perform I/O.
/// - Implementations must be `Send + Sync` (shared across Rayon threads).
pub trait ScheduleModifier: Send + Sync {
    /// Optionally replace `planned` with a modified activity.
    ///
    /// - Return `Some(activity)` to substitute the planned activity.
    /// - Return `None` to execute `planned` as-is.
    fn modify(
        &self,
        agent: AgentId,
        planned: &ScheduledActivity,
        rng: &mut AgentRng,
    ) -> Option<ScheduledActivity>;
}

// ── No-op ─────────────────────────────────────────────────────────────────────

/// A modifier that never alters the planned schedule.
///
/// Use this as the default when no stochastic deviations are needed.
pub struct NoModification;

impl ScheduleModifier for NoModification {
    #[inline]
    fn modify(
        &self,
        _agent: AgentId,
        _planned: &ScheduledActivity,
        _rng: &mut AgentRng,
    ) -> Option<ScheduledActivity> {
        None
    }
}

// ── Chained modifier ──────────────────────────────────────────────────────────

/// Applies two modifiers in sequence.
///
/// The second modifier sees the (possibly modified) output of the first.
/// Construct chains with `modifier_a.then(modifier_b)`.
pub struct ChainedModifier<A: ScheduleModifier, B: ScheduleModifier> {
    first:  A,
    second: B,
}

impl<A: ScheduleModifier, B: ScheduleModifier> ScheduleModifier for ChainedModifier<A, B> {
    fn modify(
        &self,
        agent: AgentId,
        planned: &ScheduledActivity,
        rng: &mut AgentRng,
    ) -> Option<ScheduledActivity> {
        let after_first = self.first.modify(agent, planned, rng);
        let candidate = after_first.as_ref().unwrap_or(planned);
        self.second
            .modify(agent, candidate, rng)
            .or(after_first)
    }
}

/// Extension trait that adds `.then(other)` to any `ScheduleModifier`.
pub trait ScheduleModifierExt: ScheduleModifier + Sized {
    fn then<B: ScheduleModifier>(self, other: B) -> ChainedModifier<Self, B> {
        ChainedModifier { first: self, second: other }
    }
}

impl<M: ScheduleModifier + Sized> ScheduleModifierExt for M {}
