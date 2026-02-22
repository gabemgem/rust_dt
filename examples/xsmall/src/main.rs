//! xsmall — smallest example for the rust_dt digital twin framework.
//!
//! Simulates 8 agents commuting across a synthetic 5-node road network
//! inspired by the geography of Mobile, Alabama.  Scale comment:
//! the real population is ~400 K; swap AGENT_COUNT and a real OSM network
//! to run at full scale on a 50-core workstation.

mod network;

use std::io::Cursor;
use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use dt_agent::AgentStoreBuilder;
use dt_behavior::{BehaviorModel, Intent, SimContext};
use dt_core::{AgentId, AgentRng, NodeId, SimConfig, TransportMode};
use dt_output::{CsvWriter, SimOutputObserver};
use dt_schedule::{Destination, load_plans_reader};
use dt_sim::{SimBuilder, SimObserver};
use dt_spatial::DijkstraRouter;

use network::build_network;

// ── Constants ─────────────────────────────────────────────────────────────────

const AGENT_COUNT:           usize = 8;
const SEED:                  u64   = 42;
const TICK_DURATION_SECS:    u32   = 3_600; // 1 tick = 1 hour
const SIM_DAYS:              u64   = 7;
const OUTPUT_INTERVAL_TICKS: u64   = 1;     // snapshot every tick (captures commute movement)

// ── Application components ────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct HomeNode(NodeId);

#[derive(Default, Clone)]
struct WorkNode(NodeId);

// ── Schedule CSV ──────────────────────────────────────────────────────────────

// 24-tick daily cycle (cycle_ticks=24, 1 tick = 1 hour).
// activity_id 0 = home, 1 = work.
// Pattern repeated identically for all 8 agents.
const SCHEDULE_CSV: &str = "\
agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks\n\
0,0,0,8,home,24\n\
0,1,8,9,work,24\n\
0,0,17,7,home,24\n\
1,0,0,8,home,24\n\
1,1,8,9,work,24\n\
1,0,17,7,home,24\n\
2,0,0,8,home,24\n\
2,1,8,9,work,24\n\
2,0,17,7,home,24\n\
3,0,0,8,home,24\n\
3,1,8,9,work,24\n\
3,0,17,7,home,24\n\
4,0,0,8,home,24\n\
4,1,8,9,work,24\n\
4,0,17,7,home,24\n\
5,0,0,8,home,24\n\
5,1,8,9,work,24\n\
5,0,17,7,home,24\n\
6,0,0,8,home,24\n\
6,1,8,9,work,24\n\
6,0,17,7,home,24\n\
7,0,0,8,home,24\n\
7,1,8,9,work,24\n\
7,0,17,7,home,24\n\
";

// ── Behavior model ────────────────────────────────────────────────────────────

struct DailyCommuteBehavior;

impl BehaviorModel for DailyCommuteBehavior {
    fn replan(
        &self,
        agent: AgentId,
        ctx:   &SimContext<'_>,
        _rng:  &mut AgentRng,
    ) -> Vec<Intent> {
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
}

// ── Observer wrapper to count rows ───────────────────────────────────────────

struct CountingObserver<W: dt_output::writer::OutputWriter> {
    inner:          SimOutputObserver<W>,
    snapshot_rows:  usize,
    summary_rows:   usize,
}

impl<W: dt_output::writer::OutputWriter> CountingObserver<W> {
    fn new(inner: SimOutputObserver<W>) -> Self {
        Self { inner, snapshot_rows: 0, summary_rows: 0 }
    }
}

impl<W: dt_output::writer::OutputWriter> SimObserver for CountingObserver<W> {
    fn on_tick_end(&mut self, tick: dt_core::Tick, woken: usize) {
        self.summary_rows += 1;
        self.inner.on_tick_end(tick, woken);
    }

    fn on_snapshot(
        &mut self,
        tick:     dt_core::Tick,
        mobility: &dt_mobility::MobilityStore,
        agents:   &dt_agent::AgentStore,
    ) {
        self.snapshot_rows += agents.count;
        self.inner.on_snapshot(tick, mobility, agents);
    }

