//! `dt-schedule` — agent activity plans, wake queue, and CSV loading.
//!
//! # Crate layout
//!
//! | Module        | Contents                                                  |
//! |---------------|-----------------------------------------------------------|
//! | [`activity`]  | `Destination`, `ScheduledActivity`, `ActivityPlan`        |
//! | [`wake_queue`]| `WakeQueue` (`BTreeMap<Tick, Vec<AgentId>>`)              |
//! | [`modifier`]  | `ScheduleModifier` trait, `NoModification`, `ChainedModifier` |
//! | [`loader`]    | `load_plans_csv`, `load_plans_reader`                     |
//! | [`error`]     | `ScheduleError`, `ScheduleResult<T>`                      |
//!
//! # Cycle model (summary)
//!
//! Every agent has an `ActivityPlan` with `cycle_ticks` period.  At tick `t`:
//!
//! ```text
//! cycle_pos         = t.0 % cycle_ticks
//! current_activity  = last activity whose start_offset_ticks ≤ cycle_pos
//! next_wake_tick    = t + (ticks until next activity starts)
//! ```
//!
//! The `WakeQueue` maps future ticks → agents that need re-planning, so only
//! active agents are processed each tick.

pub mod activity;
pub mod error;
pub mod loader;
pub mod modifier;
pub mod wake_queue;

#[cfg(test)]
mod tests;

pub use activity::{ActivityPlan, Destination, ScheduledActivity};
pub use error::{ScheduleError, ScheduleResult};
pub use loader::{load_plans_csv, load_plans_reader};
pub use modifier::{ChainedModifier, NoModification, ScheduleModifier, ScheduleModifierExt};
pub use wake_queue::WakeQueue;
