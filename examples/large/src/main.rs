//! `large` — agent daily commute over a 100×100 Atlanta metro road network.
//!
//! Agents are split across 100 residential nodes (column 0, west side) and
//! commute to 100 commercial nodes (column 99, east side) on a staggered
//! 3-way schedule.  Snapshots of 1-in-20 agents are written every 8 ticks.
//!
//! Run with:
//!   cargo run -p large --release

// Use mimalloc to avoid Windows heap fragmentation at 1 M agents.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod network;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::Result;
use memory_stats::memory_stats;

use dt_agent::{AgentStore, AgentStoreBuilder};
use dt_behavior::{BehaviorModel, Intent, SimContext};
use dt_core::{ActivityId, AgentId, AgentRng, NodeId, SimConfig, Tick, TransportMode};
use dt_mobility::MobilityStore;
use dt_output::{AgentSnapshotRow, CsvWriter, OutputWriter, TickSummaryRow};
use dt_schedule::{ActivityPlan, Destination, ScheduledActivity};
use dt_sim::{SimBuilder, SimObserver};
use dt_spatial::{DijkstraRouter, RoadNetwork, Route, Router, SpatialError};

use network::{build_network, home_nodes, work_nodes};

// ── Memory helper ─────────────────────────────────────────────────────────────

