//! `dt-agent` — Structure-of-Arrays agent storage for the `rust_dt` framework.
//!
//! # Crate layout
//!
//! | Module          | Contents                                                  |
//! |-----------------|-----------------------------------------------------------|
//! | [`component`]   | `ComponentVec` trait, `TypedComponentVec<T>`, `ComponentMap` |
//! | [`store`]       | `AgentStore` (SoA arrays), `AgentRngs` (per-agent RNG)    |
//! | [`builder`]     | `AgentStoreBuilder` (fluent construction)                 |
//!
//! # Feature flags
//!
//! | Flag       | SoA arrays added to `AgentStore`                           |
//! |------------|------------------------------------------------------------|
//! | `spatial`  | `node_id`, `edge_id`, `edge_progress`                      |
//! | `schedule` | `next_event_tick`, `current_activity`                      |
//! | `mobility` | `transport_mode`                                           |
//! | `serde`    | Derives `Serialize`/`Deserialize` on all public types.     |
//!
//! All features are off by default; enable only what your application uses.

pub mod builder;
pub mod component;
pub mod store;

#[cfg(test)]
mod tests;

pub use builder::AgentStoreBuilder;
pub use component::{ComponentMap, ComponentVec, TypedComponentVec};
pub use store::{AgentRngs, AgentStore};
