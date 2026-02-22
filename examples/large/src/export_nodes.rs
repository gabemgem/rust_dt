//! Export node coordinates and network edges for the visualization layer.
//!
//! Writes two JSON files to `output/large/`:
//!   - `node_coords.json`   — `[{node_id, lat, lon}, …]`
//!   - `network_edges.json` — `[{from_node, to_node}, …]`
//!
//! Run with: `cargo run -p large --bin export_nodes`

mod network;

use std::fs;
use anyhow::Result;
use serde_json::json;

use network::build_network;

fn main() -> Result<()> {
    let (net, _nodes) = build_network();

    fs::create_dir_all("output/large")?;

    // ── node_coords.json ──────────────────────────────────────────────────────
    let node_coords: Vec<serde_json::Value> = net
        .node_pos
        .iter()
        .enumerate()
        .map(|(i, pos)| json!({ "node_id": i, "lat": pos.lat, "lon": pos.lon }))
        .collect();

    let coords_json = serde_json::to_string_pretty(&node_coords)?;
    fs::write("output/large/node_coords.json", &coords_json)?;
    println!("Wrote output/large/node_coords.json ({} nodes)", node_coords.len());

    // ── network_edges.json ────────────────────────────────────────────────────
    let edges: Vec<serde_json::Value> = net
        .edge_from
        .iter()
        .zip(net.edge_to.iter())
        .map(|(from, to)| json!({ "from_node": from.0, "to_node": to.0 }))
        .collect();

    let edges_json = serde_json::to_string_pretty(&edges)?;
    fs::write("output/large/network_edges.json", &edges_json)?;
    println!("Wrote output/large/network_edges.json ({} edges)", edges.len());

    Ok(())
}
