//! Unit tests for dt-schedule.

use dt_core::{ActivityId, NodeId, Tick};

use crate::{
    ActivityPlan, Destination, NoModification, ScheduleModifier, ScheduledActivity, WakeQueue,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn act(start: u32, dur: u32, id: u16) -> ScheduledActivity {
    ScheduledActivity {
        start_offset_ticks: start,
        duration_ticks:     dur,
        activity_id:        ActivityId(id),
        destination:        Destination::Home,
    }
}

/// Three-activity daily plan (24-tick cycle, 1 tick = 1 hour).
/// Sleep 0–8, Work 8–17, Leisure 17–24.
fn daily_plan() -> ActivityPlan {
    ActivityPlan::new(
        vec![act(0, 8, 0), act(8, 9, 1), act(17, 7, 2)],
        24,
    )
}

// ── ActivityPlan ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod activity_plan {
    use super::*;

    #[test]
    fn new_sorts_by_start_offset() {
        // Provide activities out of order.
        let plan = ActivityPlan::new(
            vec![act(17, 7, 2), act(0, 8, 0), act(8, 9, 1)],
            24,
        );
        let offsets: Vec<u32> = plan.activities().iter().map(|a| a.start_offset_ticks).collect();
        assert_eq!(offsets, vec![0, 8, 17]);
    }

    #[test]
    fn empty_plan() {
        let plan = ActivityPlan::empty();
        assert!(plan.is_empty());
        assert!(plan.current_activity(Tick(0)).is_none());
        assert!(plan.next_wake_tick(Tick(0)).is_none());
    }

    #[test]
    fn single_activity_always_active() {
        let plan = ActivityPlan::new(vec![act(0, 24, 99)], 24);
        assert_eq!(plan.current_activity(Tick(0)).unwrap().activity_id, ActivityId(99));
        assert_eq!(plan.current_activity(Tick(23)).unwrap().activity_id, ActivityId(99));
        assert_eq!(plan.current_activity(Tick(100)).unwrap().activity_id, ActivityId(99));
    }

    #[test]
    fn current_activity_at_exact_start() {
        let plan = daily_plan();
        // Tick 0 = sleep starts
        assert_eq!(plan.current_activity(Tick(0)).unwrap().activity_id, ActivityId(0));
        // Tick 8 = work starts
        assert_eq!(plan.current_activity(Tick(8)).unwrap().activity_id, ActivityId(1));
        // Tick 17 = leisure starts
        assert_eq!(plan.current_activity(Tick(17)).unwrap().activity_id, ActivityId(2));
    }

    #[test]
    fn current_activity_mid_activity() {
        let plan = daily_plan();
        assert_eq!(plan.current_activity(Tick(4)).unwrap().activity_id, ActivityId(0)); // sleep
        assert_eq!(plan.current_activity(Tick(12)).unwrap().activity_id, ActivityId(1)); // work
        assert_eq!(plan.current_activity(Tick(20)).unwrap().activity_id, ActivityId(2)); // leisure
    }

    #[test]
    fn current_activity_wraps_across_cycles() {
        let plan = daily_plan();
        // Tick 24 = start of day 2, same as tick 0 → sleep
        assert_eq!(plan.current_activity(Tick(24)).unwrap().activity_id, ActivityId(0));
        // Tick 33 = tick 9 in cycle → work
        assert_eq!(plan.current_activity(Tick(33)).unwrap().activity_id, ActivityId(1));
    }

    #[test]
    fn next_wake_tick_during_sleep() {
        // At tick 4 (mid-sleep), next wake is when work starts = tick 8.
        let plan = daily_plan();
        assert_eq!(plan.next_wake_tick(Tick(4)), Some(Tick(8)));
    }

    #[test]
    fn next_wake_tick_during_work() {
        // At tick 12 (mid-work), next wake is when leisure starts = tick 17.
        let plan = daily_plan();
        assert_eq!(plan.next_wake_tick(Tick(12)), Some(Tick(17)));
    }

    #[test]
    fn next_wake_tick_last_activity_wraps() {
        // At tick 20 (mid-leisure), next wake wraps to sleep at tick 24.
        let plan = daily_plan();
        assert_eq!(plan.next_wake_tick(Tick(20)), Some(Tick(24)));
    }

    #[test]
    fn next_wake_tick_at_exact_transition() {
        // At tick 8 (just entered work), next wake is leisure at 17.
        let plan = daily_plan();
        assert_eq!(plan.next_wake_tick(Tick(8)), Some(Tick(17)));
    }

    #[test]
    fn next_wake_tick_single_activity_advances_to_next_cycle_start() {
        let plan = ActivityPlan::new(vec![act(0, 24, 0)], 24);
        // Single-activity plan: agent wakes at the *start of the next cycle*
        // (when offset 0 repeats), not necessarily cycle_ticks after now.
        //
        //   tick 5:  cycle_pos = 5,  next cycle starts at tick 24  (5 + 19)
        //   tick 100: cycle_pos = 4, next cycle starts at tick 120 (100 + 20)
        //   tick 0:  cycle_pos = 0,  next cycle starts at tick 24  (0 + 24)
        //   tick 24: cycle_pos = 0,  next cycle starts at tick 48  (24 + 24)
        assert_eq!(plan.next_wake_tick(Tick(5)),   Some(Tick(24)));
        assert_eq!(plan.next_wake_tick(Tick(100)), Some(Tick(120)));
        assert_eq!(plan.next_wake_tick(Tick(0)),   Some(Tick(24)));
        assert_eq!(plan.next_wake_tick(Tick(24)),  Some(Tick(48)));
    }

    #[test]
    fn cycle_pos_correct() {
        let plan = daily_plan(); // cycle_ticks = 24
        assert_eq!(plan.cycle_pos(Tick(0)),  0);
        assert_eq!(plan.cycle_pos(Tick(23)), 23);
        assert_eq!(plan.cycle_pos(Tick(24)), 0);
        assert_eq!(plan.cycle_pos(Tick(25)), 1);
    }

    #[test]
    fn destination_variants() {
        let node_dest = Destination::Node(NodeId(42));
        assert!(node_dest.is_resolved());
        assert_eq!(node_dest.node_id(), Some(NodeId(42)));

        assert!(!Destination::Home.is_resolved());
        assert!(!Destination::Work.is_resolved());
        assert!(Destination::Home.node_id().is_none());
    }
}

