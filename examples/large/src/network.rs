//! 100×100 synthetic road grid for the Atlanta, GA metro area.
//!
//! Node layout (10 000 nodes, row=south→north, col=west→east):
//!
//! ```text
//!  col:  0          …         99
//!       (home)               (work)
//! ```
//!
//! Same geographic bounding box as the original 7×7 grid, subdivided into
//! 100 rows × 100 cols (~315 m N-S, ~530 m E-W per cell at Atlanta latitude).
//! Home nodes = column 0 (100 nodes); work nodes = column 99 (100 nodes).

use dt_core::{GeoPoint, NodeId};
use dt_spatial::{RoadNetwork, RoadNetworkBuilder};

pub const ROWS: usize = 100;
pub const COLS: usize = 100;

/// Southern latitude and step between rows (~315 m/step).
const LAT_MIN:  f32 = 33.54;
const LAT_STEP: f32 = 0.003;

/// Western longitude and step between columns (~530 m/step near Atlanta).
const LON_MIN:  f32 = -84.57;
const LON_STEP: f32 = 0.006;

/// ~50 km/h in m/s.
const SPEED_MPS: f32 = 13.89;

/// Build the 100×100 grid and return `(network, flat_node_array)`.
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

    // Horizontal edges (east-west streets within each row).
    for row in 0..ROWS {
        // Correct east-west distance for latitude (cos projection).
        let lat_rad = (LAT_MIN + row as f32 * LAT_STEP).to_radians();
        let dist_m  = LON_STEP * lat_rad.cos() * 111_320.0;
        let travel_ms = (dist_m / SPEED_MPS * 1_000.0) as u32;
        for col in 0..COLS - 1 {
            let a = nodes[row * COLS + col];
            let b = nodes[row * COLS + col + 1];
            bldr.add_road(a, b, dist_m, travel_ms);
        }
    }

    // Vertical edges (north-south avenues within each column).
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

/// Residential (home) nodes: column 0 (westernmost column, 100 nodes).
#[allow(dead_code)]
pub fn home_nodes(all_nodes: &[NodeId]) -> Vec<NodeId> {
    (0..ROWS).map(|r| all_nodes[r * COLS]).collect()
}

/// Commercial (work) nodes: column 99 (easternmost column, 100 nodes).
#[allow(dead_code)]
pub fn work_nodes(all_nodes: &[NodeId]) -> Vec<NodeId> {
    (0..ROWS).map(|r| all_nodes[r * COLS + COLS - 1]).collect()
}
