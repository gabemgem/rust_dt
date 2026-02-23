//! `dt-behavior` — agent behavior model trait and intent types.
//!
//! # Crate layout
//!
//! | Module      | Contents                                                        |
//! |-------------|-----------------------------------------------------------------|
//! | [`intent`]  | `Intent` enum (`TravelTo`, `WakeAt`, `SendMessage`)             |
//! | [`context`] | `SimContext<'a>` — read-only tick snapshot shared by all agents |
//! | [`model`]   | `BehaviorModel` trait                                           |
//! | [`noop`]    | `NoopBehavior` — placeholder that never produces intents        |
//! | [`error`]   | `BehaviorError`, `BehaviorResult<T>`                            |
//!
//! # Design notes
//!
//! The two-phase tick loop in dt-sim works as follows:
//!
//! 1. **Intent phase** (parallel): for every agent woken this tick, call
//!    `BehaviorModel::replan` (and optionally `on_contacts`/`on_message`).
//!    All reads go through `&SimContext`; no mutation.
//!
//! 2. **Apply phase** (sequential): consume the collected `Vec<Intent>`s and
//!    mutate `AgentStore`, `WakeQueue`, and `MobilityStore` accordingly.
//!
//! This split means `BehaviorModel` only needs to be `Send + Sync` — it never
//! holds mutable state that could cause data races.

pub mod context;
pub mod error;
pub mod intent;
pub mod model;
pub mod noop;

#[cfg(test)]
mod tests;

pub use context::SimContext;
pub use error::{BehaviorError, BehaviorResult};
pub use intent::Intent;
pub use model::BehaviorModel;
pub use noop::NoopBehavior;