fn mem_mb() -> f64 {
    memory_stats()
        .map(|s| s.physical_mem as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}

// ── Constants ─────────────────────────────────────────────────────────────────

const AGENT_COUNT:           usize = 1_000_000;
const SEED:                  u64   = 42;
const SIM_DAYS:              u64   = 7;
const TICKS_PER_DAY:         u64   = 24;
/// Write agent snapshots every N ticks (captures ticks 0, 8, 16 per day).
const OUTPUT_INTERVAL_TICKS: u64   = 8;
/// Write every Nth agent → 50 K visible agents per snapshot.
const SAMPLE_RATE:           usize = 20;

/// Three staggered morning departure ticks (hour of day) by group.
const DEPART_HOME: [u32; 3] = [7, 8, 9];
/// Corresponding evening departure ticks.
const DEPART_WORK: [u32; 3] = [16, 17, 18];

// ── Application components ────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct HomeNode(NodeId);

#[derive(Default, Clone)]
struct WorkNode(NodeId);

// ── Behavior model ────────────────────────────────────────────────────────────

struct DailyCommuteBehavior {
    contacts_observed: Arc<AtomicU64>,
}

impl BehaviorModel for DailyCommuteBehavior {
    fn replan(&self, agent: AgentId, ctx: &SimContext<'_>, _rng: &mut AgentRng) -> Vec<Intent> {
        let Some(activity) = ctx.plans[agent.index()].current_activity(ctx.tick) else {
            return vec![];
        };

        let dest = match &activity.destination {
            Destination::Home => ctx
                .agents
                .component::<HomeNode>()
                .map(|v| v[agent.index()].0)
                .unwrap_or(NodeId::INVALID),
            Destination::Work => ctx
                .agents
                .component::<WorkNode>()
                .map(|v| v[agent.index()].0)
                .unwrap_or(NodeId::INVALID),
            Destination::Node(n) => *n,
        };

        if dest == NodeId::INVALID {
            return vec![];
        }

        vec![Intent::TravelTo { destination: dest, mode: TransportMode::Car }]
    }

    fn on_contacts(
        &self,
        agent:          AgentId,
        _node:          NodeId,
        agents_at_node: &[AgentId],
        _ctx:           &SimContext<'_>,
        rng:            &mut AgentRng,
    ) -> Vec<Intent> {
        // Reservoir-sample up to 4 neighbors (excluding self).
        // O(n) time, O(1) space — no heap allocation.
        let mut sample = [AgentId(u32::MAX); 4];
        let mut k = 0usize;
        let mut seen = 0usize;
        for &other in agents_at_node {
            if other == agent { continue; }
            if k < 4 {
                sample[k] = other;
                k += 1;
            } else {
                let j = rng.gen_range(0..=seen);
                if j < 4 {
                    sample[j] = other;
                }
            }
            seen += 1;
        }
        // `sample[..k]` holds the chosen contacts — available for downstream use.
        let _ = &sample[..k];
        self.contacts_observed.fetch_add(k as u64, Ordering::Relaxed);
        vec![]
    }
}

// ── Pre-computed router ───────────────────────────────────────────────────────

/// Wraps a pre-computed route table for O(1) lookups per (from, to) pair.
///
/// Pre-computing all 21×21 home↔work pairs takes ~1 ms; look-ups during
/// the sim's apply phase have zero Dijkstra overhead.
struct PrecomputedRouter {
    routes: HashMap<(u32, u32), Route>,
}

impl PrecomputedRouter {
    fn build(network: &RoadNetwork, homes: &[NodeId], works: &[NodeId]) -> Self {
        let router = DijkstraRouter;
        let mut routes = HashMap::new();
        for &h in homes {
            for &w in works {
                if let Ok(r) = router.route(network, h, w, TransportMode::Car) {
                    routes.insert((h.0, w.0), r);
                }
                if let Ok(r) = router.route(network, w, h, TransportMode::Car) {
                    routes.insert((w.0, h.0), r);
                }
            }
        }
        Self { routes }
    }
}

impl Router for PrecomputedRouter {
    fn route(
        &self,
        _network: &RoadNetwork,
        from:     NodeId,
        to:       NodeId,
        _mode:    TransportMode,
    ) -> Result<Route, SpatialError> {
        self.routes
            .get(&(from.0, to.0))
            .cloned()
            .ok_or(SpatialError::NoRoute { from, to })
    }
}

// ── Sampled output observer ───────────────────────────────────────────────────

/// Writes tick summaries every tick, and sampled agent snapshots at snapshot
/// intervals.  Only every `sample_rate`-th agent is written (50 K/snapshot).
struct SampledObserver {
    writer:             CsvWriter,
    start_unix_secs:    i64,
    tick_duration_secs: u32,
    sample_rate:        usize,
    // throughput stats
    start:              Instant,
    total_wakeups:      u64,
}

impl SimObserver for SampledObserver {
    fn on_tick_end(&mut self, tick: Tick, woken: usize) {
        if woken > 0 {
            self.total_wakeups += woken as u64;
        }
        let elapsed = self.start.elapsed().as_secs_f64();
        let mem = mem_mb();
        if woken > 0 {
            println!(
                "  day {:2}  tick {:4}  woken={:>12}  {:.3}s  ({:.1} M/s)  mem={:.0} MB",
                tick.0 / TICKS_PER_DAY + 1,
                tick.0,
                woken,
                elapsed,
                self.total_wakeups as f64 / elapsed / 1_000_000.0,
                mem,
            );
        } else {
            println!(
                "  day {:2}  tick {:4}  (idle)                     {:.3}s               mem={:.0} MB",
                tick.0 / TICKS_PER_DAY + 1,
                tick.0,
                elapsed,
                mem,
            );
        }

        let row = TickSummaryRow {
            tick:           tick.0,
            unix_time_secs: self.start_unix_secs
                + tick.0 as i64 * self.tick_duration_secs as i64,
            woken_agents:   woken as u64,
        };
        self.writer.write_tick_summary(&row).ok();
    }

    fn on_snapshot(&mut self, tick: Tick, mobility: &MobilityStore, agents: &AgentStore) {
        for i in (0..agents.count).step_by(self.sample_rate) {
            let state = &mobility.states[i];
            let row = AgentSnapshotRow {
                agent_id:         i as u32,
                tick:             tick.0,
                departure_node:   state.departure_node.0,
                in_transit:       state.in_transit,
                destination_node: if state.in_transit {
                    state.destination_node.0
                } else {
                    NodeId::INVALID.0
                },
            };
            self.writer.write_snapshots(std::slice::from_ref(&row)).ok();
        }
    }

    fn on_sim_end(&mut self, _final_tick: Tick) {
        self.writer.finish().ok();
    }
}

// ── Plan builder ──────────────────────────────────────────────────────────────

fn make_plan(depart_home: u32, depart_work: u32) -> ActivityPlan {
    ActivityPlan::new(
        vec![
            ScheduledActivity {
                start_offset_ticks: 0,
                duration_ticks:     depart_home,
                activity_id:        ActivityId(0),
                destination:        Destination::Home,
            },
            ScheduledActivity {
                start_offset_ticks: depart_home,
                duration_ticks:     depart_work - depart_home,
                activity_id:        ActivityId(1),
                destination:        Destination::Work,
            },
            ScheduledActivity {
                start_offset_ticks: depart_work,
                duration_ticks:     TICKS_PER_DAY as u32 - depart_work,
                activity_id:        ActivityId(0),
                destination:        Destination::Home,
            },
        ],
        TICKS_PER_DAY as u32,
    )
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    println!("=== rust_dt  large — 1 M agent commute (Atlanta 100x100 grid) ===");
    println!(
        "Agents: {AGENT_COUNT}  |  Days: {SIM_DAYS}  |  Seed: {SEED}  |  parallel: enabled"
    );
    println!("mem[startup]              {:.0} MB", mem_mb());
    println!();

    // 1. Road network.
    let t_build = Instant::now();
    let (network, all_nodes) = build_network();
    let home_list = home_nodes(&all_nodes);
    let work_list = work_nodes(&all_nodes);
    println!(
        "Road network: {} nodes, {} edges  ({} home nodes, {} work nodes)",
        network.node_count(),
        network.edge_count(),
        home_list.len(),
        work_list.len(),
    );

    // 2. Pre-compute all home↔work routes.
    let router = PrecomputedRouter::build(&network, &home_list, &work_list);
    println!("Pre-computed {} routes", router.routes.len());

    // 3. Agent store with HomeNode / WorkNode components.
    let (mut store, rngs) = AgentStoreBuilder::new(AGENT_COUNT, SEED)
        .register_component::<HomeNode>()
        .register_component::<WorkNode>()
        .build();

    {
        let homes = store.component_mut::<HomeNode>().unwrap();
        for i in 0..AGENT_COUNT {
            homes[i] = HomeNode(home_list[i % home_list.len()]);
        }
    }
    {
        let works = store.component_mut::<WorkNode>().unwrap();
        for i in 0..AGENT_COUNT {
            works[i] = WorkNode(work_list[i % work_list.len()]);
        }
    }

    // 4. Activity plans — staggered 3-way departure (ticks 7, 8, 9 / 16, 17, 18).
    // Build 3 template plans and clone them — Arc makes clone() O(1) with no
    // extra heap allocation, avoiding 1 M fragmented Vec<ScheduledActivity> allocs.
    let templates: [ActivityPlan; 3] =
        std::array::from_fn(|i| make_plan(DEPART_HOME[i], DEPART_WORK[i]));
    let plans: Vec<ActivityPlan> = (0..AGENT_COUNT)
        .map(|i| templates[i % 3].clone())
        .collect();

    // 5. Initial positions at each agent's home node.
    let initial_positions: Vec<NodeId> = (0..AGENT_COUNT)
        .map(|i| home_list[i % home_list.len()])
        .collect();

    println!("Build (store + plans): {:.2}s", t_build.elapsed().as_secs_f64());
    println!("mem[after build]          {:.0} MB", mem_mb());
    println!();

    // 6. Sim config.
    let config = SimConfig {
        start_unix_secs:       1_700_000_000,
        tick_duration_secs:    3_600,
        total_ticks:           SIM_DAYS * TICKS_PER_DAY,
        seed:                  SEED,
        num_threads:           None,
        output_interval_ticks: OUTPUT_INTERVAL_TICKS,
    };
    println!(
        "Sim: {} ticks ({} days), snapshots every {} ticks, 1-in-{} agents sampled",
        config.total_ticks, SIM_DAYS, OUTPUT_INTERVAL_TICKS, SAMPLE_RATE,
    );
    println!();

    // 7. Build sim.
    let contacts_observed = Arc::new(AtomicU64::new(0));
    let mut sim = SimBuilder::new(
            config.clone(), store, rngs,
            DailyCommuteBehavior { contacts_observed: Arc::clone(&contacts_observed) },
            router,
        )
        .plans(plans)
        .network(network)
        .initial_positions(initial_positions)
        .build()?;

    // 8. Output observer.
    let out_dir = Path::new("output/large");
    std::fs::create_dir_all(out_dir)?;
    let mut obs = SampledObserver {
        writer:             CsvWriter::new(out_dir)?,
        start_unix_secs:    config.start_unix_secs,
        tick_duration_secs: config.tick_duration_secs,
        sample_rate:        SAMPLE_RATE,
        start:              Instant::now(),
        total_wakeups:      0,
    };

    println!("mem[before run]           {:.0} MB", mem_mb());
    println!();

    // 9. Run.
    sim.run(&mut obs)?;

    let elapsed = obs.start.elapsed().as_secs_f64();
    println!();
    println!("mem[after run]            {:.0} MB", mem_mb());
    println!("Simulation complete in {:.3}s", elapsed);
    println!(
        "Throughput: {:.1} M wakeups/s  (total {})",
        obs.total_wakeups as f64 / elapsed / 1_000_000.0,
        obs.total_wakeups,
    );
    println!(
        "Contacts sampled:   {} total  ({:.1} M/s)",
        contacts_observed.load(Ordering::Relaxed),
        contacts_observed.load(Ordering::Relaxed) as f64 / elapsed / 1_000_000.0,
    );

    Ok(())
}
