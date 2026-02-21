//! `WakeQueue` — sparse per-tick agent activation queue.
//!
//! # Why this exists
//!
//! Most agents are idle most ticks (sleeping, at work).  Iterating all N
//! agents every tick to check "should I do something?" would cost O(N) per
//! tick regardless of how many agents are actually active.
//!
//! `WakeQueue` inverts the problem: when an agent finishes an activity it
//! registers the tick at which it needs attention next.  Each tick the
//! simulation drains only the agents scheduled for that tick — O(active) work
//! instead of O(N).
//!
//! # Performance note
//!
//! `BTreeMap` gives O(log W) insert and O(log W) pop where W = number of
//! distinct wake ticks currently enqueued.  For a 5 M-agent, 1-hour-tick
//! simulation with 3 activities/agent/day, W ≈ 24 distinct ticks (one day's
//! worth of transitions), so the constant is tiny.

use std::collections::BTreeMap;

use dt_core::{AgentId, Tick};

use crate::ActivityPlan;

/// A priority-queue mapping simulation ticks → agents that must wake at that tick.
#[derive(Default)]
pub struct WakeQueue {
    inner: BTreeMap<Tick, Vec<AgentId>>,
    /// Cached total agent count for O(1) `len()`.
    total: usize,
}

impl WakeQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the initial wake queue from a slice of `ActivityPlan`s (indexed
    /// by `AgentId`) and the simulation start tick.
    ///
    /// Each agent is scheduled to wake at `plan.next_wake_tick(sim_start)`.
    /// Agents with empty plans are not inserted.
    pub fn build_from_plans(plans: &[ActivityPlan], sim_start: Tick) -> Self {
        let mut queue = Self::new();
        for (i, plan) in plans.iter().enumerate() {
            if let Some(wake) = plan.next_wake_tick(sim_start) {
                queue.push(wake, AgentId(i as u32));
            }
        }
        queue
    }

    /// Schedule `agent` to wake at `tick`.
    ///
    /// An agent may appear multiple times in the queue (at different ticks)
    /// if — for example — a stochastic modifier inserts an unplanned wake-up.
    /// `dt-sim` should handle duplicates gracefully.
    pub fn push(&mut self, tick: Tick, agent: AgentId) {
        self.inner.entry(tick).or_default().push(agent);
        self.total += 1;
    }

    /// Remove and return all agents scheduled for exactly `tick`.
    ///
    /// Returns `None` if no agents are queued for that tick (common case for
    /// most ticks — avoids allocation).
    pub fn drain_tick(&mut self, tick: Tick) -> Option<Vec<AgentId>> {
        let agents = self.inner.remove(&tick)?;
        self.total -= agents.len();
        Some(agents)
    }

    /// The earliest tick with at least one queued agent, or `None` if empty.
    pub fn next_tick(&self) -> Option<Tick> {
        self.inner.keys().next().copied()
    }

    /// Total number of (tick, agent) entries across all future ticks.
    pub fn len(&self) -> usize {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Number of distinct future ticks that have at least one queued agent.
    pub fn tick_count(&self) -> usize {
        self.inner.len()
    }
}
