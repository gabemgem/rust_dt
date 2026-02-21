//! `large` — 1 M agent scheduling throughput benchmark.
//!
//! Each agent runs a simple 24-tick daily cycle (no travel, no file output).
//! The parallel intent phase (Rayon) is enabled via the `parallel` feature on dt-sim.
//!
//! Run with:
//!   cargo run -p large --release

use std::time::Instant;

use anyhow::Result;

use dt_agent::AgentStoreBuilder;
use dt_behavior::{BehaviorModel, Intent, SimContext};
use dt_core::{ActivityId, AgentId, AgentRng, NodeId, SimConfig, Tick};
use dt_schedule::{ActivityPlan, Destination, ScheduledActivity};
use dt_sim::{SimBuilder, SimObserver};
use dt_spatial::DijkstraRouter;

// ── Constants ─────────────────────────────────────────────────────────────────

const AGENT_COUNT:    usize = 1_000_000;
const SEED:           u64   = 42;
const SIM_DAYS:       u64   = 7;
const TICKS_PER_DAY:  u64   = 24;

// ── Behavior ──────────────────────────────────────────────────────────────────

/// Re-queues every agent for the same time next day — no routing or state.
struct DailyWakeBehavior;

impl BehaviorModel for DailyWakeBehavior {
    fn replan(&self, _agent: AgentId, ctx: &SimContext<'_>, _rng: &mut AgentRng) -> Vec<Intent> {
        vec![Intent::WakeAt(Tick(ctx.tick.0 + TICKS_PER_DAY))]
    }
}

// ── Observer ──────────────────────────────────────────────────────────────────

struct BenchObserver {
    start:         Instant,
    total_wakeups: u64,
}

impl SimObserver for BenchObserver {
    fn on_tick_end(&mut self, tick: Tick, woken: usize) {
        if woken == 0 {
            return;
        }
        self.total_wakeups += woken as u64;
        let elapsed = self.start.elapsed().as_secs_f64();
        let rate    = self.total_wakeups as f64 / elapsed / 1_000_000.0;
        println!(
            "  day {:2}  tick {:4}  woken={:>12}  cumulative={:>13}  {:.3}s  ({:.1} M wake-ups/s)",
            tick.0 / TICKS_PER_DAY + 1,
            tick.0,
            woken,
            self.total_wakeups,
            elapsed,
            rate,
        );
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_plan() -> ActivityPlan {
    // One activity spanning the whole 24-tick cycle.
    // SimBuilder seeds the wake queue at the first cycle boundary: tick 24.
    ActivityPlan::new(
        vec![ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     TICKS_PER_DAY as u32,
            activity_id:        ActivityId(0),
            destination:        Destination::Node(NodeId::INVALID),
        }],
        TICKS_PER_DAY as u32,
    )
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    println!("=== rust_dt  large benchmark ===");
    println!("Agents: {AGENT_COUNT}  |  Days: {SIM_DAYS}  |  Seed: {SEED}  |  parallel: enabled");
    println!();

    // 1. Allocate agent store + per-agent RNGs.
    let t_build = Instant::now();
    let (store, rngs) = AgentStoreBuilder::new(AGENT_COUNT, SEED).build();

    // 2. Generate one ActivityPlan per agent.
    let plans: Vec<ActivityPlan> = (0..AGENT_COUNT).map(|_| make_plan()).collect();

    println!("Build (store + plans): {:.2}s", t_build.elapsed().as_secs_f64());
    println!();

    // 3. Sim config — snapshots disabled to avoid file I/O overhead.
    let config = SimConfig {
        start_unix_secs:       1_700_000_000,
        tick_duration_secs:    3_600,
        total_ticks:           SIM_DAYS * TICKS_PER_DAY,
        seed:                  SEED,
        num_threads:           None,
        output_interval_ticks: 0,
    };

    // 4. Build sim — no road network (pure scheduling benchmark).
    let mut sim = SimBuilder::new(config, store, rngs, DailyWakeBehavior, DijkstraRouter)
        .plans(plans)
        .build()?;

    // 5. Run.
    println!(
        "Running {} ticks ({} days × {} h/tick, parallel Rayon intent phase)…",
        SIM_DAYS * TICKS_PER_DAY,
        SIM_DAYS,
        TICKS_PER_DAY,
    );
    let mut obs = BenchObserver { start: Instant::now(), total_wakeups: 0 };
    sim.run(&mut obs)?;

    let elapsed = obs.start.elapsed().as_secs_f64();
    println!();
    println!("Simulation complete in {:.3}s", elapsed);
    println!(
        "Throughput: {:.1} M agent-wakeups/s  (total {})",
        obs.total_wakeups as f64 / elapsed / 1_000_000.0,
        obs.total_wakeups,
    );

    Ok(())
}
