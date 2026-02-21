//! Simulation time model.
//!
//! # Design
//!
//! Time is represented as a monotonically increasing `Tick` counter.  The
//! mapping to wall-clock time is held in `SimClock`:
//!
//!   wall_time = start_unix_secs + tick * tick_duration_secs
//!
//! Using an integer tick as the canonical time unit means all schedule
//! arithmetic is exact (no floating-point drift) and comparisons are O(1).
//!
//! The default tick duration is 3,600 s (1 simulated hour).  Applications
//! that need finer resolution set `tick_duration_secs` to a smaller value;
//! the rest of the framework is agnostic.

use std::fmt;

// ── Tick ─────────────────────────────────────────────────────────────────────

/// An absolute simulation tick counter.
///
/// Stored as `u64` to avoid overflow: at 1 tick/second and 1 s per tick, a
/// u64 lasts ~585 billion years.  At the default 1 tick/hour it lasts far
/// longer than any conceivable run.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tick(pub u64);

impl Tick {
    pub const ZERO: Tick = Tick(0);

    /// Return the tick `n` steps after `self`.
    #[inline]
    pub fn offset(self, n: u64) -> Tick {
        Tick(self.0 + n)
    }

    /// Ticks elapsed from `earlier` to `self`.
    ///
    /// # Panics
    /// Panics in debug mode if `earlier > self`.
    #[inline]
    pub fn since(self, earlier: Tick) -> u64 {
        self.0 - earlier.0
    }
}

impl std::ops::Add<u64> for Tick {
    type Output = Tick;
    #[inline]
    fn add(self, rhs: u64) -> Tick {
        Tick(self.0 + rhs)
    }
}

impl std::ops::Sub for Tick {
    type Output = u64;
    #[inline]
    fn sub(self, rhs: Tick) -> u64 {
        self.0 - rhs.0
    }
}

impl fmt::Display for Tick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T{}", self.0)
    }
}

// ── SimClock ──────────────────────────────────────────────────────────────────

/// Converts between tick counts and Unix wall-clock seconds.
///
/// `SimClock` is cheap to copy and intentionally holds no heap data.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SimClock {
    /// Unix timestamp (seconds since epoch) of tick 0.
    pub start_unix_secs: i64,
    /// How many real seconds one tick represents.  Default: 3600 (1 hour).
    pub tick_duration_secs: u32,
    /// The current tick — advanced by `SimClock::advance()` each iteration.
    pub current_tick: Tick,
}

impl SimClock {
    /// Create a clock starting at `start_unix_secs` with the given resolution.
    pub fn new(start_unix_secs: i64, tick_duration_secs: u32) -> Self {
        Self {
            start_unix_secs,
            tick_duration_secs,
            current_tick: Tick::ZERO,
        }
    }

    /// Advance the clock by one tick.
    #[inline]
    pub fn advance(&mut self) {
        self.current_tick = Tick(self.current_tick.0 + 1);
    }

    /// Elapsed simulated seconds since tick 0.
    #[inline]
    pub fn elapsed_secs(&self) -> i64 {
        self.current_tick.0 as i64 * self.tick_duration_secs as i64
    }

    /// Current Unix timestamp corresponding to `current_tick`.
    #[inline]
    pub fn current_unix_secs(&self) -> i64 {
        self.start_unix_secs + self.elapsed_secs()
    }

    /// Break elapsed time into (day, hour, minute) components from sim start.
    /// Useful for human-readable logging without a datetime library.
    pub fn elapsed_dhm(&self) -> (u64, u32, u32) {
        let total_secs = self.elapsed_secs().max(0) as u64;
        let days = total_secs / 86_400;
        let hours = ((total_secs % 86_400) / 3_600) as u32;
        let minutes = ((total_secs % 3_600) / 60) as u32;
        (days, hours, minutes)
    }

    // ── Tick-count helpers ────────────────────────────────────────────────

    /// How many ticks span `secs` seconds? (rounds up — agent won't be late)
    #[inline]
    pub fn ticks_for_secs(&self, secs: u64) -> u64 {
        secs.div_ceil(self.tick_duration_secs as u64)
    }

    #[inline]
    pub fn ticks_for_hours(&self, hours: u64) -> u64 {
        self.ticks_for_secs(hours * 3_600)
    }

    #[inline]
    pub fn ticks_for_days(&self, days: u64) -> u64 {
        self.ticks_for_secs(days * 86_400)
    }
}

impl fmt::Display for SimClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (d, h, m) = self.elapsed_dhm();
        write!(f, "{} (day {} {:02}:{:02})", self.current_tick, d, h, m)
    }
}

// ── SimConfig ─────────────────────────────────────────────────────────────────

/// Top-level simulation configuration.
///
/// Typically loaded from a TOML/JSON file by the application crate and passed
/// to the simulation runner.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SimConfig {
    /// Unix timestamp for tick 0 (e.g. a Monday 00:00 local time).
    pub start_unix_secs: i64,

    /// Seconds per tick.  Must evenly divide 3600 or be a multiple of 3600
    /// for schedule arithmetic to remain exact.  Default: 3600.
    pub tick_duration_secs: u32,

    /// Total ticks to simulate.  For 365 days at 1 tick/hour: 365 * 24 = 8760.
    pub total_ticks: u64,

    /// Master RNG seed.  The same seed always produces identical results.
    pub seed: u64,

    /// Worker thread count passed to Rayon.  `None` uses all logical cores.
    pub num_threads: Option<usize>,

    /// Write output every N ticks.  1 = every tick; 24 = once per day (at
    /// 1-hour resolution).
    pub output_interval_ticks: u64,
}

impl SimConfig {
    /// The tick at which the simulation ends (exclusive upper bound).
    #[inline]
    pub fn end_tick(&self) -> Tick {
        Tick(self.total_ticks)
    }

    /// Construct a `SimClock` pre-configured for this run.
    pub fn make_clock(&self) -> SimClock {
        SimClock::new(self.start_unix_secs, self.tick_duration_secs)
    }
}
