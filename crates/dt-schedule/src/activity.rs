//! Core schedule types: `Destination`, `ScheduledActivity`, and `ActivityPlan`.
//!
//! # Cycle model
//!
//! Each agent carries an `ActivityPlan` consisting of an ordered list of
//! activities and a `cycle_ticks` period (e.g. 168 for one week at
//! 1-hour ticks).  At any simulation tick `t`, the agent's position within
//! its cycle is:
//!
//! ```text
//! cycle_pos = t.0 % cycle_ticks
//! ```
//!
//! The active activity is the one with the largest `start_offset_ticks` ≤
//! `cycle_pos`.  If the cycle position falls before the first activity's
//! start (which can happen mid-cycle at sim start), the last activity of the
//! previous cycle is considered active.
//!
//! # Destination resolution
//!
//! `Destination::Home` and `Destination::Work` are sentinels.  They must be
//! resolved to `Destination::Node` by the simulation layer before an agent
//! begins moving.  The application is responsible for populating per-agent
//! home/work `NodeId`s (typically from the population CSV).

use dt_core::{ActivityId, NodeId, Tick};

// ── Destination ───────────────────────────────────────────────────────────────

/// Where an agent is headed for a given activity.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Destination {
    /// A specific road-network node (fully resolved).
    Node(NodeId),
    /// Sentinel: resolved per-agent to the agent's registered home node.
    Home,
    /// Sentinel: resolved per-agent to the agent's registered work node.
    Work,
}

impl Destination {
    /// `true` if the destination has been resolved to a concrete node.
    pub fn is_resolved(&self) -> bool {
        matches!(self, Destination::Node(_))
    }

    /// Return the `NodeId` if resolved, otherwise `None`.
    pub fn node_id(&self) -> Option<NodeId> {
        match self {
            Destination::Node(n) => Some(*n),
            _ => None,
        }
    }
}

// ── ScheduledActivity ─────────────────────────────────────────────────────────

/// One entry in an agent's activity plan.
///
/// `activity_id` is application-defined (e.g. 0 = sleep, 1 = work).
/// The framework only cares about timing and destination; the meaning of each
/// ID is left to the application's `BehaviorModel`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScheduledActivity {
    /// Tick offset from the start of the cycle at which this activity begins.
    pub start_offset_ticks: u32,

    /// How long this activity is planned to last, in ticks.
    /// Informational — `ActivityPlan` uses the *next* activity's start to
    /// determine wake-up time, not this field.
    pub duration_ticks: u32,

    /// Application-defined activity type identifier.
    pub activity_id: ActivityId,

    /// Where the agent should be for this activity.
    pub destination: Destination,
}

// ── ActivityPlan ──────────────────────────────────────────────────────────────

/// A cyclic activity schedule for one agent.
///
/// Activities are stored sorted by `start_offset_ticks` so that lookups are
/// O(log n) binary searches.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActivityPlan {
    /// Activities, sorted ascending by `start_offset_ticks`.
    activities: Vec<ScheduledActivity>,
    /// Length of one schedule cycle in ticks (e.g. 168 = 1 week @ 1 hr/tick).
    pub cycle_ticks: u32,
}

impl ActivityPlan {
    /// Construct a plan, sorting `activities` by start offset.
    ///
    /// # Panics
    ///
    /// Panics in debug mode if `cycle_ticks == 0` or if any activity has
    /// `start_offset_ticks >= cycle_ticks`.
    pub fn new(mut activities: Vec<ScheduledActivity>, cycle_ticks: u32) -> Self {
        debug_assert!(cycle_ticks > 0, "cycle_ticks must be > 0");
        debug_assert!(
            activities
                .iter()
                .all(|a| a.start_offset_ticks < cycle_ticks),
            "all start_offset_ticks must be < cycle_ticks"
        );
        activities.sort_unstable_by_key(|a| a.start_offset_ticks);
        Self { activities, cycle_ticks }
    }

    /// An empty plan with no scheduled activities.
    pub fn empty() -> Self {
        Self { activities: Vec::new(), cycle_ticks: 1 }
    }

    pub fn is_empty(&self) -> bool {
        self.activities.is_empty()
    }

    pub fn len(&self) -> usize {
        self.activities.len()
    }

    /// Read-only slice of all activities (sorted by start offset).
    pub fn activities(&self) -> &[ScheduledActivity] {
        &self.activities
    }

    // ── Cycle position ────────────────────────────────────────────────────

    /// Tick offset within the current cycle for absolute tick `t`.
    #[inline]
    pub fn cycle_pos(&self, tick: Tick) -> u32 {
        (tick.0 % self.cycle_ticks as u64) as u32
    }

    // ── Lookups ───────────────────────────────────────────────────────────

    /// The activity that should be active at tick `t`, or `None` if the plan
    /// is empty.
    ///
    /// Finds the activity with the largest `start_offset_ticks` ≤ `cycle_pos`.
    /// If `cycle_pos` falls before the first activity (possible at sim start
    /// when the cycle doesn't start at 0), returns the last activity of the
    /// previous cycle.
    pub fn current_activity(&self, tick: Tick) -> Option<&ScheduledActivity> {
        if self.activities.is_empty() {
            return None;
        }
        let pos = self.cycle_pos(tick);
        let idx = self.activity_idx_at(pos);
        Some(&self.activities[idx])
    }

    /// The absolute tick at which the agent should next wake up and re-plan.
    ///
    /// Returns `None` if the plan is empty.
    ///
    /// For a plan with one activity, the agent wakes up `cycle_ticks` later
    /// (start of the next cycle).  For multi-activity plans the agent wakes at
    /// the start of the next sequential activity.
    pub fn next_wake_tick(&self, tick: Tick) -> Option<Tick> {
        if self.activities.is_empty() {
            return None;
        }
        let pos = self.cycle_pos(tick);
        let cur_idx = self.activity_idx_at(pos);
        let next_idx = (cur_idx + 1) % self.activities.len();

        let ticks_until: u64 = if next_idx > cur_idx {
            // Next activity is later in the same cycle.
            let next_offset = self.activities[next_idx].start_offset_ticks as u64;
            next_offset - pos as u64
        } else {
            // Next activity wraps to the next cycle.
            let next_offset = self.activities[next_idx].start_offset_ticks as u64;
            self.cycle_ticks as u64 - pos as u64 + next_offset
        };

        // Guard against a degenerate plan where ticks_until would be 0
        // (e.g. duplicate start offsets).  Advance by one full cycle.
        let ticks_until = ticks_until.max(1);

        Some(tick + ticks_until)
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Index of the activity currently active at `cycle_pos` within this cycle.
    fn activity_idx_at(&self, cycle_pos: u32) -> usize {
        // partition_point returns the first index where cond is false, i.e.
        // the first activity whose start_offset > cycle_pos.
        let idx = self
            .activities
            .partition_point(|a| a.start_offset_ticks <= cycle_pos);

        if idx == 0 {
            // cycle_pos is before the first activity — wrap to the last one
            // (the agent is still in the last activity from the previous cycle).
            self.activities.len() - 1
        } else {
            idx - 1
        }
    }
}
