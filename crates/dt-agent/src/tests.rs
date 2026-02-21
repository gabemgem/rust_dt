//! Unit tests for dt-agent.

#[cfg(test)]
mod component_map {
    use crate::ComponentMap;

    #[derive(Default, PartialEq, Debug)]
    struct Health(f32);

    #[derive(Default, PartialEq, Debug)]
    struct Age(u8);

    #[test]
    fn register_and_get() {
        let mut map = ComponentMap::new();
        map.register::<Health>(3);
        let slice = map.get::<Health>().expect("Health should be registered");
        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0], Health(0.0));
    }

    #[test]
    fn double_register_is_noop() {
        let mut map = ComponentMap::new();
        map.register::<Health>(2);
        // Manually set a value so we can verify it survives the second register.
        map.get_mut::<Health>().unwrap()[0] = Health(1.5);
        map.register::<Health>(99); // should not touch existing data
        assert_eq!(map.get::<Health>().unwrap()[0], Health(1.5));
        assert_eq!(map.get::<Health>().unwrap().len(), 2);
    }

    #[test]
    fn unregistered_returns_none() {
        let map = ComponentMap::new();
        assert!(map.get::<Health>().is_none());
    }

    #[test]
    fn get_mut_and_write() {
        let mut map = ComponentMap::new();
        map.register::<Age>(5);
        map.get_mut::<Age>().unwrap()[2] = Age(30);
        assert_eq!(map.get::<Age>().unwrap()[2], Age(30));
    }

    #[test]
    fn two_types_do_not_interfere() {
        let mut map = ComponentMap::new();
        map.register::<Health>(2);
        map.register::<Age>(2);
        assert_eq!(map.type_count(), 2);
        assert!(map.contains::<Health>());
        assert!(map.contains::<Age>());
        // Writing one type doesn't corrupt the other.
        map.get_mut::<Health>().unwrap()[0] = Health(0.9);
        assert_eq!(map.get::<Age>().unwrap()[0], Age(0));
    }

    #[test]
    fn push_defaults_grows_all_types() {
        let mut map = ComponentMap::new();
        map.register::<Health>(0);
        map.register::<Age>(0);
        assert_eq!(map.get::<Health>().unwrap().len(), 0);
        map.push_defaults();
        map.push_defaults();
        assert_eq!(map.get::<Health>().unwrap().len(), 2);
        assert_eq!(map.get::<Age>().unwrap().len(), 2);
    }
}

#[cfg(test)]
mod builder {
    use crate::AgentStoreBuilder;

    #[derive(Default)]
    struct Infected(bool);

    #[test]
    fn correct_count() {
        let (store, rngs) = AgentStoreBuilder::new(500, 1).build();
        assert_eq!(store.count, 500);
        assert_eq!(rngs.len(), 500);
    }

    #[test]
    fn zero_agents() {
        let (store, rngs) = AgentStoreBuilder::new(0, 0).build();
        assert!(store.is_empty());
        assert!(rngs.is_empty());
    }

    #[test]
    fn component_prefilled_with_defaults() {
        let (store, _) = AgentStoreBuilder::new(10, 0)
            .register_component::<Infected>()
            .build();
        let slice = store.component::<Infected>().expect("Infected registered");
        assert_eq!(slice.len(), 10);
        assert!(!slice[0].0); // Default is false
    }

    #[test]
    fn unregistered_component_returns_none() {
        let (store, _) = AgentStoreBuilder::new(5, 0).build();
        assert!(store.component::<Infected>().is_none());
    }

    #[test]
    fn component_mut_allows_write() {
        let (mut store, _) = AgentStoreBuilder::new(4, 0)
            .register_component::<Infected>()
            .build();
        store.component_mut::<Infected>().unwrap()[2] = Infected(true);
        assert!(store.component::<Infected>().unwrap()[2].0);
    }
}

#[cfg(test)]
mod store {
    use crate::AgentStoreBuilder;
    use dt_core::AgentId;

    #[test]
    fn agent_ids_iterator() {
        let (store, _) = AgentStoreBuilder::new(5, 0).build();
        let ids: Vec<AgentId> = store.agent_ids().collect();
        assert_eq!(ids, vec![AgentId(0), AgentId(1), AgentId(2), AgentId(3), AgentId(4)]);
    }

    #[cfg(feature = "spatial")]
    #[test]
    fn spatial_sentinels() {
        use dt_core::{EdgeId, NodeId};
        let (store, _) = AgentStoreBuilder::new(3, 0).build();
        // All agents start at INVALID — not yet placed on the network.
        assert_eq!(store.node_id[0], NodeId::INVALID);
        assert_eq!(store.edge_id[0], EdgeId::INVALID);
        assert_eq!(store.edge_progress[0], 0.0);
        assert!(store.is_at_node(AgentId(0)));
        assert!(!store.is_moving(AgentId(0)));
    }

    #[cfg(feature = "spatial")]
    #[test]
    fn spatial_write() {
        use dt_core::{EdgeId, NodeId};
        let (mut store, _) = AgentStoreBuilder::new(2, 0).build();
        store.node_id[0] = NodeId(7);
        store.edge_id[1] = EdgeId(3);
        store.edge_progress[1] = 0.42;

        assert_eq!(store.node_id[0], NodeId(7));
        assert!(store.is_at_node(AgentId(0)));   // edge_id is still INVALID
        assert!(store.is_moving(AgentId(1)));    // edge_id is now valid
    }

    #[cfg(feature = "schedule")]
    #[test]
    fn schedule_sentinels() {
        use dt_core::{ActivityId, Tick};
        let (store, _) = AgentStoreBuilder::new(2, 0).build();
        assert_eq!(store.next_event_tick[0], Tick::ZERO);
        assert_eq!(store.current_activity[0], ActivityId::INVALID);
    }

    #[cfg(feature = "mobility")]
    #[test]
    fn mobility_sentinel() {
        use dt_core::TransportMode;
        let (store, _) = AgentStoreBuilder::new(2, 0).build();
        assert_eq!(store.transport_mode[0], TransportMode::None);
    }
}

#[cfg(test)]
mod rngs {
    use crate::AgentStoreBuilder;
    use dt_core::AgentId;

    #[test]
    fn per_agent_determinism() {
        let (_, mut rngs1) = AgentStoreBuilder::new(10, 999).build();
        let (_, mut rngs2) = AgentStoreBuilder::new(10, 999).build();
        for i in 0..10u32 {
            let a: f32 = rngs1.get_mut(AgentId(i)).random();
            let b: f32 = rngs2.get_mut(AgentId(i)).random();
            assert_eq!(a, b, "agent {i} RNG should be deterministic");
        }
    }

    #[test]
    fn different_seeds_differ() {
        let (_, mut rngs_a) = AgentStoreBuilder::new(1, 1).build();
        let (_, mut rngs_b) = AgentStoreBuilder::new(1, 2).build();
        let a: u64 = rngs_a.get_mut(AgentId(0)).random();
        let b: u64 = rngs_b.get_mut(AgentId(0)).random();
        assert_ne!(a, b);
    }

    #[test]
    fn adjacent_agents_differ() {
        let (_, mut rngs) = AgentStoreBuilder::new(2, 0).build();
        let a: u64 = rngs.get_mut(AgentId(0)).random();
        let b: u64 = rngs.get_mut(AgentId(1)).random();
        assert_ne!(a, b);
    }
}
