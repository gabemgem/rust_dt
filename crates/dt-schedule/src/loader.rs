//! CSV schedule loader.
//!
//! # CSV format
//!
//! One row per scheduled activity.  All rows for the same agent must share the
//! same `cycle_ticks` value.
//!
//! ```csv
//! agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks
//! 0,0,0,8,home,168
//! 0,1,8,9,42,168
//! 0,0,17,7,home,168
//! 1,0,0,8,home,168
//! 1,2,8,9,work,168
//! ```
//!
//! **`destination`** field:
//!
//! | Value  | Meaning                                       |
//! |--------|-----------------------------------------------|
//! | `home` | `Destination::Home` sentinel                  |
//! | `work` | `Destination::Work` sentinel                  |
//! | *u32*  | `Destination::Node(NodeId(n))`                |
//!
//! Agents absent from the CSV receive an empty `ActivityPlan`.
//!
//! # Large files
//!
//! Rows are buffered in a `HashMap<agent_id, Vec<row>>` before plan
//! construction.  For 5 M agents × 3 activities each the buffer is roughly
//! 600 MB — well within the target workstation's budget.  For tighter memory
//! constraints, pre-sort the CSV by `agent_id` and stream it.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use dt_core::{ActivityId, NodeId};

use crate::activity::{ActivityPlan, Destination, ScheduledActivity};
use crate::ScheduleError;

// ── CSV record ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ScheduleRecord {
    agent_id:            u32,
    activity_id:         u16,
    start_offset_ticks:  u32,
    duration_ticks:      u32,
    destination:         String,
    cycle_ticks:         u32,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load per-agent `ActivityPlan`s from a CSV file.
///
/// Returns a `Vec` of length `agent_count`, indexed by `AgentId`.  Agents
/// with no rows in the file receive [`ActivityPlan::empty`].
pub fn load_plans_csv(path: &Path, agent_count: usize) -> Result<Vec<ActivityPlan>, ScheduleError> {
    let file = std::fs::File::open(path)
        .map_err(ScheduleError::Io)?;
    load_plans_reader(file, agent_count)
}

/// Like [`load_plans_csv`] but accepts any `Read` source.
///
/// Useful for testing (pass a `std::io::Cursor`) or loading from network
/// streams.
pub fn load_plans_reader<R: Read>(
    reader: R,
    agent_count: usize,
) -> Result<Vec<ActivityPlan>, ScheduleError> {
    // ── Parse CSV rows ────────────────────────────────────────────────────
    let mut csv_reader = csv::Reader::from_reader(reader);
    let mut by_agent: HashMap<u32, Vec<ScheduleRecord>> =
        HashMap::with_capacity(agent_count.min(1_000_000));

    for result in csv_reader.deserialize::<ScheduleRecord>() {
        let row = result.map_err(|e| ScheduleError::Parse(e.to_string()))?;
        by_agent.entry(row.agent_id).or_default().push(row);
    }

    // ── Build one ActivityPlan per agent ──────────────────────────────────
    let mut plans: Vec<ActivityPlan> = Vec::with_capacity(agent_count);

    for i in 0..agent_count as u32 {
        match by_agent.remove(&i) {
            None => plans.push(ActivityPlan::empty()),
            Some(rows) => {
                // All rows for the same agent are expected to share cycle_ticks.
                let cycle_ticks = rows[0].cycle_ticks;

                let activities: Vec<ScheduledActivity> = rows
                    .into_iter()
                    .map(|r| {
                        Ok(ScheduledActivity {
                            start_offset_ticks: r.start_offset_ticks,
                            duration_ticks:     r.duration_ticks,
                            activity_id:        ActivityId(r.activity_id),
                            destination:        parse_destination(&r.destination)?,
                        })
                    })
                    .collect::<Result<_, ScheduleError>>()?;

                plans.push(ActivityPlan::new(activities, cycle_ticks));
            }
        }
    }

    Ok(plans)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_destination(s: &str) -> Result<Destination, ScheduleError> {
    match s.trim() {
        "home" => Ok(Destination::Home),
        "work" => Ok(Destination::Work),
        n => n
            .parse::<u32>()
            .map(|id| Destination::Node(NodeId(id)))
            .map_err(|_| {
                ScheduleError::Parse(format!(
                    "invalid destination {n:?}: expected \"home\", \"work\", or a NodeId (u32)"
                ))
            }),
    }
}
