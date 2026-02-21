//! Unit tests for dt-spatial.
//!
//! All tests use a hand-crafted network so they run without any OSM file.

#[cfg(test)]
mod helpers {
    use dt_core::GeoPoint;
    use crate::{RoadNetwork, RoadNetworkBuilder};

    /// Build a small grid network for testing.
    ///
    /// Nodes (lat, lon):
    ///   0:(0,0)  1:(0,1)  2:(0,2)
    ///   3:(1,0)           4:(1,2)
    ///
    /// Undirected edges: 0-1, 1-2, 0-3, 2-4, 3-4
    ///
    /// Shortest path 0→4 (by travel time):
    ///   0→1→2→4  vs  0→3→4
    ///   We control travel times so we can assert deterministically.
    pub fn grid_network() -> (RoadNetwork, [dt_core::NodeId; 5]) {
        let mut b = RoadNetworkBuilder::new();

        // Positions (lat, lon) — small offsets; actual coords don't matter
        // for routing tests.
        let n0 = b.add_node(GeoPoint::new(0.0, 0.0));
        let n1 = b.add_node(GeoPoint::new(0.0, 1.0));
        let n2 = b.add_node(GeoPoint::new(0.0, 2.0));
        let n3 = b.add_node(GeoPoint::new(1.0, 0.0));
        let n4 = b.add_node(GeoPoint::new(1.0, 2.0));

        // Edge: (from, to, length_m, travel_ms)
        // Path via 0→1→2→4: 10+10+10 = 30 s = 30_000 ms
        // Path via 0→3→4:   50+10    = 60 s = 60_000 ms
        // → shortest is always 0→1→2→4
        b.add_road(n0, n1, 100.0, 10_000); // 10 s
        b.add_road(n1, n2, 100.0, 10_000); // 10 s
        b.add_road(n2, n4, 100.0, 10_000); // 10 s
        b.add_road(n0, n3, 500.0, 50_000); // 50 s  (long slow road)
        b.add_road(n3, n4, 100.0, 10_000); // 10 s

        (b.build(), [n0, n1, n2, n3, n4])
    }
}

// ── Builder & network structure ────────────────────────────────────────────────

#[cfg(test)]
mod builder {
    use dt_core::GeoPoint;
    use crate::RoadNetworkBuilder;

    #[test]
    fn empty_build() {
        let net = RoadNetworkBuilder::new().build();
        assert_eq!(net.node_count(), 0);
        assert_eq!(net.edge_count(), 0);
        assert!(net.is_empty());
    }

    #[test]
    fn single_road() {
        let mut b = RoadNetworkBuilder::new();
        let a = b.add_node(GeoPoint::new(30.0, -88.0));
        let c = b.add_node(GeoPoint::new(30.1, -88.0));
        b.add_road(a, c, 1_000.0, 75_000);
        let net = b.build();
        assert_eq!(net.node_count(), 2);
        assert_eq!(net.edge_count(), 2); // bidirectional
    }

    #[test]
    fn csr_out_edges() {
        let (net, [n0, n1, n2, n3, n4]) = super::helpers::grid_network();

        // n1 has edges to n0 and n2 (grid topology, bidirectional).
        let n1_out: Vec<_> = net.out_edges(n1).collect();
        assert_eq!(n1_out.len(), 2, "n1 should have 2 outgoing edges");

        // Degrees
        assert_eq!(net.out_degree(n0), 2); // n0→n1, n0→n3
        assert_eq!(net.out_degree(n2), 2); // n2→n1, n2→n4
        assert_eq!(net.out_degree(n3), 2); // n3→n0, n3→n4
        assert_eq!(net.out_degree(n4), 2); // n4→n2, n4→n3
        let _ = n1; // used above
    }

    #[test]
    fn out_edges_destination_correctness() {
        let (net, [n0, n1, _, _, _]) = super::helpers::grid_network();
        // Every outgoing edge from n0 should have n0 as its source.
        for e in net.out_edges(n0) {
            assert_eq!(net.edge_from[e.index()], n0);
        }
        // n1 is reachable from n0.
        let reaches_n1 = net
            .out_edges(n0)
            .any(|e| net.edge_to[e.index()] == n1);
        assert!(reaches_n1);
    }

    #[test]
    fn directed_only_edge() {
        let mut b = RoadNetworkBuilder::new();
        let a = b.add_node(GeoPoint::new(0.0, 0.0));
        let c = b.add_node(GeoPoint::new(0.0, 1.0));
        // One-way a → c only
        b.add_directed_edge(a, c, 100.0, 10_000);
        let net = b.build();
        assert_eq!(net.edge_count(), 1);
        assert_eq!(net.out_degree(a), 1);
        assert_eq!(net.out_degree(c), 0); // no return edge
    }
}

