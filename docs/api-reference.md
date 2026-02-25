# API Reference

Complete reference for all public types, traits, and methods across the rust_dt crate family.

---

## dt-core

Foundation types. No `dt-*` dependencies.

**Features:** `serde` — adds `Serialize`/`Deserialize` to all public types.

---

### `AgentId`, `NodeId`, `EdgeId`, `ActivityId`

Strongly-typed integer identifiers. All implement `Copy`, `Clone`, `PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`, `Debug`, `Display`, `Default`.

```rust
pub struct AgentId(pub u32);
pub struct NodeId(pub u32);
pub struct EdgeId(pub u32);
pub struct ActivityId(pub u16);
```

| Method / Constant | Signature | Notes |
|-------------------|-----------|-------|
| `INVALID` | `const Self` | Sentinel: `u32::MAX` / `u16::MAX` |
| `index` | `fn(self) -> usize` | Cast for Vec indexing |
| `Default::default` | `fn() -> Self` | Returns `INVALID` |
| `From<ID> for usize` | implicit | `usize::from(id)` |
| `TryFrom<usize> for ID` | `Result<ID, _>` | Fails if > u32/u16::MAX |

---

### `GeoPoint`

WGS-84 geographic coordinate. Single-precision (~1 m accuracy at equator).

```rust
pub struct GeoPoint { pub lat: f32, pub lon: f32 }
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `new` | `fn(lat: f32, lon: f32) -> Self` | |
| `distance_m` | `fn(self, other: GeoPoint) -> f32` | Haversine formula |
| `within_bbox` | `fn(self, center: GeoPoint, half_deg: f32) -> bool` | Fast AABB rejection |

---

### `Tick`

Absolute simulation tick counter.

```rust
pub struct Tick(pub u64);
```

| Method / Constant | Signature | Notes |
|-------------------|-----------|-------|
| `ZERO` | `const Tick` | `Tick(0)` |
| `offset` | `fn(self, n: u64) -> Tick` | `Tick(self.0 + n)` |
| `since` | `fn(self, earlier: Tick) -> u64` | `self.0 - earlier.0` |
| `Add<u64>` | `fn(self, rhs: u64) -> Tick` | operator `+` |
| `Sub<Tick>` | `fn(self, rhs: Tick) -> u64` | operator `-` between ticks |
| `Display` | | Prints `"T{n}"` |

---

### `SimClock`

Maps ticks to wall-clock time.

```rust
pub struct SimClock {
    pub start_unix_secs:   i64,
    pub tick_duration_secs: u32,
    pub current_tick:      Tick,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `new` | `fn(start_unix_secs: i64, tick_duration_secs: u32) -> Self` | |
| `advance` | `fn(&mut self)` | Increments `current_tick` |
| `elapsed_secs` | `fn(&self) -> i64` | `current_tick * tick_duration_secs` |
| `current_unix_secs` | `fn(&self) -> i64` | `start + elapsed_secs` |
| `elapsed_dhm` | `fn(&self) -> (u64, u32, u32)` | `(days, hours, minutes)` |
| `ticks_for_secs` | `fn(&self, secs: u64) -> u64` | Ceiling division |
| `ticks_for_hours` | `fn(&self, hours: u64) -> u64` | Ceiling division |
| `ticks_for_days` | `fn(&self, days: u64) -> u64` | Ceiling division |

---

### `SimConfig`

Top-level simulation configuration. Passed to `SimBuilder` and propagated throughout.

```rust
pub struct SimConfig {
    pub start_unix_secs:        i64,
    pub tick_duration_secs:     u32,   // default: 3600
    pub total_ticks:            u64,
    pub seed:                   u64,
    pub num_threads:            Option<usize>,  // None = all cores
    pub output_interval_ticks:  u64,            // 0 = never
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `end_tick` | `fn(&self) -> Tick` | `Tick(total_ticks)` |
| `make_clock` | `fn(&self) -> SimClock` | |

---

### `AgentRng`

Per-agent deterministic RNG. `!Sync` — never shared between threads.

```rust
pub struct AgentRng(SmallRng);  // seeded: global_seed XOR (agent_id * GOLDEN_RATIO)
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `new` | `fn(global_seed: u64, agent: AgentId) -> Self` | |
| `inner` | `fn(&mut self) -> &mut SmallRng` | Direct access |
| `random::<T>` | `fn(&mut self) -> T` | Standard distribution |
| `gen_range` | `fn<T, R: SampleRange<T>>(&mut self, range: R) -> T` | |
| `gen_bool` | `fn(&mut self, p: f64) -> bool` | Bernoulli(p) |
| `shuffle` | `fn<T>(&mut self, slice: &mut [T])` | Fisher-Yates |
| `choose` | `fn<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T>` | |

---

### `SimRng`

Global setup RNG. Use for network generation, initial agent placement, etc.

| Method | Signature | Notes |
|--------|-----------|-------|
| `new` | `fn(seed: u64) -> Self` | |
| `child` | `fn(&mut self, offset: u64) -> SimRng` | Derived independent RNG |
| `inner` | `fn(&mut self) -> &mut SmallRng` | |
| `random::<T>` | `fn(&mut self) -> T` | |
| `gen_range` | `fn<T, R>(&mut self, range: R) -> T` | |
| `gen_bool` | `fn(&mut self, p: f64) -> bool` | |

---

### `TransportMode`

```rust
#[non_exhaustive]
pub enum TransportMode {
    None,     // stationary (default)
    Car,
    Walk,
    Bike,
    Transit,
}
```

> **Important:** `#[non_exhaustive]` — always include `_ =>` in external `match` arms.

| Method | Signature | Notes |
|--------|-----------|-------|
| `is_moving` | `fn(self) -> bool` | `false` only for `None` |
| `as_str` | `fn(self) -> &'static str` | `"car"`, `"walk"`, etc. |

---

### `DtError` / `DtResult<T>`

```rust
pub enum DtError {
    AgentNotFound(AgentId),
    NodeNotFound(NodeId),
    Config(String),
    Parse(String),
    Io(std::io::Error),
}
pub type DtResult<T> = Result<T, DtError>;
```

---

## dt-agent

Structure-of-Arrays agent storage.

**Features:** `spatial`, `schedule`, `mobility`, `serde`

---

### `AgentStoreBuilder`

Fluent builder. Consumed by `build()`.

```rust
impl AgentStoreBuilder {
    pub fn new(count: usize, seed: u64) -> Self
    pub fn register_component<T: Default + Send + Sync + 'static>(self) -> Self
    pub fn build(self) -> (AgentStore, AgentRngs)
}
```

---

### `AgentStore`

Main SoA container. Indexed by `agent_id.index()`.

```rust
pub struct AgentStore {
    pub count: usize,

    // feature = "spatial"
    pub node_id:       Vec<NodeId>,
    pub edge_id:       Vec<EdgeId>,
    pub edge_progress: Vec<f32>,

    // feature = "schedule"
    pub next_event_tick:   Vec<Tick>,
    pub current_activity:  Vec<ActivityId>,

    // feature = "mobility"
    pub transport_mode: Vec<TransportMode>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `is_empty` | `fn(&self) -> bool` | |
| `agent_ids` | `fn(&self) -> impl Iterator<Item = AgentId>` | `0..count` |
| `is_at_node` *(spatial)* | `fn(&self, agent: AgentId) -> bool` | |
| `is_moving` *(spatial)* | `fn(&self, agent: AgentId) -> bool` | |
| `component::<T>` | `fn(&self) -> Option<&[T]>` | Read-only slice |
| `component_mut::<T>` | `fn(&mut self) -> Option<&mut Vec<T>>` | Mutable |
| `components` | `fn(&self) -> &ComponentMap` | |
| `components_mut` | `fn(&mut self) -> &mut ComponentMap` | |

---

### `AgentRngs`

Separate from `AgentStore` to allow split borrows during parallel intent phase.

```rust
pub struct AgentRngs { pub inner: Vec<AgentRng> }
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `get_mut` | `fn(&mut self, agent: AgentId) -> &mut AgentRng` | |
| `get_many_mut` | `unsafe fn(&mut self, agents: &[AgentId]) -> Vec<&mut AgentRng>` | Requires unique indices |
| `len` | `fn(&self) -> usize` | |

---

### `ComponentMap`

Type-erased storage for application-defined component types.

| Method | Signature | Notes |
|--------|-----------|-------|
| `new` | `fn() -> Self` | |
| `register::<T>` | `fn(&mut self, current_count: usize)` | Call before any agents added |
| `push_defaults` | `fn(&mut self)` | Extend all vecs by 1 |
| `get::<T>` | `fn(&self) -> Option<&[T]>` | |
| `get_mut::<T>` | `fn(&mut self) -> Option<&mut Vec<T>>` | |
| `contains::<T>` | `fn(&self) -> bool` | |
| `type_count` | `fn(&self) -> usize` | |

---

## dt-spatial

Road network (CSR format with R-tree index) and routing.

**Features:** `osm` (enables PBF loading), `serde`

---

### `RoadNetworkBuilder`

```rust
impl RoadNetworkBuilder {
    pub fn new() -> Self
    pub fn with_capacity(nodes: usize, edges: usize) -> Self
    pub fn add_node(&mut self, pos: GeoPoint) -> NodeId
    pub fn add_directed_edge(&mut self, from: NodeId, to: NodeId, length_m: f32, travel_ms: u32)
    pub fn add_road(&mut self, a: NodeId, b: NodeId, length_m: f32, travel_ms: u32)
    // add_road = add_directed_edge(a→b) + add_directed_edge(b→a)
    pub fn node_pos(&self, id: NodeId) -> GeoPoint
    pub fn node_count(&self) -> usize
    pub fn edge_count(&self) -> usize
    pub fn build(self) -> RoadNetwork   // O(E log E) + O(N log N)

    // feature = "osm"
    pub fn load_from_pbf(path: &Path) -> SpatialResult<RoadNetwork>
}
```

---

### `RoadNetwork`

```rust
pub struct RoadNetwork {
    pub node_pos:       Vec<GeoPoint>,
    pub node_out_start: Vec<u32>,       // CSR row pointers (len = node_count + 1)
    pub edge_from:      Vec<NodeId>,
    pub edge_to:        Vec<NodeId>,
    pub edge_length_m:  Vec<f32>,
    pub edge_travel_ms: Vec<u32>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `empty` | `fn() -> Self` | Zero-node network |
| `node_count` | `fn(&self) -> usize` | |
| `edge_count` | `fn(&self) -> usize` | |
| `is_empty` | `fn(&self) -> bool` | |
| `out_edges` | `fn(&self, node: NodeId) -> impl Iterator<Item = EdgeId>` | CSR slice, zero-alloc |
| `out_degree` | `fn(&self, node: NodeId) -> usize` | |
| `snap_to_node` | `fn(&self, pos: GeoPoint) -> Option<NodeId>` | R-tree nearest neighbor |
| `k_nearest_nodes` | `fn(&self, pos: GeoPoint, k: usize) -> Vec<NodeId>` | R-tree kNN |

---

### `Router` trait

```rust
pub trait Router: Send + Sync {
    fn route(&self, network: &RoadNetwork, from: NodeId, to: NodeId, mode: TransportMode)
        -> Result<Route, SpatialError>;
}
```

**`DijkstraRouter`** — built-in implementation. Mode-dependent speed multipliers:

| Mode | Speed |
|------|-------|
| Car | `edge_travel_ms` from network |
| Walk | 1.4 m/s |
| Bike | 4.2 m/s |
| Transit | 8.3 m/s |

---

### `Route`

```rust
pub struct Route {
    pub edges:              Vec<EdgeId>,
    pub total_travel_secs:  f32,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `travel_ticks` | `fn(&self, tick_duration_secs: u32) -> u64` | Ceiling division |
| `is_trivial` | `fn(&self) -> bool` | Empty edge list |

---

### `SpatialError` / `SpatialResult<T>`

```rust
pub enum SpatialError {
    NoRoute { from: NodeId, to: NodeId },
    NodeNotFound(NodeId),
    Io(std::io::Error),
    Osm(String),  // feature = "osm"
}
```

---

## dt-schedule

Activity plans, wake queue, CSV schedule loading.

---

### `Destination`

```rust
pub enum Destination {
    Node(NodeId),   // concrete node — resolved at plan creation time
    Home,           // sentinel — resolved by behavior via HomeNode component
    Work,           // sentinel — resolved by behavior via WorkNode component
}
```

| Method | Signature |
|--------|-----------|
| `is_resolved` | `fn(&self) -> bool` — true for `Node(_)` |
| `node_id` | `fn(&self) -> Option<NodeId>` |

---

### `ScheduledActivity`

```rust
pub struct ScheduledActivity {
    pub start_offset_ticks: u32,
    pub duration_ticks:     u32,
    pub activity_id:        ActivityId,
    pub destination:        Destination,
}
```

---

### `ActivityPlan`

Cyclic per-agent schedule. Backed by `Arc<[ScheduledActivity]>` — clone is O(1).

```rust
impl ActivityPlan {
    pub fn new(activities: Vec<ScheduledActivity>, cycle_ticks: u32) -> Self
    // Panics if: cycle_ticks == 0, or any start_offset_ticks >= cycle_ticks
    // Sorts activities by start_offset_ticks

    pub fn empty() -> Self
    pub fn is_empty(&self) -> bool
    pub fn len(&self) -> usize
    pub fn activities(&self) -> &[ScheduledActivity]
    pub fn cycle_ticks(&self) -> u32

    pub fn cycle_pos(&self, tick: Tick) -> u32    // tick.0 % cycle_ticks
    pub fn current_activity(&self, tick: Tick) -> Option<&ScheduledActivity>
    pub fn next_wake_tick(&self, tick: Tick) -> Option<Tick>
    // Returns the next activity start tick after `tick`
    // Returns None if plan is empty
}
```

**Cycle semantics:**
- `cycle_pos = tick.0 % cycle_ticks`
- Active activity = last entry with `start_offset_ticks ≤ cycle_pos`
- Next wake tick = the next entry's start, converted to an absolute tick, or next cycle start

---

### `WakeQueue`

```rust
impl WakeQueue {
    pub fn new() -> Self
    pub fn build_from_plans(plans: &[ActivityPlan], sim_start: Tick) -> Self
    pub fn push(&mut self, tick: Tick, agent: AgentId)
    pub fn drain_tick(&mut self, tick: Tick) -> Option<Vec<AgentId>>
    // Returns agents in ascending AgentId order, removes the entry
    pub fn next_tick(&self) -> Option<Tick>
    pub fn len(&self) -> usize          // total agents queued
    pub fn is_empty(&self) -> bool
    pub fn tick_count(&self) -> usize   // distinct future ticks
}
```

---

### CSV Loaders

```rust
pub fn load_plans_csv(path: &Path, agent_count: usize) -> ScheduleResult<Vec<ActivityPlan>>
pub fn load_plans_reader<R: Read>(reader: R, agent_count: usize) -> ScheduleResult<Vec<ActivityPlan>>
```

CSV columns: `agent_id, activity_id, start_offset_ticks, duration_ticks, destination, cycle_ticks`

`destination`: `"home"`, `"work"`, or a `u32` node ID.

Returns a `Vec<ActivityPlan>` of length `agent_count`. Agents with no rows get `ActivityPlan::empty()`.

---

### `ScheduleModifier` trait

Hook for stochastic plan deviations. Applied before each activity is executed.

```rust
pub trait ScheduleModifier: Send + Sync {
    fn modify(&self, agent: AgentId, planned: &ScheduledActivity, rng: &mut AgentRng)
        -> Option<ScheduledActivity>;
    // Some(modified) → use modified activity
    // None → keep as-is
}
```

**Built-ins:** `NoModification` (always returns `None`). Compose with `modifier_a.then(modifier_b)`.

---

## dt-behavior

BehaviorModel trait, Intent enum, SimContext.

---

### `Intent`

```rust
pub enum Intent {
    TravelTo { destination: NodeId, mode: TransportMode },
    WakeAt(Tick),
    SendMessage { to: AgentId, payload: Vec<u8> },
}
```

---

### `SimContext<'a>`

Read-only snapshot passed to behavior methods.

```rust
pub struct SimContext<'a> {
    pub tick:               Tick,
    pub tick_duration_secs: u32,
    pub agents:             &'a AgentStore,
    pub plans:              &'a [ActivityPlan],
}

impl<'a> SimContext<'a> {
    pub fn new(tick: Tick, tick_duration_secs: u32, agents: &'a AgentStore,
               plans: &'a [ActivityPlan]) -> Self
}
```

---

### `BehaviorModel` trait

```rust
pub trait BehaviorModel: Send + Sync + 'static {
    /// Required. Called once per woken agent per tick.
    fn replan(&self, agent: AgentId, ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent>;

    /// Optional. Called for stationary agents co-located with others at the same node.
    fn on_contacts(&self, agent: AgentId, node: NodeId, agents_at_node: &[AgentId],
                   ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent> { vec![] }

    /// Optional. Called when an agent receives a SendMessage intent addressed to it.
    fn on_message(&self, agent: AgentId, from: AgentId, payload: &[u8],
                  ctx: &SimContext<'_>, rng: &mut AgentRng) -> Vec<Intent> { vec![] }
}
```

**`NoopBehavior`** — placeholder that always returns `vec![]`.

---

## dt-mobility

Agent movement state, storage, and engine.

---

### `MovementState`

```rust
pub struct MovementState {
    pub in_transit:       bool,
    pub departure_node:   NodeId,
    pub destination_node: NodeId,
    pub departure_tick:   Tick,
    pub arrival_tick:     Tick,
}

impl MovementState {
    pub fn stationary(node: NodeId, tick: Tick) -> Self
    pub fn progress(&self, now: Tick) -> f32   // [0.0, 1.0]
}
```

---

### `MobilityStore`

```rust
pub struct MobilityStore {
    pub states: Vec<MovementState>,           // Indexed by AgentId
    pub routes: HashMap<AgentId, Route>,      // Sparse: only in-transit agents
}

impl MobilityStore {
    pub fn new(agent_count: usize) -> Self
    pub fn begin_travel<R: Router>(&mut self, agent: AgentId, from: NodeId, to: NodeId,
                                   mode: TransportMode, now: Tick, tick_duration_secs: u32,
                                   router: &R, network: &RoadNetwork) -> Result<Tick, SpatialError>
    // Returns arrival_tick
    pub fn arrive(&mut self, agent: AgentId, now: Tick) -> NodeId
    pub fn progress(&self, agent: AgentId, now: Tick) -> f32
    pub fn in_transit(&self, agent: AgentId) -> bool
}
```

---

### `MobilityEngine<R: Router>`

High-level façade over `MobilityStore`.

```rust
pub struct MobilityEngine<R: Router> {
    pub router: R,
    pub store:  MobilityStore,
}

impl<R: Router> MobilityEngine<R> {
    pub fn new(router: R, agent_count: usize) -> Self
    pub fn place(&mut self, agent: AgentId, node: NodeId, tick: Tick)
    pub fn begin_travel(&mut self, agent: AgentId, destination: NodeId, mode: TransportMode,
                        now: Tick, tick_duration_secs: u32,
                        network: &RoadNetwork) -> Result<Tick, MobilityError>
    pub fn tick_arrivals(&mut self, now: Tick) -> Vec<(AgentId, NodeId)>
    // Returns all (agent, destination_node) pairs that arrived this tick
    pub fn visual_position(&self, agent: AgentId, now: Tick) -> (NodeId, NodeId, f32)
    // (departure_node, destination_node, progress ∈ [0.0, 1.0])
}
```

---

### `MobilityError`

```rust
pub enum MobilityError {
    AlreadyInTransit(AgentId),
    NotPlaced(AgentId),
    Routing(SpatialError),
}
```

---

## dt-sim

Simulation orchestrator. Depends on all other crates.

**Features:** `parallel` (Rayon intent phase), `fx-hash` (FxHashMap for contact index)

---

### `SimBuilder<B, R>`

```rust
impl<B: BehaviorModel, R: Router> SimBuilder<B, R> {
    pub fn new(config: SimConfig, agents: AgentStore, rngs: AgentRngs,
               behavior: B, router: R) -> Self
    pub fn plans(self, plans: Vec<ActivityPlan>) -> Self
    // Default: vec![ActivityPlan::empty(); agent_count]
    pub fn network(self, network: RoadNetwork) -> Self
    // Default: RoadNetwork::empty()
    pub fn initial_positions(self, positions: Vec<NodeId>) -> Self
    // Default: vec![NodeId::INVALID; agent_count]
    pub fn build(self) -> SimResult<Sim<B, R>>
}
```

---

### `Sim<B, R>`

All fields are `pub` for inspection.

```rust
pub struct Sim<B: BehaviorModel, R: Router> {
    pub config:        SimConfig,
    pub clock:         SimClock,
    pub agents:        AgentStore,
    pub rngs:          AgentRngs,
    pub plans:         Vec<ActivityPlan>,
    pub wake_queue:    WakeQueue,
    pub mobility:      MobilityEngine<R>,
    pub behavior:      B,
    pub network:       RoadNetwork,
    pub message_queue: HashMap<AgentId, Vec<(AgentId, Vec<u8>)>>,
}

impl<B: BehaviorModel, R: Router> Sim<B, R> {
    pub fn run<O: SimObserver>(&mut self, observer: &mut O) -> SimResult<()>
    // Process ticks from clock.current_tick to config.end_tick()

    pub fn run_ticks<O: SimObserver>(&mut self, n: u64, observer: &mut O) -> SimResult<()>
    // Process exactly n ticks from current position
}
```

---

### `SimObserver` trait

```rust
pub trait SimObserver {
    fn on_tick_start(&mut self, _tick: Tick) {}
    fn on_tick_end(&mut self, _tick: Tick, _woken: usize) {}
    fn on_snapshot(&mut self, _tick: Tick, _mobility: &MobilityStore, _agents: &AgentStore) {}
    fn on_sim_end(&mut self, _final_tick: Tick) {}
}
```

**Snapshot timing:** `on_snapshot` fires when `output_interval_ticks > 0` and `tick.0 % output_interval_ticks == 0`.

**`NoopObserver`** — implements all methods as no-ops.

---

### `SimError`

```rust
pub enum SimError {
    Config(String),
    AgentCountMismatch { expected: usize, got: usize, what: &'static str },
    Mobility(MobilityError),
}
pub type SimResult<T> = Result<T, SimError>;
```

---

## dt-output

Output writers for simulation data.

**Features:** `sqlite` (rusqlite, bundled), `parquet` (Arrow + Snappy)

Default (no features): CSV writer always available.

---

### `OutputWriter` trait

```rust
pub trait OutputWriter {
    fn write_snapshots(&mut self, rows: &[AgentSnapshotRow]) -> OutputResult<()>;
    fn write_tick_summary(&mut self, row: &TickSummaryRow) -> OutputResult<()>;
    fn finish(&mut self) -> OutputResult<()>;  // idempotent
}
```

---

### Row types

```rust
pub struct AgentSnapshotRow {
    pub agent_id:         u32,
    pub tick:             u64,
    pub departure_node:   u32,
    pub in_transit:       bool,
    pub destination_node: u32,    // u32::MAX if stationary
}

pub struct TickSummaryRow {
    pub tick:           u64,
    pub unix_time_secs: i64,
    pub woken_agents:   u64,
}
```

---

### `CsvWriter`

```rust
impl CsvWriter {
    pub fn new(dir: &Path) -> OutputResult<Self>
    // Creates: {dir}/agent_snapshots.csv, {dir}/tick_summaries.csv
}
impl OutputWriter for CsvWriter {}
```

---

### `SqliteWriter` *(feature: sqlite)*

```rust
impl SqliteWriter {
    pub fn new(path: &Path) -> OutputResult<Self>
    // Creates SQLite db with tables: agent_snapshots, tick_summaries
}
impl OutputWriter for SqliteWriter {}
```

---

### `ParquetWriter` *(feature: parquet)*

```rust
impl ParquetWriter {
    pub fn new(dir: &Path) -> OutputResult<Self>
    // Creates: {dir}/agent_snapshots.parquet, {dir}/tick_summaries.parquet
    // Compression: Snappy
}
impl OutputWriter for ParquetWriter {}
```

---

### `SimOutputObserver<W>`

Bridges `SimObserver` events to an `OutputWriter`. Buffers and flushes on each snapshot.

```rust
impl<W: OutputWriter> SimOutputObserver<W> {
    pub fn new(writer: W, config: &SimConfig) -> Self
    pub fn take_error(&mut self) -> Option<OutputError>  // non-panicking error extraction
    pub fn into_writer(self) -> W
}
impl<W: OutputWriter> SimObserver for SimOutputObserver<W> {}
```

---

### `OutputError`

```rust
pub enum OutputError {
    Io(std::io::Error),
    Csv(csv::Error),
    Sqlite(rusqlite::Error),     // feature: sqlite
    Arrow(arrow::error::ArrowError),  // feature: parquet
    Parquet(parquet::errors::ParquetError),  // feature: parquet
}
pub type OutputResult<T> = Result<T, OutputError>;
```

---

## Feature Flag Summary

| Crate | Feature | Effect |
|-------|---------|--------|
| `dt-core` | `serde` | `Serialize`/`Deserialize` on all public types |
| `dt-agent` | `spatial` | `node_id`, `edge_id`, `edge_progress` SoA fields |
| `dt-agent` | `schedule` | `next_event_tick`, `current_activity` SoA fields |
| `dt-agent` | `mobility` | `transport_mode` SoA field |
| `dt-agent` | `serde` | `Serialize`/`Deserialize` on agent types |
| `dt-spatial` | `osm` | `RoadNetworkBuilder::load_from_pbf` |
| `dt-spatial` | `serde` | `Serialize`/`Deserialize` on network types |
| `dt-sim` | `parallel` | Rayon-parallel intent phase |
| `dt-sim` | `fx-hash` | FxHashMap for contact index (20–50% faster) |
| `dt-output` | `sqlite` | `SqliteWriter` via rusqlite (bundled) |
| `dt-output` | `parquet` | `ParquetWriter` via Arrow + Snappy |
