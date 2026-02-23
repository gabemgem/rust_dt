//! Unit tests for dt-behavior.

use dt_agent::{AgentStore, AgentStoreBuilder};
use dt_core::{AgentId, AgentRng, NodeId, Tick, TransportMode};
use dt_schedule::ActivityPlan;

use crate::{
    BehaviorModel, Intent, NoopBehavior, SimContext,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_context<'a>(store: &'a AgentStore, plans: &'a [ActivityPlan]) -> SimContext<'a> {
    SimContext::new(Tick(0), 3600, store, plans)
}

fn make_store(n: usize) -> AgentStore {
    let (store, _rngs) = AgentStoreBuilder::new(n, 0).build();
    store
}

// ── Intent ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod intent_tests {
    use dt_core::NodeId;

    use super::*;

    #[test]
    fn travel_to_fields() {
        let intent = Intent::TravelTo {
            destination: NodeId(7),
            mode:        TransportMode::Car,
        };
        match intent {
            Intent::TravelTo { destination, mode } => {
                assert_eq!(destination, NodeId(7));
                assert_eq!(mode, TransportMode::Car);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn wake_at() {
        let intent = Intent::WakeAt(Tick(42));
        assert_eq!(intent, Intent::WakeAt(Tick(42)));
    }

    #[test]
    fn send_message() {
        let intent = Intent::SendMessage {
            to:      AgentId(3),
            payload: vec![1, 2, 3],
        };
        match intent {
            Intent::SendMessage { to, payload } => {
                assert_eq!(to, AgentId(3));
                assert_eq!(payload, vec![1, 2, 3]);
            }
            _ => panic!("wrong variant"),
        }
    }
}

// ── SimContext ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn fields_accessible() {
        let store = make_store(2);
        let plans = vec![ActivityPlan::empty(), ActivityPlan::empty()];
        let ctx = make_context(&store, &plans);
        assert_eq!(ctx.tick, Tick(0));
        assert_eq!(ctx.tick_duration_secs, 3600);
        assert_eq!(ctx.agents.count, 2);
        assert_eq!(ctx.plans.len(), 2);
    }
}

// ── NoopBehavior ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod noop_tests {
    use super::*;

    #[test]
    fn replan_returns_empty() {
        let store = make_store(1);
        let plans = vec![ActivityPlan::empty()];
        let ctx = make_context(&store, &plans);
        let mut rng = AgentRng::new(0, AgentId(0));
        let intents = NoopBehavior.replan(AgentId(0), &ctx, &mut rng);
        assert!(intents.is_empty());
    }

    #[test]
    fn on_contacts_returns_empty() {
        let store = make_store(1);
        let plans = vec![ActivityPlan::empty()];
        let ctx = make_context(&store, &plans);
        let mut rng = AgentRng::new(0, AgentId(0));
        let intents = NoopBehavior.on_contacts(AgentId(0), NodeId(0), &[], &ctx, &mut rng);
        assert!(intents.is_empty());
    }

    #[test]
    fn on_message_returns_empty() {
        let store = make_store(1);
        let plans = vec![ActivityPlan::empty()];
        let ctx = make_context(&store, &plans);
        let mut rng = AgentRng::new(0, AgentId(0));
        let intents = NoopBehavior.on_message(AgentId(0), AgentId(1), b"hello", &ctx, &mut rng);
        assert!(intents.is_empty());
    }
}

// ── Custom BehaviorModel ──────────────────────────────────────────────────────

#[cfg(test)]
mod custom_model_tests {
    use dt_core::NodeId;

    use super::*;

    /// A behavior that always wants to travel to node 99.
    struct AlwaysTravel;

    impl BehaviorModel for AlwaysTravel {
        fn replan(
            &self,
            _agent: AgentId,
            _ctx:   &SimContext<'_>,
            _rng:   &mut AgentRng,
        ) -> Vec<Intent> {
            vec![Intent::TravelTo {
                destination: NodeId(99),
                mode:        TransportMode::Walk,
            }]
        }
    }

    #[test]
    fn custom_model_returns_intent() {
        let store = make_store(1);
        let plans = vec![ActivityPlan::empty()];
        let ctx = make_context(&store, &plans);
        let mut rng = AgentRng::new(0, AgentId(0));
        let intents = AlwaysTravel.replan(AgentId(0), &ctx, &mut rng);
        assert_eq!(intents.len(), 1);
        assert!(matches!(
            intents[0],
            Intent::TravelTo { destination: NodeId(99), mode: TransportMode::Walk }
        ));
    }

    #[test]
    fn model_is_object_safe_via_box() {
        // Verify BehaviorModel can be used as a trait object.
        let model: Box<dyn BehaviorModel> = Box::new(AlwaysTravel);
        let store = make_store(1);
        let plans = vec![ActivityPlan::empty()];
        let ctx = make_context(&store, &plans);
        let mut rng = AgentRng::new(0, AgentId(0));
        let intents = model.replan(AgentId(0), &ctx, &mut rng);
        assert_eq!(intents.len(), 1);
    }
}
