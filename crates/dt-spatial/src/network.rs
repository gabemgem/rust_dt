//! Road network representation and builder.
//!
//! # Data layout
//!
//! The graph uses **Compressed Sparse Row (CSR)** format for outgoing edges.
//! Given a `NodeId n`, its outgoing edges occupy the slice:
//!
//! ```text
//! edge_from[ node_out_start[n] .. node_out_start[n+1] ]
//! ```
//!
//! All edge arrays (`edge_from`, `edge_to`, `edge_length_m`, `edge_travel_ms`)
//! are sorted by source node and indexed by `EdgeId`.  Iteration over a
//! node's outgoing edges is therefore a contiguous memory scan — ideal for
//! Dijkstra's inner loop.
//!
//! # Spatial index
//!
//! An R-tree (via `rstar`) maps `(lat, lon)` to the nearest `NodeId`.  Used
//! at load time to snap agent home/work lat/lon pairs to road nodes.

use rstar::{PointDistance, RTree, RTreeObject, AABB};

use dt_core::{EdgeId, GeoPoint, NodeId};

// ── R-tree node entry ─────────────────────────────────────────────────────────

/// Entry stored in the R-tree spatial index: a 2-D `[lat, lon]` point with
/// the associated `NodeId`.
#[derive(Clone)]
struct NodeEntry {
    point: [f32; 2], // [lat, lon]
    id: NodeId,
}

impl RTreeObject for NodeEntry {
    type Envelope = AABB<[f32; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}

impl PointDistance for NodeEntry {
    /// Squared Euclidean distance in lat/lon space.  Sufficient for
    /// nearest-node queries within a city (error < 0.1 % at ≤ 60° lat).
    fn distance_2(&self, point: &[f32; 2]) -> f32 {
        let dlat = self.point[0] - point[0];
        let dlon = self.point[1] - point[1];
        dlat * dlat + dlon * dlon
    }
}

// ── RoadNetwork ───────────────────────────────────────────────────────────────

/// Directed road graph in CSR format plus a spatial index for node snapping.
///
/// All fields are `pub` for direct indexed access on hot paths.  Do not
/// construct directly; use [`RoadNetworkBuilder`].
pub struct RoadNetwork {
    // ── Node data ─────────────────────────────────────────────────────────
    /// Geographic position of each node.  Indexed by `NodeId`.
    pub node_pos: Vec<GeoPoint>,

    // ── CSR edge adjacency ────────────────────────────────────────────────
    /// CSR row pointer.  Outgoing edges of node `n` are at EdgeIds
    /// `node_out_start[n] .. node_out_start[n+1]`.
    /// Length = `node_count + 1`.
    pub node_out_start: Vec<u32>,

    // ── Edge data (indexed by EdgeId = position in sorted order) ──────────
    /// Source node of each edge.  Redundant with CSR but required for
    /// efficient route reconstruction (trace `prev_edge` back to source).
    pub edge_from: Vec<NodeId>,

    /// Destination node of each edge.
    pub edge_to: Vec<NodeId>,

    /// Length of each edge in metres.
    pub edge_length_m: Vec<f32>,

    /// Car travel time in milliseconds.  Used as Dijkstra edge cost.
    /// Other modes compute their own costs from `edge_length_m` at query time.
    pub edge_travel_ms: Vec<u32>,

    // ── Spatial index ─────────────────────────────────────────────────────
    spatial_idx: RTree<NodeEntry>,
}

impl RoadNetwork {
    /// Construct an empty network with no nodes or edges.
    ///
    /// Useful as a placeholder when no routing is needed (e.g. behaviour-only
    /// simulations using `NoopBehavior`).  Any routing request against an
    /// empty network will return [`SpatialError::NoRoute`].
    pub fn empty() -> Self {
        RoadNetworkBuilder::new().build()
    }

    // ── Graph dimensions ──────────────────────────────────────────────────

    pub fn node_count(&self) -> usize {
        self.node_pos.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edge_to.len()
    }

    pub fn is_empty(&self) -> bool {
        self.node_pos.is_empty()
    }

    // ── Graph traversal ───────────────────────────────────────────────────

    /// Iterator over the `EdgeId`s of all outgoing edges from `node`.
    ///
    /// This is a contiguous index range — no heap allocation.
    #[inline]
    pub fn out_edges(&self, node: NodeId) -> impl Iterator<Item = EdgeId> + '_ {
        let start = self.node_out_start[node.index()] as usize;
        let end   = self.node_out_start[node.index() + 1] as usize;
        (start..end).map(|i| EdgeId(i as u32))
    }

    /// Out-degree of `node` (number of outgoing edges).
    #[inline]
    pub fn out_degree(&self, node: NodeId) -> usize {
        let start = self.node_out_start[node.index()] as usize;
        let end   = self.node_out_start[node.index() + 1] as usize;
        end - start
    }

    // ── Spatial queries ───────────────────────────────────────────────────

    /// Return the `NodeId` of the nearest road node to `pos`.
    ///
    /// Returns `None` only if the network has no nodes.
    pub fn snap_to_node(&self, pos: GeoPoint) -> Option<NodeId> {
        self.spatial_idx
            .nearest_neighbor(&[pos.lat, pos.lon])
            .map(|e| e.id)
    }