    fn on_sim_end(&mut self, final_tick: dt_core::Tick) {
        self.inner.on_sim_end(final_tick);
    }
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    println!("=== xsmall — rust_dt digital twin ===");
    println!("Agents: {AGENT_COUNT}  |  Days: {SIM_DAYS}  |  Seed: {SEED}");
    println!("(Scale to ~400 K agents + real OSM network for production run)");
    println!();

    // 1. Build road network.
    let (network, nodes) = build_network();
    let [north_residential, south_residential, downtown, commerce_park, _connector] = nodes;
    println!(
        "Road network: {} nodes, {} edges",
        network.node_count(),
        network.edge_count()
    );

    // 2. Build agent store with custom components.
    let (mut store, rngs) = AgentStoreBuilder::new(AGENT_COUNT, SEED)
        .register_component::<HomeNode>()
        .register_component::<WorkNode>()
        .build();

    // Agents 0–4: home = north_residential, work = downtown.
    // Agents 5–7: home = south_residential, work = commerce_park.
    {
        let homes = store.component_mut::<HomeNode>().unwrap();
        homes[..5].fill(HomeNode(north_residential));
        homes[5..].fill(HomeNode(south_residential));
    }
    {
        let works = store.component_mut::<WorkNode>().unwrap();
        works[..5].fill(WorkNode(downtown));
        works[5..].fill(WorkNode(commerce_park));
    }

    // 3. Load plans from the embedded schedule CSV.
    let plans = load_plans_reader(Cursor::new(SCHEDULE_CSV), AGENT_COUNT)?;
    println!("Loaded {} activity plans", plans.len());

    // 4. Initial positions: each agent starts at their home node.
    let initial_positions: Vec<NodeId> = (0..AGENT_COUNT)
        .map(|i| {
            store
                .component::<HomeNode>()
                .map(|v| v[i].0)
                .unwrap_or(NodeId::INVALID)
        })
        .collect();

    // 5. Sim config.
    let config = SimConfig {
        start_unix_secs:       1_700_000_000, // fixed reference Monday 00:00 UTC
        tick_duration_secs:    TICK_DURATION_SECS,
        total_ticks:           SIM_DAYS * 24,
        seed:                  SEED,
        num_threads:           None, // all logical cores
        output_interval_ticks: OUTPUT_INTERVAL_TICKS,
    };
    println!(
        "Sim: {} ticks ({} days × 24 h), output every {} ticks",
        config.total_ticks, SIM_DAYS, OUTPUT_INTERVAL_TICKS
    );
    println!();

    // 6. Build sim.
    let mut sim = SimBuilder::new(config.clone(), store, rngs, DailyCommuteBehavior, DijkstraRouter)
        .plans(plans)
        .network(network)
        .initial_positions(initial_positions)
        .build()?;

    // 7. Set up output.
    std::fs::create_dir_all("output/xsmall")?;
    let writer = CsvWriter::new(Path::new("output/xsmall"))?;
    let inner_obs = SimOutputObserver::new(writer, &config);
    let mut obs = CountingObserver::new(inner_obs);

    // 8. Run.
    let t0 = Instant::now();
    sim.run(&mut obs)?;
    let elapsed = t0.elapsed();

    if let Some(e) = obs.inner.take_error() {
        eprintln!("output error: {e}");
    }

    // 9. Summary.
    println!("Simulation complete in {:.3} s", elapsed.as_secs_f64());
    println!(
        "  agent_snapshots.csv : {} rows",
        obs.snapshot_rows
    );
    println!(
        "  tick_summaries.csv  : {} rows",
        obs.summary_rows
    );
    println!();

    // 10. Final agent positions table.
    println!("{:<10} {:<8} {:<12}", "Agent", "Transit", "Node");
    println!("{}", "-".repeat(32));
    for i in 0..AGENT_COUNT {
        let state = &sim.mobility.store.states[i];
        println!(
            "{:<10} {:<8} {:<12}",
            i,
            if state.in_transit { "yes" } else { "no" },
            state.departure_node.0,
        );
    }

    Ok(())
}
