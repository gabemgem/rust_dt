//! Routing trait and default Dijkstra implementation.
//!
//! # Pluggability
//!
//! `dt-sim` calls routing via the [`Router`] trait, so applications can swap
//! in custom implementations (contraction hierarchies, A*, behavioural
//! models) without touching the framework core.  The default [`DijkstraRouter`]
//! is sufficient for the MVP.
//!
//! # Cost units
//!
//! All costs and totals are in **milliseconds** (u32) internally.  `Route`
//! exposes `total_travel_secs: f32` and a `travel_ticks()` helper for
//! integration with the sim clock.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use dt_core::{EdgeId, NodeId, TransportMode};

use crate::network::RoadNetwork;
use crate::SpatialError;

// ── Route ─────────────────────────────────────────────────────────────────────

/// The result of a routing query: an ordered list of `EdgeId`s and the total
/// car travel time.
#[derive(Debug, Clone)]
pub struct Route {
    /// Edges to traverse in order, from source to destination.
    pub edges: Vec<EdgeId>,
    /// Cumulative car travel time in seconds.
    pub total_travel_secs: f32,
}

impl Route {
    /// Convert travel time to simulation ticks (ceiling division so agents
    /// never arrive before the correct tick).
    pub fn travel_ticks(&self, tick_duration_secs: u32) -> u64 {
        (self.total_travel_secs / tick_duration_secs as f32).ceil() as u64
    }

    /// `true` if the source and destination are the same node.
    pub fn is_trivial(&self) -> bool {
        self.edges.is_empty()
    }
}

// ── Router trait ──────────────────────────────────────────────────────────────

/// Pluggable routing engine.
///
/// Implement this trait to replace the default Dijkstra with a contraction
/// hierarchy, A*, or a behavioural model (e.g., congestion avoidance).
///
/// # Thread safety
///
/// Implementations must be `Send + Sync` so they can be shared across Rayon
/// worker threads during parallel intent computation.
pub trait Router: Send + Sync {
    /// Compute a route from `from` to `to` for the given transport mode.
    ///
    /// Returns `None` if no path exists (disconnected graph, `from == to`
    /// is handled as an empty route rather than `None`).
    fn route(
        &self,
        network: &RoadNetwork,
        from: NodeId,
        to: NodeId,
        mode: TransportMode,
    ) -> Result<Route, SpatialError>;
}

// ── DijkstraRouter ────────────────────────────────────────────────────────────

/// Standard Dijkstra's algorithm over the CSR road graph.
///
/// Uses `edge_travel_ms` as cost for `Car` mode.  For other modes the cost is
/// derived from `edge_length_m` divided by the mode's assumed speed:
///
/// | Mode    | Speed     |
/// |---------|-----------|
/// | Car     | from OSM  |
/// | Walk    | 1.4 m/s   |
/// | Bike    | 4.2 m/s   |
/// | Transit | 8.3 m/s   |
///
/// Applications that need mode-specific road graphs (e.g. cycling paths,
/// GTFS transit) should implement their own [`Router`].
pub struct DijkstraRouter;

impl Router for DijkstraRouter {
    fn route(
        &self,
        network: &RoadNetwork,
        from: NodeId,
        to: NodeId,
        mode: TransportMode,
    ) -> Result<Route, SpatialError> {
        dijkstra(network, from, to, mode)
    }
}

// ── Dijkstra internals ────────────────────────────────────────────────────────

/// Edge cost in milliseconds for the given mode.
#[inline]
fn edge_cost_ms(network: &RoadNetwork, edge: EdgeId, mode: TransportMode) -> u32 {
    match mode {
        TransportMode::Car | TransportMode::None => network.edge_travel_ms[edge.index()],
        TransportMode::Walk => {
            (network.edge_length_m[edge.index()] / 1.4 * 1000.0) as u32
        }
        TransportMode::Bike => {
            (network.edge_length_m[edge.index()] / 4.2 * 1000.0) as u32
        }
        TransportMode::Transit => {
            // Approximation; real transit uses GTFS schedules in dt-mobility.
            (network.edge_length_m[edge.index()] / 8.3 * 1000.0) as u32
        }
        // Future modes added to TransportMode fall back to car cost.
        _ => network.edge_travel_ms[edge.index()],
    }
}

fn dijkstra(
    network: &RoadNetwork,
    from: NodeId,
    to: NodeId,
    mode: TransportMode,
) -> Result<Route, SpatialError> {
    if from == to {
        return Ok(Route { edges: vec![], total_travel_secs: 0.0 });
    }

    let n = network.node_count();
    // dist[v] = best known cost (ms) to reach v.
    let mut dist     = vec![u32::MAX; n];
    // prev_edge[v] = EdgeId that reached v; EdgeId::INVALID for unreached nodes.
    let mut prev_edge = vec![EdgeId::INVALID; n];

    dist[from.index()] = 0;

    // Min-heap: (cost, node). Reverse makes BinaryHeap (max) behave as min-heap.
    // Secondary key NodeId ensures deterministic tie-breaking.
    let mut heap: BinaryHeap<Reverse<(u32, NodeId)>> = BinaryHeap::new();
    heap.push(Reverse((0, from)));

    while let Some(Reverse((cost, node))) = heap.pop() {
        if node == to {
            return Ok(reconstruct(network, prev_edge, to, cost));
        }

        // Skip stale heap entries.
        if cost > dist[node.index()] {
            continue;
        }

        for edge in network.out_edges(node) {
            let neighbor = network.edge_to[edge.index()];
            let new_cost = cost.saturating_add(edge_cost_ms(network, edge, mode));

            if new_cost < dist[neighbor.index()] {
                dist[neighbor.index()] = new_cost;
                prev_edge[neighbor.index()] = edge;
                heap.push(Reverse((new_cost, neighbor)));
            }
        }
    }

    Err(SpatialError::NoRoute { from, to })
}

fn reconstruct(
    network: &RoadNetwork,
    prev_edge: Vec<EdgeId>,
    to: NodeId,
    total_ms: u32,
) -> Route {
    let mut edges = Vec::new();
    let mut cur = to;
    loop {
        let e = prev_edge[cur.index()];
        if e == EdgeId::INVALID {
            break;
        }
        edges.push(e);
        cur = network.edge_from[e.index()];
    }
    edges.reverse();
    Route {
        edges,
        total_travel_secs: total_ms as f32 / 1000.0,
    }
}