    /// Return up to `k` nearest nodes to `pos`, sorted by ascending distance.
    pub fn k_nearest_nodes(&self, pos: GeoPoint, k: usize) -> Vec<NodeId> {
        self.spatial_idx
            .nearest_neighbor_iter(&[pos.lat, pos.lon])
            .take(k)
            .map(|e| e.id)
            .collect()
    }
}

// ── RoadNetworkBuilder ────────────────────────────────────────────────────────

/// Construct a [`RoadNetwork`] incrementally, then call [`build`](Self::build).
///
/// The builder accepts nodes and directed edges in any order.  `build()`
/// sorts edges by source node, constructs the CSR arrays, and bulk-loads the
/// R-tree.
///
/// # Example
///
/// ```
/// use dt_core::GeoPoint;
/// use dt_spatial::RoadNetworkBuilder;
///
/// let mut b = RoadNetworkBuilder::new();
/// let a = b.add_node(GeoPoint::new(30.69, -88.04));
/// let c = b.add_node(GeoPoint::new(30.70, -88.03));
/// b.add_road(a, c, 1_200.0, 90_000); // 1.2 km, 90 s travel → 90_000 ms
/// let net = b.build();
/// assert_eq!(net.node_count(), 2);
/// assert_eq!(net.edge_count(), 2); // bidirectional
/// ```
pub struct RoadNetworkBuilder {
    nodes:     Vec<GeoPoint>,
    raw_edges: Vec<RawEdge>,
}

struct RawEdge {
    from:       NodeId,
    to:         NodeId,
    length_m:   f32,
    travel_ms:  u32,
}

impl RoadNetworkBuilder {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), raw_edges: Vec::new() }
    }

    /// Pre-allocate for the expected number of nodes and edges to reduce
    /// reallocations when bulk-loading from OSM or CSV.
    pub fn with_capacity(nodes: usize, edges: usize) -> Self {
        Self {
            nodes:     Vec::with_capacity(nodes),
            raw_edges: Vec::with_capacity(edges),
        }
    }

    /// Add a road node and return its `NodeId` (sequential from 0).
    pub fn add_node(&mut self, pos: GeoPoint) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(pos);
        id
    }

    /// Add a **directed** edge from `from` to `to`.
    ///
    /// - `length_m`: physical length in metres.
    /// - `travel_ms`: car travel time in milliseconds (used as Dijkstra cost).
    pub fn add_directed_edge(&mut self, from: NodeId, to: NodeId, length_m: f32, travel_ms: u32) {
        self.raw_edges.push(RawEdge { from, to, length_m, travel_ms });
    }

    /// Convenience: add edges in **both directions** for an undirected road
    /// segment (the common case for most OSM road types).
    pub fn add_road(&mut self, a: NodeId, b: NodeId, length_m: f32, travel_ms: u32) {
        self.add_directed_edge(a, b, length_m, travel_ms);
        self.add_directed_edge(b, a, length_m, travel_ms);
    }

    /// Look up the position of a node added earlier (used by the OSM loader
    /// to compute edge lengths between adjacent way nodes).
    pub fn node_pos(&self, id: NodeId) -> GeoPoint {
        self.nodes[id.index()]
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn edge_count(&self) -> usize { self.raw_edges.len() }

    /// Consume the builder and produce a [`RoadNetwork`].
    ///
    /// Time complexity: O(E log E) for edge sort + O(N log N) for R-tree bulk
    /// load, where N = nodes, E = edges.
    pub fn build(self) -> RoadNetwork {
        let node_count = self.nodes.len();
        let edge_count = self.raw_edges.len();

        // Sort edges by source node for CSR construction.
        let mut raw = self.raw_edges;
        raw.sort_unstable_by_key(|e| e.from.0);

        // Build edge arrays from sorted raw edges.
        let edge_from:      Vec<NodeId> = raw.iter().map(|e| e.from).collect();
        let edge_to:        Vec<NodeId> = raw.iter().map(|e| e.to).collect();
        let edge_length_m:  Vec<f32>    = raw.iter().map(|e| e.length_m).collect();
        let edge_travel_ms: Vec<u32>    = raw.iter().map(|e| e.travel_ms).collect();

        // Build CSR row pointer (node_out_start).
        let mut node_out_start = vec![0u32; node_count + 1];
        for e in &raw {
            node_out_start[e.from.index() + 1] += 1;
        }
        for i in 1..=node_count {
            node_out_start[i] += node_out_start[i - 1];
        }
        debug_assert_eq!(node_out_start[node_count] as usize, edge_count);

        // Bulk-load R-tree for O(N log N) construction (faster than N inserts).
        let entries: Vec<NodeEntry> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, &pos)| NodeEntry {
                point: [pos.lat, pos.lon],
                id: NodeId(i as u32),
            })
            .collect();
        let spatial_idx = RTree::bulk_load(entries);

        RoadNetwork {
            node_pos: self.nodes,
            node_out_start,
            edge_from,
            edge_to,
            edge_length_m,
            edge_travel_ms,
            spatial_idx,
        }
    }
}

impl Default for RoadNetworkBuilder {
    fn default() -> Self {
        Self::new()
    }
}
