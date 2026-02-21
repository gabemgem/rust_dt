//! `dt-sim` — tick loop orchestrator for the rust_dt framework.
//!
//! # Two-phase tick loop
//!
//! ```text
//! for tick in 0..config.total_ticks:
//!   ① Arrivals  — agents reaching their destination are marked stationary
//!                 and re-inserted into the wake queue.
//!   ② Wake      — drain agents scheduled for this tick from WakeQueue.
//!   ③ Intents   — call BehaviorModel::replan for each woken agent
//!                 (parallel with the `parallel` feature).
//!   ④ Apply     — for each intent in ascending AgentId order:
//!                   WakeAt(t)          → push agent into wake queue at t
//!                   TravelTo(dest, m)  → begin_travel; push arrival_tick
//!                   SendMessage(..)    → TODO (future)
//! ```
//!
//! # Cargo features
//!
//! | Feature    | Effect                                                 |
//! |------------|--------------------------------------------------------|
//! | `parallel` | Runs the intent phase on Rayon's thread pool.          |
//!
//! # Quick-start
//!
//! ```rust,ignore
//! use dt_agent::AgentStoreBuilder;
//! use dt_behavior::NoopBehavior;
//! use dt_core::SimConfig;
//! use dt_sim::{NoopObserver, SimBuilder};
//! use dt_spatial::DijkstraRouter;
//!
//! let (store, rngs) = AgentStoreBuilder::new(1_000, 42).build();
//! let mut sim = SimBuilder::new(config, store, rngs, NoopBehavior, DijkstraRouter)
//!     .build()?;
//! sim.run(&mut NoopObserver)?;
//! ```

pub mod builder;
pub mod error;
pub mod observer;
pub mod sim;

#[cfg(test)]
mod tests;

pub use builder::SimBuilder;
pub use error::{SimError, SimResult};
pub use observer::{NoopObserver, SimObserver};
pub use sim::Sim;
