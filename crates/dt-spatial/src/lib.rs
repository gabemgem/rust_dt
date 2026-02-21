//! `dt-spatial` — road network, spatial indexing, and routing.
//!
//! # Crate layout
//!
//! | Module      | Contents                                                    |
//! |-------------|-------------------------------------------------------------|
//! | [`network`] | `RoadNetwork` (CSR + R-tree), `RoadNetworkBuilder`          |
//! | [`router`]  | `Router` trait, `Route`, `DijkstraRouter`                  |
//! | [`osm`]     | `load_from_pbf` (feature = `"osm"` only)                   |
//! | [`error`]   | `SpatialError`, `SpatialResult<T>`                         |
//!
//! # Feature flags
//!
//! | Flag    | Effect                                                       |
//! |---------|--------------------------------------------------------------|
//! | `osm`   | Enables OSM PBF loading via the `osmpbf` crate.             |
//! | `serde` | Derives `Serialize`/`Deserialize` on public types.           |

pub mod error;
pub mod network;
pub mod router;

#[cfg(feature = "osm")]
pub mod osm;

#[cfg(test)]
mod tests;

pub use error::{SpatialError, SpatialResult};
pub use network::{RoadNetwork, RoadNetworkBuilder};
pub use router::{DijkstraRouter, Route, Router};
