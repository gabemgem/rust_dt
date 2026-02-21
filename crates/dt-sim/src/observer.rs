//! Simulation observer trait for progress reporting and data collection.

use dt_agent::AgentStore;
use dt_core::Tick;
use dt_mobility::MobilityStore;

/// Callbacks invoked by [`Sim::run`][crate::Sim::run] at key points in the
/// tick loop.
///
/// All methods have default no-op implementations so implementors only need to
/// override what they care about.
///
/// # Example — progress printer
///
/// ```rust,ignore
/// struct ProgressPrinter { interval: u64 }
///
/// impl SimObserver for ProgressPrinter {
///     fn on_tick_end(&mut self, tick: Tick, woken: usize) {
///         if tick.0 % self.interval == 0 {
///             println!("tick {tick}: woke {woken} agents");
///         }
///     }
/// }
/// ```
pub trait SimObserver {
    /// Called at the very start of each tick, before any processing.
    fn on_tick_start(&mut self, _tick: Tick) {}

    /// Called at the end of each tick.
    ///
    /// `woken` is the number of agents that were woken (had `replan` called)
    /// this tick.
    fn on_tick_end(&mut self, _tick: Tick, _woken: usize) {}

    /// Called at snapshot intervals (every `config.output_interval_ticks` ticks).
    ///
    /// Provides read-only access to the full mobility and agent state so that
    /// output writers can record a position snapshot without the sim needing
    /// to know about any specific output format.
    fn on_snapshot(
        &mut self,
        _tick:     Tick,
        _mobility: &MobilityStore,
        _agents:   &AgentStore,
    ) {}

    /// Called once after the final tick completes.
    fn on_sim_end(&mut self, _final_tick: Tick) {}
}

/// A [`SimObserver`] that does nothing.  Use when you need to call `run` but
/// don't want progress callbacks.
pub struct NoopObserver;

impl SimObserver for NoopObserver {}