// ── Spatial snap ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod snap {
    use dt_core::GeoPoint;
    use crate::RoadNetworkBuilder;

    #[test]
    fn snap_exact_position() {
        let (net, [n0, ..]) = super::helpers::grid_network();
        // (0.0, 0.0) is exactly node 0.
        let snapped = net.snap_to_node(GeoPoint::new(0.0, 0.0)).unwrap();
        assert_eq!(snapped, n0);
    }

    #[test]
    fn snap_nearest() {
        let (net, [n0, n1, ..]) = super::helpers::grid_network();
        // (0.0, 0.4) is closer to n1 (0.0, 1.0)? No — 0.4 < 0.6 so closer to n0.
        let near_n0 = net.snap_to_node(GeoPoint::new(0.0, 0.4)).unwrap();
        assert_eq!(near_n0, n0);
        let near_n1 = net.snap_to_node(GeoPoint::new(0.0, 0.6)).unwrap();
        assert_eq!(near_n1, n1);
    }

    #[test]
    fn empty_network_returns_none() {
        let net = RoadNetworkBuilder::new().build();
        assert!(net.snap_to_node(GeoPoint::new(0.0, 0.0)).is_none());
    }

    #[test]
    fn k_nearest_order() {
        let (net, nodes) = super::helpers::grid_network();
        // From (0.0, 0.0) the nearest nodes in order are n0, n1, n3, ...
        let nearest = net.k_nearest_nodes(GeoPoint::new(0.0, 0.0), 2);
        assert_eq!(nearest[0], nodes[0]); // n0 is exact
        // n1 (dist=1) and n3 (dist=1) are equidistant in lat/lon — either is valid.
        assert!(nearest[1] == nodes[1] || nearest[1] == nodes[3]);
    }
}

// ── Dijkstra routing ──────────────────────────────────────────────────────────

#[cfg(test)]
mod routing {
    use dt_core::TransportMode;
    use crate::{DijkstraRouter, Router, SpatialError};

    #[test]
    fn trivial_same_node() {
        let (net, [n0, ..]) = super::helpers::grid_network();
        let r = DijkstraRouter.route(&net, n0, n0, TransportMode::Car).unwrap();
        assert!(r.is_trivial());
        assert_eq!(r.total_travel_secs, 0.0);
    }

    #[test]
    fn shortest_path_correct() {
        let (net, [n0, n1, n2, _, n4]) = super::helpers::grid_network();
        let route = DijkstraRouter
            .route(&net, n0, n4, TransportMode::Car)
            .unwrap();

        // Shortest: n0→n1→n2→n4 = 30 s
        assert_eq!(route.total_travel_secs, 30.0);
        assert_eq!(route.edges.len(), 3);

        // Verify edge sequence connectivity
        assert_eq!(net.edge_from[route.edges[0].index()], n0);
        assert_eq!(net.edge_to[route.edges[0].index()], n1);
        assert_eq!(net.edge_to[route.edges[1].index()], n2);
        assert_eq!(net.edge_to[route.edges[2].index()], n4);
    }

    #[test]
    fn no_route_disconnected() {
        use dt_core::GeoPoint;
        use crate::RoadNetworkBuilder;

        let mut b = RoadNetworkBuilder::new();
        let a = b.add_node(GeoPoint::new(0.0, 0.0));
        let c = b.add_node(GeoPoint::new(1.0, 0.0));
        // No edges — a and c are completely disconnected.
        let net = b.build();
        let result = DijkstraRouter.route(&net, a, c, TransportMode::Car);
        assert!(matches!(result, Err(SpatialError::NoRoute { .. })));
    }

    #[test]
    fn directed_one_way_blocks_return() {
        use dt_core::GeoPoint;
        use crate::RoadNetworkBuilder;

        let mut b = RoadNetworkBuilder::new();
        let a = b.add_node(GeoPoint::new(0.0, 0.0));
        let c = b.add_node(GeoPoint::new(0.0, 1.0));
        b.add_directed_edge(a, c, 100.0, 10_000); // one-way a→c
        let net = b.build();

        // Forward: OK
        assert!(DijkstraRouter.route(&net, a, c, TransportMode::Car).is_ok());
        // Return: no route
        assert!(DijkstraRouter.route(&net, c, a, TransportMode::Car).is_err());
    }

    #[test]
    fn travel_ticks_ceiling() {
        let (net, [n0, _, _, _, n4]) = super::helpers::grid_network();
        let route = DijkstraRouter
            .route(&net, n0, n4, TransportMode::Car)
            .unwrap();
        // 30 s, 1-hour ticks → ceil(30 / 3600) = 1 tick
        assert_eq!(route.travel_ticks(3600), 1);
        // 30 s, 1-minute (60 s) ticks → ceil(30 / 60) = 1 tick
        assert_eq!(route.travel_ticks(60), 1);
        // 30 s, 10-second ticks → ceil(30 / 10) = 3 ticks
        assert_eq!(route.travel_ticks(10), 3);
    }

    #[test]
    fn walk_mode_slower_than_car() {
        let (net, [n0, _, _, _, n4]) = super::helpers::grid_network();
        let car  = DijkstraRouter.route(&net, n0, n4, TransportMode::Car).unwrap();
        let walk = DijkstraRouter.route(&net, n0, n4, TransportMode::Walk).unwrap();
        // Walk uses length/speed, car uses pre-computed OSM times.
        // Both should find a valid route; walk should take longer.
        assert!(walk.total_travel_secs > car.total_travel_secs);
    }
}
