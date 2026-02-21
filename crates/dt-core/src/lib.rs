//! `dt-core` — foundational types for the `rust_dt` digital twin framework.
//!
//! This crate is a dependency of every other `dt-*` crate.  It intentionally
//! has no `dt-*` dependencies and minimal external ones (only `rand` and
//! `thiserror`, plus optional `serde`).
//!
//! # What lives here
//!
//! | Module          | Contents                                              |
//! |-----------------|-------------------------------------------------------|
//! | [`ids`]         | `AgentId`, `NodeId`, `EdgeId`, `ActivityId`           |
//! | [`geo`]         | `GeoPoint`, haversine distance                        |
//! | [`time`]        | `Tick`, `SimClock`, `SimConfig`                       |
//! | [`rng`]         | `AgentRng` (per-agent), `SimRng` (global)             |
//! | [`transport`]   | `TransportMode` enum                                  |
//! | [`error`]       | `DtError`, `DtResult`                                 |
//!
//! # Feature flags
//!
//! | Flag    | Effect                                                     |
//! |---------|------------------------------------------------------------|
//! | `serde` | Adds `Serialize`/`Deserialize` to all public types.        |
//!           | Required by `dt-checkpoint`.                               |

pub mod error;
pub mod geo;
pub mod ids;
pub mod rng;
pub mod time;
pub mod transport;

#[cfg(test)]
mod tests;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use error::{DtError, DtResult};
pub use geo::GeoPoint;
pub use ids::{ActivityId, AgentId, EdgeId, NodeId};
pub use rng::{AgentRng, SimRng};
pub use time::{SimClock, SimConfig, Tick};
pub use transport::TransportMode;
