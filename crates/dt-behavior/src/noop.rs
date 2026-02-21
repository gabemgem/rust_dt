//! A no-op behavior model — agents never produce intents.

use dt_core::{AgentId, AgentRng};

use crate::{BehaviorModel, Intent, SimContext};

/// A [`BehaviorModel`] that always returns an empty intent list.
///
/// Useful as a placeholder in tests or for "passive" agent populations that
/// simply occupy space without acting.
pub struct NoopBehavior;

impl BehaviorModel for NoopBehavior {
    fn replan(
        &self,
        _agent: AgentId,
        _ctx:   &SimContext<'_>,
        _rng:   &mut AgentRng,
    ) -> Vec<Intent> {
        vec![]
    }
}
