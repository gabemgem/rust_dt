//! Unit tests for dt-mobility.

use dt_core::{AgentId, NodeId, Tick, TransportMode};
use dt_spatial::{DijkstraRouter, RoadNetwork, RoadNetworkBuilder, Router};

use crate::{MobilityEngine, MobilityStore, MovementState};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Two-node network: node 0 ↔ node 1, 1000 m, 120 s at ~30 km/h.
fn two_node_network() -> RoadNetwork {
    let mut b = RoadNetworkBuilder::new();
    let n0 = b.add_node(dt_core::GeoPoint { lat: 0.0, lon: 0.0 });
    let n1 = b.add_node(dt_core::GeoPoint { lat: 0.0, lon: 0.01 }); // ~1.1 km east
    b.add_road(n0, n1, 1000.0, 120_000);
    b.build()
}

/// Three-node network: 0 ↔ 1 ↔ 2, 500 m / 60 s per segment.
fn three_node_network() -> RoadNetwork {
    let mut b = RoadNetworkBuilder::new();
    let n0 = b.add_node(dt_core::GeoPoint { lat: 0.0,   lon: 0.0 });
    let n1 = b.add_node(dt_core::GeoPoint { lat: 0.005, lon: 0.0 });
    let n2 = b.add_node(dt_core::GeoPoint { lat: 0.01,  lon: 0.0 });
    b.add_road(n0, n1, 500.0, 60_000);
    b.add_road(n1, n2, 500.0, 60_000);
    b.build()
}

fn engine(agent_count: usize) -> MobilityEngine<DijkstraRouter> {
    MobilityEngine::new(DijkstraRouter, agent_count)
}

// ── MovementState ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod movement_state {
    use super::*;

    #[test]
    fn stationary_progress_is_one() {
        let s = MovementState::stationary(NodeId(0), Tick(10));
        assert_eq!(s.progress(Tick(10)), 1.0);
        assert_eq!(s.progress(Tick(99)), 1.0);
        assert!(!s.in_transit);
    }

    #[test]
    fn in_transit_progress_midpoint() {
        let s = MovementState {
            in_transit:       true,
            departure_node:   NodeId(0),
            destination_node: NodeId(1),
            departure_tick:   Tick(0),
            arrival_tick:     Tick(10),
        };
        assert!((s.progress(Tick(5)) - 0.5).abs() < 1e-6);
        assert_eq!(s.progress(Tick(0)),  0.0);
        assert_eq!(s.progress(Tick(10)), 1.0);
        assert_eq!(s.progress(Tick(15)), 1.0); // capped at 1.0
    }

    #[test]
    fn zero_duration_transit_progress_is_one() {
        let s = MovementState {
            in_transit:       true,
            departure_node:   NodeId(0),
            destination_node: NodeId(1),
            departure_tick:   Tick(5),
            arrival_tick:     Tick(5),
        };
        assert_eq!(s.progress(Tick(5)), 1.0);
    }
}

// ── MobilityStore ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod mobility_store {
    use super::*;

    #[test]
    fn new_all_stationary() {
        let store = MobilityStore::new(3);
        for i in 0..3 {
            assert!(!store.states[i].in_transit);
            assert_eq!(store.states[i].departure_node, NodeId::INVALID);
        }
        assert!(store.routes.is_empty());
    }

    #[test]
    fn in_transit_helper() {
        let store = MobilityStore::new(2);
        assert!(!store.in_transit(AgentId(0)));
    }

    #[test]
    fn arrive_removes_route_and_marks_stationary() {
        let net = two_node_network();
        let mut store = MobilityStore::new(2);
        // Manually place agent in transit.
        store.states[0] = MovementState {
            in_transit:       true,
            departure_node:   NodeId(0),
            destination_node: NodeId(1),
            departure_tick:   Tick(0),
            arrival_tick:     Tick(5),
        };
        store.routes.insert(AgentId(0), DijkstraRouter.route(&net, NodeId(0), NodeId(1), TransportMode::Car).unwrap());

        let dest = store.arrive(AgentId(0), Tick(5));
        assert_eq!(dest, NodeId(1));
        assert!(!store.states[0].in_transit);
        assert!(store.routes.get(&AgentId(0)).is_none());
    }
}

// ── MobilityEngine ────────────────────────────────────────────────────────────

#[cfg(test)]
mod mobility_engine {
    use super::*;

