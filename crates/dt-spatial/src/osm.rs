//! OSM PBF loader — enabled with the `osm` Cargo feature.
//!
//! # Usage
//!
//! ```ignore
//! use std::path::Path;
//! use dt_spatial::osm::load_from_pbf;
//!
//! let network = load_from_pbf(Path::new("mobile_al.osm.pbf"))?;
//! ```
//!
//! # What is loaded
//!
//! Only drivable `highway=*` ways are included (see [`car_speed_mps`]).
//! All other features (footways, buildings, POIs, relations) are ignored.
//! One-way roads add a single directed edge; two-way roads add both directions.
//!
//! # Memory note
//!
//! The loader buffers all OSM nodes in a `HashMap<i64, GeoPoint>` for the
//! first pass (needed because ways reference node IDs by OSM integer ID).
//! For Mobile, AL this is roughly 3–8 million entries (≈ 100–200 MB).
//! The map is freed before the R-tree is built.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader};

use dt_core::{GeoPoint, NodeId};

use crate::network::{RoadNetwork, RoadNetworkBuilder};
use crate::SpatialError;

// ── Public entry point ────────────────────────────────────────────────────────

/// Load a road network from an OSM PBF file.
///
/// Only car-drivable roads are included.  Use
/// [`RoadNetworkBuilder`] directly for non-OSM sources.
///
/// # Errors
///
/// Returns [`SpatialError::Osm`] on parse errors,
/// [`SpatialError::Io`] on file errors.
pub fn load_from_pbf(path: &Path) -> Result<RoadNetwork, SpatialError> {
    // ── Phase 1: collect all OSM nodes + road ways in one sequential pass ──
    let reader = ElementReader::from_path(path)?;

    let mut all_nodes: HashMap<i64, GeoPoint> = HashMap::new();
    let mut road_ways: Vec<OsmWay> = Vec::new();

    reader
        .for_each(|elem| match elem {
            Element::Node(n) => {
                all_nodes.insert(
                    n.id(),
                    GeoPoint::new(n.lat() as f32, n.lon() as f32),
                );
            }
            Element::DenseNode(n) => {
                all_nodes.insert(
                    n.id(),
                    GeoPoint::new(n.lat() as f32, n.lon() as f32),
                );
            }
            Element::Way(w) => {
                // Collect tags eagerly so &str lifetimes don't escape the closure.
                let tags: Vec<(&str, &str)> = w.tags().collect();
                let highway = tags
                    .iter()
                    .find(|(k, _)| *k == "highway")
                    .map(|(_, v)| *v);

                if let Some(speed_mps) = highway.and_then(car_speed_mps) {
                    let oneway = is_oneway(highway.unwrap_or(""), &tags);
                    let refs: Vec<i64> = w.refs().collect();
                    road_ways.push(OsmWay { refs, speed_mps, oneway });
                }
            }
            _ => {}
        })
        .map_err(|e| SpatialError::Osm(e.to_string()))?;

    // ── Phase 2: identify road-referenced node IDs ────────────────────────
    let road_node_ids: HashSet<i64> = road_ways
        .iter()
        .flat_map(|w| w.refs.iter().copied())
        .collect();

    // ── Phase 3: build network ────────────────────────────────────────────
    // Pre-allocate: ~2× road nodes for edges (rough estimate).
    let mut builder = RoadNetworkBuilder::with_capacity(
        road_node_ids.len(),
        road_node_ids.len() * 2,
    );

    // Map OSM node IDs → our NodeIds, adding only road-relevant nodes.
    let mut osm_to_dt: HashMap<i64, NodeId> =
        HashMap::with_capacity(road_node_ids.len());

    for osm_id in &road_node_ids {
        if let Some(&pos) = all_nodes.get(osm_id) {
            let dt_id = builder.add_node(pos);
            osm_to_dt.insert(*osm_id, dt_id);
        }
    }

    // Free the full node map — no longer needed.
    drop(all_nodes);
    drop(road_node_ids);

    // Add directed edges from way node sequences.
    for way in &road_ways {
        for window in way.refs.windows(2) {
            let (osm_a, osm_b) = (window[0], window[1]);
            if let (Some(&from), Some(&to)) =
                (osm_to_dt.get(&osm_a), osm_to_dt.get(&osm_b))
            {
                let len_m = builder.node_pos(from).distance_m(builder.node_pos(to));
                let travel_ms = (len_m / way.speed_mps * 1_000.0) as u32;

                builder.add_directed_edge(from, to, len_m, travel_ms);
                if !way.oneway {
                    builder.add_directed_edge(to, from, len_m, travel_ms);
                }
            }
        }
    }

    Ok(builder.build())
}

// ── Internal types ────────────────────────────────────────────────────────────

struct OsmWay {
    refs:      Vec<i64>,
    speed_mps: f32,
    oneway:    bool,
}

// ── Tag helpers ───────────────────────────────────────────────────────────────

/// Return the assumed car speed (m/s) for a road class, or `None` if this
/// `highway` value is not drivable by car.
///
/// Speeds are conservative urban defaults — applications may override by
/// implementing their own loader with OSM `maxspeed` parsing.
fn car_speed_mps(highway: &str) -> Option<f32> {
    match highway {
        "motorway" | "motorway_link"         => Some(29.1), // ~65 mph
        "trunk"    | "trunk_link"            => Some(24.6), // ~55 mph
        "primary"  | "primary_link"          => Some(20.1), // ~45 mph
        "secondary"| "secondary_link"        => Some(17.9), // ~40 mph
        "tertiary" | "tertiary_link"         => Some(13.4), // ~30 mph
        "residential" | "living_street"      => Some(8.9),  // ~20 mph
        "service"  | "unclassified"          => Some(6.7),  // ~15 mph
        // Explicitly non-car:
        "footway" | "path" | "cycleway"
        | "pedestrian" | "steps" | "track"   => None,
        // Unknown road type — assign a cautious default rather than dropping.
        _                                    => Some(8.9),
    }
}

/// Determine whether a way should be treated as one-way for car traffic.
///
/// Motorways and motorway links are implicitly one-way in OSM convention.
fn is_oneway(highway: &str, tags: &[(&str, &str)]) -> bool {
    let explicit = tags.iter().any(|(k, v)| {
        *k == "oneway" && matches!(*v, "yes" | "1" | "true")
    });
    let implicit = matches!(highway, "motorway" | "motorway_link");
    explicit || implicit
}