// ── WakeQueue ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod wake_queue {
    use dt_core::AgentId;

    use super::*;

    #[test]
    fn push_and_drain() {
        let mut q = WakeQueue::new();
        q.push(Tick(5), AgentId(0));
        q.push(Tick(5), AgentId(1));
        q.push(Tick(7), AgentId(2));

        assert_eq!(q.len(), 3);
        assert_eq!(q.next_tick(), Some(Tick(5)));

        let drained = q.drain_tick(Tick(5)).unwrap();
        assert_eq!(drained.len(), 2);
        assert_eq!(q.len(), 1);
        assert_eq!(q.next_tick(), Some(Tick(7)));
    }

    #[test]
    fn drain_absent_tick_returns_none() {
        let mut q = WakeQueue::new();
        q.push(Tick(10), AgentId(0));
        assert!(q.drain_tick(Tick(9)).is_none());
        assert_eq!(q.len(), 1); // not consumed
    }

    #[test]
    fn empty_queue() {
        let q = WakeQueue::new();
        assert!(q.is_empty());
        assert!(q.next_tick().is_none());
    }

    #[test]
    fn tick_count() {
        let mut q = WakeQueue::new();
        q.push(Tick(1), AgentId(0));
        q.push(Tick(1), AgentId(1));
        q.push(Tick(3), AgentId(2));
        assert_eq!(q.tick_count(), 2); // 2 distinct ticks
        assert_eq!(q.len(), 3);        // 3 total agents
    }

    #[test]
    fn build_from_plans_skips_empty() {
        let plans = vec![
            daily_plan(),                // agent 0: gets a wake tick
            ActivityPlan::empty(),       // agent 1: no wake tick
            daily_plan(),                // agent 2: gets a wake tick
        ];
        let q = WakeQueue::build_from_plans(&plans, Tick(0));
        // Both agent 0 and 2 should be in the queue; agent 1 should not.
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn build_from_plans_correct_tick() {
        // Single-activity 24-tick plan: at sim start (tick 0), next wake is 24.
        let plans = vec![ActivityPlan::new(vec![act(0, 24, 0)], 24)];
        let q = WakeQueue::build_from_plans(&plans, Tick(0));
        assert_eq!(q.next_tick(), Some(Tick(24)));
    }
}