    #[test]
    fn place_sets_node() {
        let mut eng = engine(2);
        eng.place(AgentId(0), NodeId(5), Tick(0));
        assert_eq!(eng.store.states[0].departure_node, NodeId(5));
        assert!(!eng.store.states[0].in_transit);
    }

    #[test]
    fn begin_travel_sets_in_transit() {
        let net = two_node_network();
        let mut eng = engine(1);
        eng.place(AgentId(0), NodeId(0), Tick(0));

        let arrival = eng
            .begin_travel(AgentId(0), NodeId(1), TransportMode::Car, Tick(0), 3600, &net)
            .unwrap();

        assert!(arrival > Tick(0));
        assert!(eng.store.states[0].in_transit);
        assert_eq!(eng.store.states[0].destination_node, NodeId(1));
    }

    #[test]
    fn begin_travel_not_placed_errors() {
        let net = two_node_network();
        let mut eng = engine(1);
        // Agent at INVALID node (not placed).
        let result = eng.begin_travel(AgentId(0), NodeId(1), TransportMode::Car, Tick(0), 3600, &net);
        assert!(matches!(result, Err(crate::MobilityError::NotPlaced(_))));
    }

    #[test]
    fn begin_travel_already_in_transit_errors() {
        let net = two_node_network();
        let mut eng = engine(1);
        eng.place(AgentId(0), NodeId(0), Tick(0));
        eng.begin_travel(AgentId(0), NodeId(1), TransportMode::Car, Tick(0), 3600, &net).unwrap();
        // Try to start another journey while in transit.
        let result = eng.begin_travel(AgentId(0), NodeId(0), TransportMode::Car, Tick(0), 3600, &net);
        assert!(matches!(result, Err(crate::MobilityError::AlreadyInTransit(_))));
    }

    #[test]
    fn tick_arrivals_returns_arrived_agents() {
        let net = two_node_network();
        let mut eng = engine(2);
        eng.place(AgentId(0), NodeId(0), Tick(0));
        eng.place(AgentId(1), NodeId(0), Tick(0));

        let arr0 = eng.begin_travel(AgentId(0), NodeId(1), TransportMode::Car, Tick(0), 3600, &net).unwrap();
        let arr1 = eng.begin_travel(AgentId(1), NodeId(1), TransportMode::Car, Tick(0), 3600, &net).unwrap();

        // Before arrival: no arrivals.
        let arrived = eng.tick_arrivals(Tick(0));
        assert!(arrived.is_empty());

        // At arrival tick for both (they depart the same tick with same route).
        assert_eq!(arr0, arr1);
        let arrived = eng.tick_arrivals(arr0);
        assert_eq!(arrived.len(), 2);
        for (agent, node) in &arrived {
            assert_eq!(*node, NodeId(1));
            // Agent should now be stationary.
            assert!(!eng.store.states[agent.index()].in_transit);
        }
    }

    #[test]
    fn visual_position_stationary() {
        let mut eng = engine(1);
        eng.place(AgentId(0), NodeId(3), Tick(0));
        let (dep, dest, progress) = eng.visual_position(AgentId(0), Tick(5));
        assert_eq!(dep, NodeId(3));
        assert_eq!(dest, NodeId(3));
        assert_eq!(progress, 1.0);
    }

    #[test]
    fn visual_position_in_transit() {
        let net = two_node_network();
        let mut eng = engine(1);
        eng.place(AgentId(0), NodeId(0), Tick(0));
        let arrival = eng
            .begin_travel(AgentId(0), NodeId(1), TransportMode::Car, Tick(0), 3600, &net)
            .unwrap();

        let (dep, dest, progress) = eng.visual_position(AgentId(0), Tick(0));
        assert_eq!(dep, NodeId(0));
        assert_eq!(dest, NodeId(1));
        assert!((progress - 0.0).abs() < 1e-6);

        let (_, _, progress_end) = eng.visual_position(AgentId(0), arrival);
        assert!((progress_end - 1.0).abs() < 1e-6);
    }

    #[test]
    fn multi_hop_route_stored() {
        let net = three_node_network();
        let mut eng = engine(1);
        eng.place(AgentId(0), NodeId(0), Tick(0));
        eng.begin_travel(AgentId(0), NodeId(2), TransportMode::Car, Tick(0), 3600, &net)
            .unwrap();
        // Route should have 2 edges (0→1, 1→2).
        let route = eng.store.routes.get(&AgentId(0)).unwrap();
        assert_eq!(route.edges.len(), 2);
    }
}
