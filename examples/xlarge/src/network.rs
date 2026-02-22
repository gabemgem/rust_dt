//! 10×10 synthetic road grid for the Chicago, IL metro area.
//!
//! Node layout (100 nodes, row=south→north, col=west→east):
//!
//! ```text
//!  col: 0-4 (western suburbs)  |  5-9 (city center / near-north)
//!       Elmhurst / Oak Park     |  Loop / Near North / Lakefront
//!       (home)                  |  (work)
//! ```
//!
//! Horizontal roads: ~60 km/h (grid spacing ~5.0 km E-W, ~4.4 km N-S).

use dt_core::{GeoPoint, NodeId};
use dt_spatial::{RoadNetwork, RoadNetworkBuilder};

pub const ROWS: usize = 10;
pub const COLS: usize = 10;

/// Southern latitude and step between rows (~4.4 km/step).
const LAT_MIN:  f32 = 41.65;
const LAT_STEP: f32 = 0.04;

/// Western longitude and step between columns (~5.0 km/step near Chicago).
const LON_MIN:  f32 = -88.00;
const LON_STEP: f32 = 0.06;

/// ~60 km/h in m/s.
const SPEED_MPS: f32 = 16.67;

/// Build the 10×10 grid and return `(network, flat_node_array)`.
///
/// `flat_node_array[row * COLS + col]` is the `NodeId` at that grid cell.
pub fn build_network() -> (RoadNetwork, Vec<NodeId>) {
    let mut bldr = RoadNetworkBuilder::new();
    let mut nodes = vec![NodeId::INVALID; ROWS * COLS];

    // Place nodes at (lat, lon) grid positions.
    for row in 0..ROWS {
        for col in 0..COLS {
            let lat = LAT_MIN + row as f32 * LAT_STEP;
            let lon = LON_MIN + col as f32 * LON_STEP;
            nodes[row * COLS + col] = bldr.add_node(GeoPoint::new(lat, lon));
        }
    }

    // Horizontal edges (east-west roads within each row).
    for row in 0..ROWS {
        let lat_rad = (LAT_MIN + row as f32 * LAT_STEP).to_radians();
        let dist_m  = LON_STEP * lat_rad.cos() * 111_320.0;
        let travel_ms = (dist_m / SPEED_MPS * 1_000.0) as u32;
        for col in 0..COLS - 1 {
            let a = nodes[row * COLS + col];
            let b = nodes[row * COLS + col + 1];
            bldr.add_road(a, b, dist_m, travel_ms);
        }
    }

    // Vertical edges (north-south roads within each column).
    let dist_m    = LAT_STEP * 111_320.0;
    let travel_ms = (dist_m / SPEED_MPS * 1_000.0) as u32;
    for row in 0..ROWS - 1 {
        for col in 0..COLS {
            let a = nodes[row * COLS + col];
            let b = nodes[(row + 1) * COLS + col];
            bldr.add_road(a, b, dist_m, travel_ms);
        }
    }

    (bldr.build(), nodes)
}

/// Residential (home) nodes: columns 0-4 (western suburbs).
#[allow(dead_code)]
pub fn home_nodes(all_nodes: &[NodeId]) -> Vec<NodeId> {
    (0..ROWS)
        .flat_map(|r| (0..5).map(move |c| all_nodes[r * COLS + c]))
        .collect()
}

/// Commercial (work) nodes: columns 5-9 (city center / near-north).
#[allow(dead_code)]
pub fn work_nodes(all_nodes: &[NodeId]) -> Vec<NodeId> {
    (0..ROWS)
        .flat_map(|r| (5..COLS).map(move |c| all_nodes[r * COLS + c]))
        .collect()
}
