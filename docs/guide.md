# Building Applications with rust_dt

This guide walks through building a full-featured agent-based simulation step by step, from a bare minimum to a million-agent parallel simulation with output, contacts, and messaging. Each section adds one capability; you can stop at whatever level of complexity your application needs.

The running example throughout this guide is a **daily commute simulation**: agents have homes and workplaces, follow a schedule, travel by car on a road network, and interact with neighbors at shared nodes.

---

## Table of Contents

1. [Project Setup](#1-project-setup)
2. [Simulation Configuration](#2-simulation-configuration)
3. [Custom Agent Components](#3-custom-agent-components)
4. [Building a Road Network](#4-building-a-road-network)
5. [Activity Plans](#5-activity-plans)
6. [The Behavior Model](#6-the-behavior-model)
7. [Building and Running the Simulation](#7-building-and-running-the-simulation)
8. [Contact Events](#8-contact-events)
9. [Agent Messaging](#9-agent-messaging)
10. [Output Writers](#10-output-writers)
11. [Custom SimObserver](#11-custom-simobserver)
12. [Performance Guide](#12-performance-guide)
13. [Loading Real OSM Networks](#13-loading-real-osm-networks)
14. [Loading Schedules from CSV](#14-loading-schedules-from-csv)

---

## 1. Project Setup

Create a new binary crate alongside the rust_dt workspace:

```
my_city/
  Cargo.toml
  src/
    main.rs
```

**`my_city/Cargo.toml`:**

```toml
[package]
name    = "my_city"
version = "0.1.0"
edition = "2024"

[dependencies]
dt-core     = { path = "../rust_dt/crates/dt-core" }
dt-agent    = { path = "../rust_dt/crates/dt-agent", features = ["spatial", "schedule", "mobility"] }
dt-spatial  = { path = "../rust_dt/crates/dt-spatial" }
dt-schedule = { path = "../rust_dt/crates/dt-schedule" }
dt-behavior = { path = "../rust_dt/crates/dt-behavior" }
dt-mobility = { path = "../rust_dt/crates/dt-mobility" }
dt-sim      = { path = "../rust_dt/crates/dt-sim" }
dt-output   = { path = "../rust_dt/crates/dt-output" }
anyhow      = "1"

[features]
parallel = ["dt-sim/parallel"]
```

> **Edition 2024 note:** `gen` is a reserved keyword. If you call `rand::Rng::gen` on a `SmallRng`, write `rng.r#gen()`. The `AgentRng` wrapper exposes `random()` and `gen_range()` instead.

---

## 2. Simulation Configuration

`SimConfig` is the top-level clock and budget configuration. Create it first — its values flow into every other piece of the system.

```rust
use dt_core::SimConfig;

let config = SimConfig {
    // Simulation epoch: a Monday at midnight UTC (Unix timestamp).
    // Tick 0 corresponds to this moment.
    start_unix_secs: 1_700_000_000,

    // How many real-world seconds one tick represents.
    // 3600 = 1 hour per tick (the most common choice).
    tick_duration_secs: 3_600,

    // Total number of ticks to run (7 days × 24 ticks/day = 168).
    total_ticks: 7 * 24,

    // Global random seed. Reproducibility: the same seed always produces
    // identical output regardless of thread count.
    seed: 42,

    // Number of Rayon threads. None = use all logical CPU cores.
    // Only relevant when the `parallel` feature is enabled.
    num_threads: None,

    // Fire on_snapshot() every N ticks. 0 = never.
    output_interval_ticks: 8,
};
```

**Derived helpers on `SimConfig`:**

```rust
config.end_tick()       // Tick(total_ticks)
config.make_clock()     // SimClock — wall-clock time conversion
```

**`SimClock` — converting ticks to wall time:**

```rust
let mut clock = config.make_clock();
println!("{}", clock);                      // e.g. "T0 | 2023-11-14 22:13:20 UTC"
clock.advance();
println!("{}", clock.current_unix_secs()); // start + tick_duration_secs

let (days, hours, mins) = clock.elapsed_dhm();
```

---

## 3. Custom Agent Components

Agents in rust_dt carry data through a **component system** — a type-erased `Vec<T>` per type, indexed by `AgentId`. You define your own data types, register them on the builder, and access them in your behavior model.

### Defining Components

```rust
use dt_core::NodeId;

// Components must be: Default + Clone + Send + Sync + 'static
#[derive(Default, Clone)]
struct HomeNode(NodeId);    // NodeId::INVALID by default (the sentinel value)

#[derive(Default, Clone)]
struct WorkNode(NodeId);

#[derive(Default, Clone)]
struct AgeGroup(u8);        // 0 = child, 1 = adult, 2 = senior

#[derive(Default, Clone)]
struct IsInfected(bool);
```

### Building the Agent Store

```rust
use dt_agent::AgentStoreBuilder;

const AGENT_COUNT: usize = 10_000;

let (mut store, rngs) = AgentStoreBuilder::new(AGENT_COUNT, config.seed)
    .register_component::<HomeNode>()
    .register_component::<WorkNode>()
    .register_component::<AgeGroup>()
    .register_component::<IsInfected>()
    .build();
```

`build()` returns `(AgentStore, AgentRngs)` — the split borrow design allows the parallel intent phase to hold `&mut AgentRngs` + `&AgentStore` simultaneously.

### Populating Components

Access components via `store.component_mut::<T>()` — returns `Option<&mut Vec<T>>`:

```rust
// Assign home/work nodes round-robin across a list of node IDs
{
    let homes = store.component_mut::<HomeNode>().unwrap();
    for i in 0..AGENT_COUNT {
        homes[i] = HomeNode(residential_nodes[i % residential_nodes.len()]);
    }
}
{
    let works = store.component_mut::<WorkNode>().unwrap();
    for i in 0..AGENT_COUNT {
        works[i] = WorkNode(commercial_nodes[i % commercial_nodes.len()]);
    }
}

// Half the population starts infected
{
    let infected = store.component_mut::<IsInfected>().unwrap();
    for i in 0..AGENT_COUNT / 2 {
        infected[i] = IsInfected(true);
    }
}
```

### Reading Components in Behaviors

```rust
// Read-only slice — zero-copy, cache-friendly
if let Some(homes) = ctx.agents.component::<HomeNode>() {
    let my_home = homes[agent.index()].0;
}

// Write access — only in the apply phase, not inside BehaviorModel::replan
if let Some(infected) = store.component_mut::<IsInfected>() {
    infected[agent.index()] = IsInfected(true);
}
```

---

## 4. Building a Road Network

The road network is a **compressed sparse row (CSR)** directed graph with an R-tree spatial index for fast nearest-node queries.

### Programmatic Construction

```rust
use dt_core::GeoPoint;
use dt_spatial::RoadNetworkBuilder;

let mut builder = RoadNetworkBuilder::new();

// Add nodes (returns NodeId in insertion order: 0, 1, 2, ...)
let downtown    = builder.add_node(GeoPoint::new(30.694, -88.043));
let suburbs_n   = builder.add_node(GeoPoint::new(30.750, -88.043));
let suburbs_s   = builder.add_node(GeoPoint::new(30.640, -88.043));
let airport     = builder.add_node(GeoPoint::new(30.694, -88.100));
let university  = builder.add_node(GeoPoint::new(30.694, -87.990));

// add_road adds both directed edges (a→b and b→a)
// args: (from, to, length_metres, travel_time_milliseconds)
builder.add_road(suburbs_n,  downtown,   6_200.0, 420_000);  // ~7 min at 55 km/h
builder.add_road(suburbs_s,  downtown,   6_200.0, 420_000);
builder.add_road(downtown,   airport,    7_000.0, 480_000);
builder.add_road(downtown,   university, 5_000.0, 360_000);

// For one-way streets use add_directed_edge instead of add_road
builder.add_directed_edge(airport, downtown, 7_000.0, 420_000);

let network = builder.build();
println!("{} nodes, {} edges", network.node_count(), network.edge_count());
```

### Grid Networks (for benchmarks)

```rust
// Build an N×M grid — useful for synthetic benchmarks
let rows = 10usize;
let cols = 10usize;
let mut builder = RoadNetworkBuilder::with_capacity(rows * cols, rows * cols * 4);

let mut nodes = vec![vec![]; rows];
for r in 0..rows {
    for c in 0..cols {
        let lat = 30.0 + r as f32 * 0.01;
        let lon = -88.0 + c as f32 * 0.01;
        nodes[r].push(builder.add_node(GeoPoint::new(lat, lon)));
    }
}
// Horizontal and vertical edges
for r in 0..rows {
    for c in 0..cols {
        if c + 1 < cols {
            builder.add_road(nodes[r][c], nodes[r][c+1], 1_000.0, 72_000);
        }
        if r + 1 < rows {
            builder.add_road(nodes[r][c], nodes[r+1][c], 1_000.0, 72_000);
        }
    }
}
let network = builder.build();
```

### Spatial Queries

```rust
// Snap a GPS coordinate to the nearest node
let pos = GeoPoint::new(30.700, -88.043);
if let Some(nearest) = network.snap_to_node(pos) {
    println!("nearest node: {:?}", nearest);
}

// k nearest nodes
let candidates = network.k_nearest_nodes(pos, 5);

// Iterate out-edges from a node (CSR slice, zero-alloc)
for edge_id in network.out_edges(downtown) {
    let to   = network.edge_to[edge_id.index()];
    let dist = network.edge_length_m[edge_id.index()];
}
```

---

## 5. Activity Plans

An `ActivityPlan` is a **cyclic schedule** — a sorted list of `ScheduledActivity` entries that repeat every `cycle_ticks`. The simulation wakes an agent when its next activity begins.

### Building Plans Programmatically

```rust
use dt_core::ActivityId;
use dt_schedule::{ActivityPlan, Destination, ScheduledActivity};

// 24-tick (1 day) cycle: home 0–8, work 8–17, home 17–24
fn daily_plan(depart_morning: u32, depart_evening: u32) -> ActivityPlan {
    ActivityPlan::new(
        vec![
            ScheduledActivity {
                start_offset_ticks: 0,
                duration_ticks:     depart_morning,
                activity_id:        ActivityId(0),   // 0 = "home"
                destination:        Destination::Home,
            },
            ScheduledActivity {
                start_offset_ticks: depart_morning,
                duration_ticks:     depart_evening - depart_morning,
                activity_id:        ActivityId(1),   // 1 = "work"
                destination:        Destination::Work,
            },
            ScheduledActivity {
                start_offset_ticks: depart_evening,
                duration_ticks:     24 - depart_evening,
                activity_id:        ActivityId(0),
                destination:        Destination::Home,
            },
        ],
        24, // cycle_ticks
    )
}
```

**`Destination` variants:**

| Variant | Meaning |
|---------|---------|
| `Destination::Node(id)` | Fixed node — resolved at plan creation time |
| `Destination::Home` | Sentinel — your behavior resolves to the agent's `HomeNode` component |
| `Destination::Work` | Sentinel — resolved to `WorkNode` component |

### Staggered Schedules (for realism)

Stagger departure times across groups to avoid everyone leaving at the same tick:

```rust
// 3 shift groups: depart at hours 7, 8, or 9
let templates = [
    daily_plan(7, 16),
    daily_plan(8, 17),
    daily_plan(9, 18),
];

// Arc-backed cloning is O(1) — no extra allocations for 1 M agents
let plans: Vec<ActivityPlan> = (0..AGENT_COUNT)
    .map(|i| templates[i % 3].clone())
    .collect();
```

### Loading Plans from CSV

```rust
use std::path::Path;
use dt_schedule::load_plans_csv;

let plans = load_plans_csv(Path::new("schedules/my_city.csv"), AGENT_COUNT)?;
```

CSV format:

```
agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks
0,0,0,8,home,24
0,1,8,9,work,24
0,0,17,7,home,24
1,0,0,8,42,168
```

`destination` may be `home`, `work`, or a `u32` node ID. See [Section 14](#14-loading-schedules-from-csv) for full CSV details.

### Week-Long and Custom Cycles

```rust
// 168-tick (1 week) cycle with weekday/weekend variation
let weekly = ActivityPlan::new(
    vec![
        // Mon–Fri work
        ScheduledActivity { start_offset_ticks: 8,   duration_ticks: 9,  activity_id: ActivityId(1), destination: Destination::Work },
        ScheduledActivity { start_offset_ticks: 17,  duration_ticks: 15, activity_id: ActivityId(0), destination: Destination::Home },
        // ... more activities at offsets 32, 41, 56, 65, 80, 89, 104, 113
        // Sat–Sun at home
        ScheduledActivity { start_offset_ticks: 120, duration_ticks: 48, activity_id: ActivityId(0), destination: Destination::Home },
    ],
    168, // cycle_ticks
);
```

### Querying Plans

```rust
let plan = &plans[agent.index()];
let pos = plan.cycle_pos(ctx.tick);               // u32 offset within current cycle

if let Some(act) = plan.current_activity(ctx.tick) {
    println!("activity id: {:?}", act.activity_id);
    println!("destination: {:?}", act.destination);
}

if let Some(next_tick) = plan.next_wake_tick(ctx.tick) {
    // When this agent should next be woken
}
```

---

## 6. The Behavior Model

The `BehaviorModel` trait is the **heart of your application**. It determines what each agent does when it wakes up. The trait is monomorphized at compile time — zero dynamic dispatch.

```rust
pub trait BehaviorModel: Send + Sync + 'static {
    fn replan(&self, agent: AgentId, ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent>;

    // Optional hooks:
    fn on_contacts(&self, agent: AgentId, node: NodeId, agents_at_node: &[AgentId],
                   ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent> { vec![] }
    fn on_message(&self, agent: AgentId, from: AgentId, payload: &[u8],
                  ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent> { vec![] }
}
```

### What's Available in `SimContext`

```rust
pub struct SimContext<'a> {
    pub tick:               Tick,
    pub tick_duration_secs: u32,
    pub agents:             &'a AgentStore,   // all agent data (read-only)
    pub plans:              &'a [ActivityPlan],
}
```

### The `Intent` Enum

Your `replan` function returns a list of intents. The sim applies them after all agents have been queried:

```rust
use dt_behavior::Intent;
use dt_core::{NodeId, Tick, TransportMode};

// Wake this agent again at a specific tick
Intent::WakeAt(Tick(ctx.tick.0 + 6))

// Start travelling to a node
Intent::TravelTo {
    destination: some_node_id,
    mode: TransportMode::Car,
}

// Send a raw message to another agent
Intent::SendMessage {
    to:      other_agent_id,
    payload: b"hello".to_vec(),
}
```

When an agent returns `TravelTo`, the sim calls the router, computes an `arrival_tick`, and automatically re-wakes the agent at arrival. You don't need to explicitly issue `WakeAt` after travel.

### Complete Behavior Example: Daily Commute

```rust
use dt_behavior::{BehaviorModel, Intent, SimContext};
use dt_core::{AgentId, AgentRng, NodeId, TransportMode};
use dt_schedule::Destination;

struct CommuteBehavior;

impl BehaviorModel for CommuteBehavior {
    fn replan(
        &self,
        agent: AgentId,
        ctx:   &SimContext<'_>,
        _rng:  &mut AgentRng,
    ) -> Vec<Intent> {
        // Get the current scheduled activity for this agent
        let Some(activity) = ctx.plans[agent.index()].current_activity(ctx.tick) else {
            return vec![];  // no plan → stay idle
        };

        // Resolve sentinel destinations to concrete node IDs via components
        let dest = match &activity.destination {
            Destination::Home => ctx.agents
                .component::<HomeNode>()
                .map(|v| v[agent.index()].0)
                .unwrap_or(NodeId::INVALID),
            Destination::Work => ctx.agents
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
```

### Probabilistic Behavior with `AgentRng`

Each agent has its own deterministic RNG, seeded from the global seed:

```rust
fn replan(&self, agent: AgentId, ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent> {
    // 30% chance of going to a random errand instead of work today
    if rng.gen_bool(0.30) {
        let errand_nodes = &self.errand_nodes;
        if let Some(&errand) = rng.choose(errand_nodes) {
            return vec![Intent::TravelTo { destination: errand, mode: TransportMode::Walk }];
        }
    }

    // Otherwise follow normal schedule
    // ...
}
```

### Using All Transport Modes

```rust
use dt_core::TransportMode;

// Match on the plan's activity to pick the right mode
let mode = match activity.activity_id.0 {
    0 => TransportMode::Walk,    // short trips on foot
    1 => TransportMode::Car,     // commute by car
    2 => TransportMode::Bike,    // recreational cycling
    3 => TransportMode::Transit, // public transit
    _ => TransportMode::Car,
};
```

Speed assumptions used by `DijkstraRouter`:

| Mode      | Speed   |
|-----------|---------|
| Car       | from OSM `edge_travel_ms` |
| Walk      | 1.4 m/s (~5 km/h) |
| Bike      | 4.2 m/s (~15 km/h) |
| Transit   | 8.3 m/s (~30 km/h) |

---

## 7. Building and Running the Simulation

### SimBuilder

```rust
use dt_sim::SimBuilder;
use dt_spatial::DijkstraRouter;

// Required fields: config, agents, rngs, behavior, router
let mut sim = SimBuilder::new(config.clone(), store, rngs, CommuteBehavior, DijkstraRouter)
    // Optional: provide activity plans (default: all empty)
    .plans(plans)
    // Optional: provide the road network (default: empty)
    .network(network)
    // Optional: starting node per agent (default: NodeId::INVALID)
    .initial_positions(initial_positions)
    .build()?;
```

### Setting Initial Positions

Agents must be placed on a node before they can travel. Build the initial position vector to match agent indices:

```rust
let initial_positions: Vec<NodeId> = (0..AGENT_COUNT)
    .map(|i| {
        store.component::<HomeNode>()
            .map(|v| v[i].0)
            .unwrap_or(NodeId::INVALID)
    })
    .collect();
```

### Running

```rust
use dt_sim::NoopObserver;

// Run the full simulation (ticks 0..config.total_ticks)
sim.run(&mut NoopObserver)?;

// Or run a fixed number of ticks from the current position
sim.run_ticks(24, &mut NoopObserver)?;  // advance 1 day
```

### Inspecting State After the Sim

All `Sim` fields are `pub`:

```rust
// Final agent positions
for i in 0..AGENT_COUNT {
    let state = &sim.mobility.store.states[i];
    println!("agent {:4}: in_transit={}", i, state.in_transit);
}

// Current clock
println!("{}", sim.clock);

// Wake queue stats
println!("{} agents queued for future ticks", sim.wake_queue.len());
```

---

## 8. Contact Events

The `on_contacts` hook is called for each stationary agent at a node that has at least one other co-located agent. Use it to model disease spread, social interaction, information exchange, etc.

```rust
fn on_contacts(
    &self,
    agent:          AgentId,
    node:           NodeId,
    agents_at_node: &[AgentId],  // all agents at this node including `agent`
    ctx:            &SimContext<'_>,
    rng:            &mut AgentRng,
) -> Vec<Intent> {
    // Example: track unique encounters
    let others: Vec<AgentId> = agents_at_node.iter()
        .copied()
        .filter(|&a| a != agent)
        .collect();

    for &neighbor in &others {
        // Read neighbor's infection status
        if let Some(infected) = ctx.agents.component::<IsInfected>() {
            if infected[neighbor.index()].0 {
                // Probabilistic transmission
                if rng.gen_bool(0.05) {
                    // Return an intent to send a message to self
                    // (messaging is the mechanism for "marking" yourself)
                    return vec![Intent::SendMessage {
                        to:      agent,           // self-message
                        payload: b"infected".to_vec(),
                    }];
                }
            }
        }
    }

    vec![]
}
```

**Reservoir sampling for large groups (O(1) space):**

```rust
fn on_contacts(&self, agent: AgentId, _node: NodeId, agents_at_node: &[AgentId],
               _ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent> {
    // Sample up to 4 neighbors without allocating a full list
    let mut sample = [AgentId::INVALID; 4];
    let mut k = 0usize;
    let mut seen = 0usize;

    for &other in agents_at_node {
        if other == agent { continue; }
        if k < 4 {
            sample[k] = other;
            k += 1;
        } else {
            let j = rng.gen_range(0..=seen);
            if j < 4 { sample[j] = other; }
        }
        seen += 1;
    }

    let _my_sample = &sample[..k];  // use the sampled neighbors
    vec![]
}
```

---

## 9. Agent Messaging

Agents can send `Vec<u8>` payloads to any other agent via `Intent::SendMessage`. Messages are buffered during the tick and delivered at the start of the next tick via `on_message`.

**Sending:**

```rust
fn replan(&self, agent: AgentId, ctx: &SimContext<'_>, _rng: &mut AgentRng) -> Vec<Intent> {
    // Notify a supervisor agent
    let payload = format!("arrived:{}", ctx.tick.0).into_bytes();
    vec![Intent::SendMessage { to: AgentId(0), payload }]
}
```

**Receiving:**

```rust
fn on_message(
    &self,
    agent:   AgentId,
    from:    AgentId,
    payload: &[u8],
    ctx:     &SimContext<'_>,
    _rng:    &mut AgentRng,
) -> Vec<Intent> {
    if payload == b"infected" {
        // Schedule a re-evaluation next tick
        return vec![Intent::WakeAt(Tick(ctx.tick.0 + 1))];
    }
    vec![]
}
```

**Self-messages** (send to yourself) are a useful way to set flags that will be processed in `on_message` on the next tick, since the apply phase is sequential and you can't mutate shared state from within `replan`.

---

## 10. Output Writers

`dt-output` provides three backends, all implementing the `OutputWriter` trait.

### CSV (default, always available)

```rust
use std::path::Path;
use dt_output::{CsvWriter, SimOutputObserver};

std::fs::create_dir_all("output/my_city")?;
let writer = CsvWriter::new(Path::new("output/my_city"))?;
let mut obs = SimOutputObserver::new(writer, &config);

sim.run(&mut obs)?;

if let Some(e) = obs.take_error() {
    eprintln!("output error: {e}");
}
```

Creates two files:
- `output/my_city/agent_snapshots.csv` — one row per agent per snapshot tick
- `output/my_city/tick_summaries.csv` — one row per tick (tick, unix_time, woken_agents)

### SQLite (feature: `sqlite`)

```toml
dt-output = { path = "...", features = ["sqlite"] }
```

```rust
use dt_output::SqliteWriter;

let writer = SqliteWriter::new(Path::new("output/my_city/output.db"))?;
let mut obs = SimOutputObserver::new(writer, &config);
sim.run(&mut obs)?;
```

Creates `output.db` with tables `agent_snapshots` and `tick_summaries`. Useful for ad-hoc SQL analysis.

### Parquet (feature: `parquet`)

```toml
dt-output = { path = "...", features = ["parquet"] }
```

```rust
use dt_output::ParquetWriter;

let writer = ParquetWriter::new(Path::new("output/my_city"))?;
let mut obs = SimOutputObserver::new(writer, &config);
sim.run(&mut obs)?;
```

Creates `agent_snapshots.parquet` and `tick_summaries.parquet` with Snappy compression. Ideal for downstream analysis with Pandas, DuckDB, or Spark.

### Controlling Snapshot Frequency

Snapshots are triggered by `output_interval_ticks` in `SimConfig`:

```rust
SimConfig {
    output_interval_ticks: 8,   // snapshot at ticks 0, 8, 16, 24, ...
    // output_interval_ticks: 0,  // never (tick summaries still written)
    ..
}
```

### Snapshot Row Schema

```
AgentSnapshotRow {
    agent_id:         u32,    // AgentId
    tick:             u64,    // absolute tick
    departure_node:   u32,    // current/last node
    in_transit:       bool,   // true if moving
    destination_node: u32,    // u32::MAX if stationary
}
```

---

## 11. Custom SimObserver

For application-specific output — progress reporting, derived statistics, per-tick summaries — implement `SimObserver` directly:

```rust
use dt_core::Tick;
use dt_agent::AgentStore;
use dt_mobility::MobilityStore;
use dt_sim::SimObserver;
use std::time::Instant;

struct MyObserver {
    start:         Instant,
    total_wakeups: u64,
    max_in_transit: u64,
}

impl SimObserver for MyObserver {
    fn on_tick_start(&mut self, tick: Tick) {
        // Called at the beginning of each tick, before any agent processing
    }

    fn on_tick_end(&mut self, tick: Tick, woken: usize) {
        self.total_wakeups += woken as u64;
        let rate = self.total_wakeups as f64
            / self.start.elapsed().as_secs_f64()
            / 1_000_000.0;
        println!("tick {:4}  woken={:>8}  {:.2} M/s", tick.0, woken, rate);
    }

    fn on_snapshot(&mut self, tick: Tick, mobility: &MobilityStore, agents: &AgentStore) {
        // Called at every output_interval_ticks tick
        // Access full mobility state and all agent components
        let in_transit: u64 = mobility.states.iter()
            .filter(|s| s.in_transit)
            .count() as u64;
        self.max_in_transit = self.max_in_transit.max(in_transit);
        println!("  snapshot tick {:4}: {} in transit", tick.0, in_transit);
    }

    fn on_sim_end(&mut self, final_tick: Tick) {
        println!("Sim finished at tick {}. Peak in-transit: {}",
                 final_tick.0, self.max_in_transit);
    }
}
```

### Composing Observers

You can wrap `SimOutputObserver` inside your own struct to get both standard output and custom logic:

```rust
struct ComposedObserver {
    output:       SimOutputObserver<CsvWriter>,
    total_wakeups: u64,
}

impl SimObserver for ComposedObserver {
    fn on_tick_end(&mut self, tick: Tick, woken: usize) {
        self.total_wakeups += woken as u64;
        self.output.on_tick_end(tick, woken);     // delegate to CSV writer
    }
    fn on_snapshot(&mut self, tick: Tick, mob: &MobilityStore, agents: &AgentStore) {
        self.output.on_snapshot(tick, mob, agents);
    }
    fn on_sim_end(&mut self, tick: Tick) {
        self.output.on_sim_end(tick);
    }
}
```

### Sampling for Large Simulations

At 1 M+ agents, writing every agent every snapshot tick is expensive. Sample by agent index:

```rust
fn on_snapshot(&mut self, tick: Tick, mobility: &MobilityStore, agents: &AgentStore) {
    // Write every 20th agent (50 K agents/snapshot at 1 M total)
    for i in (0..agents.count).step_by(20) {
        let state = &mobility.states[i];
        let row = AgentSnapshotRow {
            agent_id:         i as u32,
            tick:             tick.0,
            departure_node:   state.departure_node.0,
            in_transit:       state.in_transit,
            destination_node: if state.in_transit { state.destination_node.0 }
                              else { u32::MAX },
        };
        self.writer.write_snapshots(std::slice::from_ref(&row)).ok();
    }
}
```

---

## 12. Performance Guide

### Enable Parallel Execution

Add the `parallel` feature to dt-sim to enable Rayon parallelism in the intent phase:

```toml
[features]
parallel = ["dt-sim/parallel"]
```

```bash
cargo run -p my_city --release --features parallel
```

The intent phase (`replan`, `on_contacts`, `on_message`) runs in parallel across woken agents. The apply phase is always sequential (determinism guarantee). Result: **linear scaling with core count** for the intent phase.

### Pre-compute Routes

For simulations where all origin-destination pairs are known in advance, pre-compute routes to eliminate Dijkstra overhead during the run:

```rust
use std::collections::HashMap;
use dt_spatial::{DijkstraRouter, RoadNetwork, Route, Router, SpatialError};
use dt_core::{NodeId, TransportMode};

struct PrecomputedRouter {
    routes: HashMap<(u32, u32), Route>,
}

impl PrecomputedRouter {
    fn build(network: &RoadNetwork, origins: &[NodeId], destinations: &[NodeId]) -> Self {
        let base = DijkstraRouter;
        let mut routes = HashMap::new();
        for &o in origins {
            for &d in destinations {
                if let Ok(r) = base.route(network, o, d, TransportMode::Car) {
                    routes.insert((o.0, d.0), r);
                }
                if let Ok(r) = base.route(network, d, o, TransportMode::Car) {
                    routes.insert((d.0, o.0), r);
                }
            }
        }
        Self { routes }
    }
}

impl Router for PrecomputedRouter {
    fn route(&self, _net: &RoadNetwork, from: NodeId, to: NodeId,
             _mode: TransportMode) -> Result<Route, SpatialError> {
        self.routes.get(&(from.0, to.0))
            .cloned()
            .ok_or(SpatialError::NoRoute { from, to })
    }
}
```

Build with `DijkstraRouter` first, then plug in `PrecomputedRouter` for the sim run. Pre-computation of 100×100 pairs takes ~10 ms; every subsequent lookup is O(1).

### Alternative Global Allocator (Windows)

On Windows, the default allocator fragments badly at millions of agents. Use [mimalloc](https://crates.io/crates/mimalloc):

```toml
[dependencies]
mimalloc = { version = "0.1", default-features = false }
```

```rust
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

Typical improvement: 20–40% faster allocation throughput on Windows with large agent counts.

### FxHash for Contact Index

The `parallel` feature includes a contact index (which nodes have which agents). Enable `fx-hash` to replace the default SipHash with FxHashMap:

```toml
dt-sim = { path = "...", features = ["parallel", "fx-hash"] }
```

20–50% faster contact index lookups on integer keys.

### Arc-Backed Plan Cloning

When many agents share the same schedule template, use `ActivityPlan::clone()` — it internally uses `Arc<[ScheduledActivity]>` so cloning is O(1) with no heap allocation:

```rust
let templates = [plan_a, plan_b, plan_c];
let plans: Vec<ActivityPlan> = (0..AGENT_COUNT)
    .map(|i| templates[i % 3].clone())  // O(1) clone via Arc
    .collect();
```

### Sparse Output

Set `output_interval_ticks` to a larger value and/or sample agents in your observer to reduce I/O overhead. For a 1 M agent sim, sampling 1-in-20 agents every 8 ticks reduces snapshot volume by 160×.

---

## 13. Loading Real OSM Networks

The `dt-spatial` crate includes a full OSM PBF loader behind the `osm` feature flag. It performs a two-pass read: first collecting all node coordinates, then building directed edges from car-drivable `highway=*` ways. One-way roads (explicit `oneway=yes` tags, plus motorways by convention) add a single directed edge; two-way roads add both directions.

**Enable the feature:**

```toml
dt-spatial = { path = "...", features = ["osm"] }
```

**Load a network:**

```rust
use std::path::Path;
use dt_spatial::osm::load_from_pbf;

let network = load_from_pbf(Path::new("my_city.osm.pbf"))?;
println!("{} nodes, {} edges loaded from OSM", network.node_count(), network.edge_count());
```

OSM PBF files can be downloaded from [Geofabrik](https://download.geofabrik.de/) or [BBBike](https://download.bbbike.org/). For a city-sized area (~400 K population), a typical PBF file is 20–100 MB and loads in a few seconds.

**Supported highway types and assumed speeds:**

| OSM tag | Speed |
|---------|-------|
| `motorway` / `motorway_link` | 65 mph (29.1 m/s) |
| `trunk` / `trunk_link` | 55 mph (24.6 m/s) |
| `primary` / `primary_link` | 45 mph (20.1 m/s) |
| `secondary` / `secondary_link` | 40 mph (17.9 m/s) |
| `tertiary` / `tertiary_link` | 30 mph (13.4 m/s) |
| `residential` / `living_street` | 20 mph (8.9 m/s) |
| `service` / `unclassified` | 15 mph (6.7 m/s) |
| `footway`, `path`, `cycleway`, `pedestrian`, `steps`, `track` | excluded |

These are conservative urban defaults. The loader does not currently parse `maxspeed` tags — if you need speed-limit-accurate travel times, use `RoadNetworkBuilder` directly and populate `edge_travel_ms` from your own OSM parsing.

**Snap agent home/work locations to the network:**

```rust
use dt_core::GeoPoint;

let home_gps = GeoPoint::new(30.694, -88.043);
let home_node = network.snap_to_node(home_gps)
    .expect("no nodes in network near this coordinate");
```

**Memory note:** The loader buffers all OSM node coordinates in a `HashMap<i64, GeoPoint>` during the first pass (needed because OSM ways reference nodes by integer ID). For a city-scale PBF this is roughly 100–200 MB. The map is freed before the R-tree is built.

---

## 14. Loading Schedules from CSV

For large populations with heterogeneous schedules, load from CSV rather than generating programmatically:

```rust
use std::path::Path;
use dt_schedule::load_plans_csv;

let plans = load_plans_csv(Path::new("data/schedules.csv"), AGENT_COUNT)?;
```

### CSV Format

```
agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks
0,0,0,8,home,24
0,1,8,9,work,24
0,0,17,7,home,24
1,0,0,8,1234,168
2,1,40,8,work,168
```

| Column | Type | Notes |
|--------|------|-------|
| `agent_id` | `u32` | 0-indexed; agents with no rows get `ActivityPlan::empty()` |
| `activity_id` | `u16` | Application-defined; use your own constants |
| `start_offset_ticks` | `u32` | Must be < `cycle_ticks` |
| `duration_ticks` | `u32` | Informational only; not enforced by the engine |
| `destination` | `"home"`, `"work"`, or `u32` | Node ID or sentinel |
| `cycle_ticks` | `u32` | Must match all rows for the same agent |

Rows are sorted by `start_offset_ticks` automatically. Activities are processed modulo `cycle_ticks`.

### Reading from an Embedded String (for tests)

```rust
use std::io::Cursor;
use dt_schedule::load_plans_reader;

const CSV: &str = "\
agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks\n\
0,0,0,8,home,24\n\
0,1,8,9,work,24\n\
";

let plans = load_plans_reader(Cursor::new(CSV), 1)?;
```

---

## Complete Minimal Application

Putting it all together — a complete working main.rs for a small daily-commute simulation:

```rust
use std::io::Cursor;
use std::path::Path;

use anyhow::Result;
use dt_agent::AgentStoreBuilder;
use dt_behavior::{BehaviorModel, Intent, SimContext};
use dt_core::{AgentId, AgentRng, NodeId, SimConfig, TransportMode};
use dt_output::{CsvWriter, SimOutputObserver};
use dt_schedule::{Destination, load_plans_reader};
use dt_sim::{SimBuilder, NoopObserver};
use dt_spatial::{DijkstraRouter, RoadNetworkBuilder};
use dt_core::GeoPoint;

// --- Custom components ---

#[derive(Default, Clone)] struct HomeNode(NodeId);
#[derive(Default, Clone)] struct WorkNode(NodeId);

// --- Behavior ---

struct DailyCommute;

impl BehaviorModel for DailyCommute {
    fn replan(&self, agent: AgentId, ctx: &SimContext<'_>, _rng: &mut AgentRng) -> Vec<Intent> {
        let Some(act) = ctx.plans[agent.index()].current_activity(ctx.tick) else {
            return vec![];
        };
        let dest = match &act.destination {
            Destination::Home => ctx.agents.component::<HomeNode>()
                .map(|v| v[agent.index()].0).unwrap_or(NodeId::INVALID),
            Destination::Work => ctx.agents.component::<WorkNode>()
                .map(|v| v[agent.index()].0).unwrap_or(NodeId::INVALID),
            Destination::Node(n) => *n,
        };
        if dest == NodeId::INVALID { return vec![]; }
        vec![Intent::TravelTo { destination: dest, mode: TransportMode::Car }]
    }
}

// --- Schedule ---

const SCHEDULE: &str = "\
agent_id,activity_id,start_offset_ticks,duration_ticks,destination,cycle_ticks\n\
0,0,0,8,home,24\n\
0,1,8,9,work,24\n\
0,0,17,7,home,24\n\
1,0,0,8,home,24\n\
1,1,8,9,work,24\n\
1,0,17,7,home,24\n\
";

fn main() -> Result<()> {
    const N: usize = 2;
    const SEED: u64 = 42;

    // 1. Network
    let mut nb = RoadNetworkBuilder::new();
    let home_node = nb.add_node(GeoPoint::new(30.69, -88.05));
    let work_node = nb.add_node(GeoPoint::new(30.70, -88.04));
    nb.add_road(home_node, work_node, 1_500.0, 120_000);
    let network = nb.build();

    // 2. Agents
    let (mut store, rngs) = AgentStoreBuilder::new(N, SEED)
        .register_component::<HomeNode>()
        .register_component::<WorkNode>()
        .build();
    store.component_mut::<HomeNode>().unwrap().fill(HomeNode(home_node));
    store.component_mut::<WorkNode>().unwrap().fill(WorkNode(work_node));

    // 3. Plans
    let plans = load_plans_reader(Cursor::new(SCHEDULE), N)?;

    // 4. Initial positions
    let positions = vec![home_node; N];

    // 5. Config
    let config = SimConfig {
        start_unix_secs: 1_700_000_000,
        tick_duration_secs: 3_600,
        total_ticks: 7 * 24,
        seed: SEED,
        num_threads: None,
        output_interval_ticks: 1,
    };

    // 6. Build & run
    std::fs::create_dir_all("output/my_city")?;
    let writer = CsvWriter::new(Path::new("output/my_city"))?;
    let mut obs = SimOutputObserver::new(writer, &config);

    let mut sim = SimBuilder::new(config.clone(), store, rngs, DailyCommute, DijkstraRouter)
        .plans(plans)
        .network(network)
        .initial_positions(positions)
        .build()?;

    sim.run(&mut obs)?;
    println!("Done in {} ticks.", config.total_ticks);
    Ok(())
}
```
