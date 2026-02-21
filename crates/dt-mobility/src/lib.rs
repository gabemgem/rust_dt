//! `dt-mobility` — agent movement state, routing, and arrival tracking.
//!
//! # Crate layout
//!
//! | Module      | Contents                                                          |
//! |-------------|-------------------------------------------------------------------|
//! | [`state`]   | `MovementState` — per-agent travel state                          |
//! | [`store`]   | `MobilityStore` — `Vec<MovementState>` + sparse route cache       |
//! | [`engine`]  | `MobilityEngine<R>` — intent-driven travel + arrival advancement  |
//! | [`error`]   | `MobilityError`, `MobilityResult<T>`                              |
//!
//! # Movement model (hourly-tick teleport)
//!
//! Agents use a **teleport-at-arrival** model:
//!
//! 1. `MobilityEngine::begin_travel` computes a route via a pluggable
//!    [`Router`][dt_spatial::Router] and sets `arrival_tick = now + travel_ticks`.
//! 2. The agent logically stays at `departure_node` until `arrival_tick`.
//! 3. `MobilityEngine::tick_arrivals(now)` returns all agents whose
//!    `arrival_tick <= now` and calls `store.arrive()` to mark them stationary
//!    at `destination_node`.
//! 4. dt-sim inserts those agents back into the `WakeQueue` for re-planning.
//!
//! For visualization, `MobilityEngine::visual_position` returns
//! `(departure_node, destination_node, progress ∈ [0,1])` so rendering tools
//! can interpolate a smooth path along the stored route.

pub mod engine;
pub mod error;
pub mod state;
pub mod store;

#[cfg(test)]
mod tests;

pub use engine::MobilityEngine;
pub use error::{MobilityError, MobilityResult};
pub use state::MovementState;
pub use store::MobilityStore;