// ── ScheduleModifier ──────────────────────────────────────────────────────────

#[cfg(test)]
mod modifier {
    use dt_core::{AgentId, AgentRng};

    use crate::modifier::ScheduleModifierExt;

    use super::*;

    fn dummy_activity() -> ScheduledActivity {
        act(0, 8, 0)
    }

    #[test]
    fn no_modification_returns_none() {
        let mut rng = AgentRng::new(0, AgentId(0));
        let result = NoModification.modify(AgentId(0), &dummy_activity(), &mut rng);
        assert!(result.is_none());
    }

    #[test]
    fn chained_both_none() {
        let chained = NoModification.then(NoModification);
        let mut rng = AgentRng::new(0, AgentId(0));
        assert!(chained.modify(AgentId(0), &dummy_activity(), &mut rng).is_none());
    }

    #[test]
    fn chained_first_modifies() {
        use crate::modifier::ScheduleModifierExt;

        struct ReplaceWith(ScheduledActivity);
        impl ScheduleModifier for ReplaceWith {
            fn modify(
                &self,
                _: AgentId,
                _: &ScheduledActivity,
                _: &mut AgentRng,
            ) -> Option<ScheduledActivity> {
                Some(self.0.clone())
            }
        }

        let replacement = act(5, 3, 99);
        let chained = ReplaceWith(replacement.clone()).then(NoModification);
        let mut rng = AgentRng::new(0, AgentId(0));
        let result = chained.modify(AgentId(0), &dummy_activity(), &mut rng).unwrap();
        assert_eq!(result.activity_id, ActivityId(99));
    }
}

// ── CSV Loader ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod loader {
    use std::io::Cursor;

    use dt_core::{ActivityId, NodeId};

    use crate::{load_plans_reader, Destination};

    const CSV: &[u8] = b"\
agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks\n\
0,0,0,8,home,24\n\
0,1,8,9,42,24\n\
0,2,17,7,work,24\n\
1,0,0,8,home,24\n\
1,1,8,9,0,24\n\
";

    #[test]
    fn loads_two_agents() {
        let plans = load_plans_reader(Cursor::new(CSV), 3).unwrap();
        assert_eq!(plans.len(), 3);
        assert_eq!(plans[0].len(), 3);
        assert_eq!(plans[1].len(), 2);
        assert!(plans[2].is_empty()); // agent 2 absent from CSV
    }

    #[test]
    fn correct_activity_ids() {
        let plans = load_plans_reader(Cursor::new(CSV), 2).unwrap();
        let acts = plans[0].activities();
        assert_eq!(acts[0].activity_id, ActivityId(0));
        assert_eq!(acts[1].activity_id, ActivityId(1));
        assert_eq!(acts[2].activity_id, ActivityId(2));
    }

    #[test]
    fn destination_parsing() {
        let plans = load_plans_reader(Cursor::new(CSV), 2).unwrap();
        let acts = plans[0].activities();
        assert_eq!(acts[0].destination, Destination::Home);
        assert_eq!(acts[1].destination, Destination::Node(NodeId(42)));
        assert_eq!(acts[2].destination, Destination::Work);
    }

    #[test]
    fn sorted_after_load() {
        // Rows for agent 0 are in order; still verify they're sorted.
        let plans = load_plans_reader(Cursor::new(CSV), 1).unwrap();
        let offsets: Vec<u32> = plans[0]
            .activities()
            .iter()
            .map(|a| a.start_offset_ticks)
            .collect();
        assert_eq!(offsets, vec![0, 8, 17]);
    }

    #[test]
    fn invalid_destination_errors() {
        let bad = b"\
agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks\n\
0,0,0,8,invalid_dest,24\n\
";
        let result = load_plans_reader(Cursor::new(bad.as_slice()), 1);
        assert!(result.is_err());
    }

    #[test]
    fn agent_absent_gets_empty_plan() {
        let plans = load_plans_reader(Cursor::new(CSV), 5).unwrap();
        assert!(plans[2].is_empty());
        assert!(plans[3].is_empty());
        assert!(plans[4].is_empty());
    }
}
