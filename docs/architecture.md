# rust_dt Architecture

This document explains the internal design of the rust_dt simulation engine: how the tick loop works, why memory is laid out the way it is, how determinism is maintained under parallelism, and the key borrow-checker patterns that make Rayon safe.

---

## Table of Contents

1. [High-Level Design Goals](#1-high-level-design-goals)
2. [Crate Dependency Graph](#2-crate-dependency-graph)
3. [Structure of Arrays (SoA) Memory Layout](#3-structure-of-arrays-soa-memory-layout)
4. [The Wake Queue](#4-the-wake-queue)
5. [The Tick Loop (Four Phases)](#5-the-tick-loop-four-phases)
6. [Determinism Under Parallelism](#6-determinism-under-parallelism)
7. [The Borrow-Split Pattern](#7-the-borrow-split-pattern)
8. [The Component System](#8-the-component-system)
9. [Movement Model](#9-movement-model)
10. [Routing Architecture](#10-routing-architecture)
11. [Time Model](#11-time-model)
12. [RNG Design](#12-rng-design)
13. [Design Decisions](#13-design-decisions)

---

## 1. High-Level Design Goals

| Goal | Target |
|------|--------|
| Scale | 5 M agents × 365 days × 1 tick/hour |
| Performance | < 5 minutes on a 50-core, 128 GB workstation |
| Determinism | Same output for any thread count, always |
| Extensibility | Application behavior injected with zero overhead |
| Memory efficiency | < 1 KB/agent for the framework itself |

The core trade-offs that follow from these goals:

- **Fixed timestep over event-driven**: At 5 M agents × 365 days × 24 ticks = 43.8 B potential events. Even a minimally-allocated BinaryHeap would require > 100 GB of heap churn. Fixed-tick + sparse wake queue is cheaper.
- **Structure of Arrays over struct of agents**: CPU cache lines are 64 bytes. A 1 M-agent array of large agent structs wastes cache bandwidth when Rayon iterates over one field. Separate `Vec<T>` per field means each Rayon worker pulls only the data it needs.
- **Monomorphic behavior over dynamic dispatch**: `BehaviorModel` is a generic parameter on `Sim<B, R>`. The compiler generates a single code path per application; no vtable lookups in the hot loop.

---

## 2. Crate Dependency Graph

```
dt-core              (IDs, GeoPoint, Tick, SimClock, SimConfig, AgentRng)
  │
  ├── dt-agent        (AgentStore SoA, AgentRngs, ComponentMap)
  │     │
  │     ├── dt-spatial   (RoadNetwork CSR + R-tree, DijkstraRouter, Router trait)
  │     │
  │     ├── dt-schedule  (ActivityPlan, WakeQueue, CSV loader)
  │     │
  │     └── dt-behavior  (BehaviorModel trait, Intent, SimContext)
  │           │
  │           └── dt-mobility  (MovementState, MobilityStore, MobilityEngine)
  │                 │
  │                 └── dt-sim  (Sim<B,R>, SimBuilder, SimObserver, tick loop)
  │                       │
  │                       └── dt-output  (CsvWriter, SqliteWriter, ParquetWriter)
```

Each crate depends only on what's strictly necessary. Applications can depend on a subset — unused crates compile to nothing.

---

## 3. Structure of Arrays (SoA) Memory Layout

`AgentStore` stores each field in a separate `Vec` indexed by `AgentId::index()`:

```
agent_id    0    1    2    3    4   ...  N-1
            ─────────────────────────────────
node_id  [  5    12   3    5    12  ...  3  ]  ← Vec<NodeId>
mode     [  Car  Walk None Car  Car ...  None]  ← Vec<TransportMode>
activity [  0    1    0    1    0   ...  0  ]  ← Vec<ActivityId>
HomeNode [  5    5    12   12   3   ...  7  ]  ← Vec<HomeNode>  (component)
WorkNode [  8    8    9    9    10  ...  11 ]  ← Vec<WorkNode>  (component)
```

**Why this matters for cache efficiency:**

When Rayon iterates over all woken agents and calls `replan(agent, ctx, rng)`, each worker thread accesses:

- `AgentRngs::inner[agent.index()]` — sequential within the RNG vec
- `plans[agent.index()]` — sequential within the plan vec
- `agents.component::<HomeNode>()[agent.index()]` — sequential within the HomeNode vec

Each of these is a cache-line-friendly sequential scan. If agents were stored as a `Vec<AgentStruct>` with all fields interleaved, a Rayon worker fetching one field would pull in (and evict) all the other fields of that agent.

**Comparison:**

| Layout | 64 agents per cache line (HomeNode only) | Effective cache use |
|--------|------------------------------------------|---------------------|
| SoA (separate Vecs) | ~Yes | High |
| AoS (Vec<AgentStruct> with 20 fields) | ~3 agents per cache line | Low |

---

## 4. The Wake Queue

```rust
pub struct WakeQueue {
    inner: BTreeMap<Tick, Vec<AgentId>>,
}
```

Only agents that need to act are processed each tick. An agent is in the queue only if it has something to do — most agents at most ticks are absent from the queue entirely.

**Invariant**: Each `Vec<AgentId>` in the BTreeMap is kept in **ascending order**. This is what makes the apply phase deterministic.

**Lifecycle of an agent in the queue:**

```
Sim start:  WakeQueue::build_from_plans()
                ↓
            plan.next_wake_tick(Tick(0)) → Some(Tick(8))
                ↓
            push(Tick(8), agent_id)
                ↓
Tick 8:     drain_tick(Tick(8)) → [agent_id, ...]
                ↓
            replan() → Intent::TravelTo { destination: work, ... }
                ↓
            begin_travel() → arrival_tick = Tick(9)
                ↓
            push(Tick(9), agent_id)  ← will be drained at tick 9

Tick 9:     tick_arrivals(Tick(9)) → agent arrives at destination
                ↓
            plan.next_wake_tick(Tick(9)) → Some(Tick(17))
                ↓
            push(Tick(17), agent_id)
```

An agent is **never** processed at a tick it isn't woken for. Total processing cost per tick is proportional to woken agents, not total agent count.

---

## 5. The Tick Loop (Four Phases)

```
for tick in 0..total_ticks:

  ┌─────────────────────────────────────────┐
  │  Phase 1: Arrivals                      │
  │  mobility.tick_arrivals(now)            │
  │  → for each arrived agent:              │
  │      state.in_transit = false           │
  │      wake_queue.push(next_wake_tick)    │
  └─────────────────────────────────────────┘
               ↓
  ┌─────────────────────────────────────────┐
  │  Phase 2: Drain Wake Queue              │
  │  let woken = wake_queue.drain_tick(now) │
  │  → Vec<AgentId> in ascending order      │
  └─────────────────────────────────────────┘
               ↓
  ┌─────────────────────────────────────────┐
  │  Phase 3: Intent (read-only)            │
  │  for agent in woken:                    │
  │      intents[agent] = behavior.replan() │
  │  [parallel with Rayon if feature=on]    │
  └─────────────────────────────────────────┘
               ↓
  ┌─────────────────────────────────────────┐
  │  Phase 4: Apply (sequential)            │
  │  for (agent, intents) in ascending ID:  │
  │    WakeAt(t)   → push to queue          │
  │    TravelTo{d} → begin_travel, push     │
  │    SendMessage → buffer for next tick   │
  └─────────────────────────────────────────┘
               ↓
  observer.on_tick_end(now, woken.len())
  if snapshot interval: observer.on_snapshot(now, &mobility, &agents)
```

**Phase 3 and 4 are separated** so that the intent phase can run in parallel without data races. Phase 3 is purely read-only: it reads `AgentStore`, `plans`, `SimContext`, and writes only to per-agent `AgentRng` (which is exclusively owned per agent in the parallel path). Phase 4 is sequential and mutates the sim.

---

## 6. Determinism Under Parallelism

Three properties guarantee identical output regardless of thread count:

**1. Per-agent RNG isolation**

Each agent's `AgentRng` is seeded as:

```
seed = global_seed XOR (agent_id.0 as u64 * 0x9e3779b97f4a7c15)
```

The golden-ratio-derived constant ensures seeds are maximally spread across the u64 space. Agents never share an RNG, so parallel iteration order doesn't affect any agent's random sequence.

**2. Intent collection and sorting**

When `parallel` is enabled, intents are collected into a `Vec<(AgentId, Vec<Intent>)>` and sorted by `AgentId` before the apply phase. The apply phase always processes agents in the same order.

**3. Wake queue ordering**

`drain_tick` returns agents in ascending `AgentId` order (BTreeMap guarantees). Arrivals are processed before the intent phase, also in ascending order.

**Result**: Two runs on 1 core and 64 cores produce bit-identical output, including identical routes, tick snapshots, and message delivery.

---

## 7. The Borrow-Split Pattern

Rust's borrow checker forbids holding `&mut X` and `&Y` simultaneously when they alias. The parallel intent phase needs:

- `&mut AgentRngs` — each worker needs exclusive `&mut AgentRng` for its agent
- `&AgentStore` — all workers need shared read access to agent data
- `&[ActivityPlan]` — shared read access
- `&B` — shared read access to the behavior model

If `AgentRngs` lived inside `AgentStore`, borrowing `&mut store.rngs` would conflict with `&store`. The solution is **separation**:

```rust
// AgentStoreBuilder::build() returns both separately:
let (store, rngs) = AgentStoreBuilder::new(N, SEED).build();

// Sim holds them as separate fields:
pub struct Sim<B, R> {
    pub agents: AgentStore,   // &agents is fine alongside &mut rngs
    pub rngs:   AgentRngs,    // &mut rngs doesn't alias &agents
    ...
}
```

Inside the parallel intent phase, the borrow checker must see them as separate field borrows:

```rust
// Must extract explicit named bindings — the borrow checker
// tracks per-field borrows but not through struct method calls:
let agents   = &self.agents;
let plans    = &*self.plans;
let behavior = &self.behavior;
let rngs     = &mut self.rngs;

// Now rayon can zip rngs with woken agents:
let rng_refs = rngs.get_many_mut(&woken);
woken.par_iter().zip(rng_refs).map(|(&agent, rng)| {
    let ctx = SimContext::new(tick, tick_duration_secs, agents, plans);
    (agent, behavior.replan(agent, &ctx, rng))
}).collect()
```

`AgentRngs::get_many_mut` is `unsafe` — it hands out multiple `&mut AgentRng` simultaneously by asserting (with a debug-mode check) that all indices are distinct.

Similarly, `MobilityEngine` splits its borrow:

```rust
// WRONG — borrows self twice:
self.store.begin_travel(..., &self.router, ...);

// RIGHT — extract router reference first:
let router = &self.router;
self.store.begin_travel(..., router, ...);
```

---

## 8. The Component System

`ComponentMap` is a `HashMap<TypeId, Box<dyn ComponentVec>>` that stores one `Vec<T>` per registered type:

```rust
pub trait ComponentVec: Send + Sync + 'static {
    fn push_default(&mut self);  // called once per new agent
    fn len(&self) -> usize;
}

pub struct TypedComponentVec<T: Default + Send + Sync + 'static>(pub Vec<T>);
```

**Registration** happens at build time via `AgentStoreBuilder::register_component::<T>()`. The builder calls `push_default()` once per agent for each registered type, initializing all components to `T::default()`.

**Access** is by type identity:

```rust
store.component::<HomeNode>()      // Option<&[HomeNode]>
store.component_mut::<HomeNode>()  // Option<&mut Vec<HomeNode>>
```

`TypeId` is stable within a single compilation — component access is safe and zero-cost at runtime (one `HashMap` lookup, then a slice reference).

**Why not a macro or derive?** Components are user-defined types. The framework has no compile-time knowledge of what fields an application needs. The `Box<dyn ComponentVec>` design lets applications register any number of arbitrary types without modifying framework code.

---

## 9. Movement Model

rust_dt uses **teleport-at-arrival** movement:

```
departure_tick                              arrival_tick
     │                                          │
     ▼                                          ▼
─────●──────────────────────────────────────────●────────
  departure_node                           destination_node
  (agent logically here)                  (agent appears here)
```

While `in_transit = true`, the agent's logical position is `departure_node`. At `arrival_tick`, `tick_arrivals()` flips `in_transit = false` and sets the logical position to `destination_node`.

This model is intentional: it avoids edge-based position tracking (which would require updating mid-transit states every tick) while still supporting **visual interpolation** for the visualization layer:

```rust
let (from, to, progress) = engine.visual_position(agent, now);
// progress ∈ [0.0, 1.0] — linearly interpolated along the route
```

Routes are stored in `MobilityStore::routes` (a sparse `HashMap<AgentId, Route>`) only for agents currently in transit, and only for visualization. They are not used in the core simulation logic.

---

## 10. Routing Architecture

The `Router` trait decouples route computation from the rest of the engine:

```rust
pub trait Router: Send + Sync {
    fn route(&self, network: &RoadNetwork, from: NodeId, to: NodeId, mode: TransportMode)
        -> Result<Route, SpatialError>;
}
```

**DijkstraRouter** — the built-in implementation. Runs A*/Dijkstra on the CSR network for each query. Cost is `edge_travel_ms` adjusted by mode speed multiplier.

**PrecomputedRouter** — application-level optimization. Pre-compute all O/D pairs once before the sim starts; queries are O(1) HashMap lookups. Used in the `large` and `xlarge` examples where all origins and destinations are known ahead of time.

**Custom Router** — implement the `Router` trait for any algorithm: contraction hierarchies, time-dependent routing, stochastic travel times, etc. The sim calls `router.route()` in the apply phase (sequential, so no synchronization required).

**`RoadNetwork` internals:**

```
node_out_start: [0, 2, 5, 5, 7, ...]     ← CSR row pointers (node_count + 1 entries)
edge_to:        [1, 3, 0, 2, 4, ...]     ← destination node for each edge
edge_length_m:  [100, 200, 100, ...]     ← geographic distance
edge_travel_ms: [72000, 144000, ...]     ← car travel time
node_pos:       [(lat, lon), ...]        ← geographic coordinates
```

Out-edges for a node `n` are `edge_to[node_out_start[n]..node_out_start[n+1]]` — a single slice with no allocation.

---

## 11. Time Model

All time is represented as an integer `Tick(u64)`:

```
Tick 0    Tick 1    Tick 2   ...   Tick N
  │         │         │              │
  ├─────────┤─────────┤              │
  tick_duration_secs (e.g. 3600)    total_ticks
```

`SimClock` maps ticks to Unix timestamps:

```
unix_time = start_unix_secs + tick * tick_duration_secs
```

Activity plans use `cycle_pos = tick.0 % cycle_ticks` to find the current activity. This is integer modulo — exact, no floating-point drift.

**Why `u64`?** At 1 tick/hour, a u32 overflows after ~490,000 years. At 1 tick/second, u64 overflows after ~585 billion years. Using u64 for `Tick` costs nothing on 64-bit hardware.

---

## 12. RNG Design

**AgentRng** wraps `SmallRng` (a fast non-cryptographic PRNG from the `rand` crate). It exposes:

- `random::<T>()` — sample from the standard distribution for `T`
- `gen_range(range)` — uniform sample in a range
- `gen_bool(p)` — Bernoulli trial
- `shuffle(slice)` — Fisher-Yates in-place
- `choose(slice)` — uniform choice
- `inner()` — direct access to the `SmallRng` for custom distributions

**Seeding formula:**

```
agent_seed = global_seed XOR (agent_id.0 as u64 * 0x9e3779b97f4a7c15)
```

`0x9e3779b97f4a7c15` is the 64-bit golden-ratio-derived constant (Knuth multiplicative hashing). Multiplying by it and XOR-ing with the global seed ensures:

1. Each agent has a unique seed even if `global_seed = 0`
2. Seeds for consecutive agent IDs are maximally spread (low correlation)
3. The global seed can be changed to produce a completely different run

**SimRng** is a separate global RNG for use by setup code (populating agents, generating networks, etc.). It supports `child(offset)` to derive independent sub-RNGs:

```rust
let mut rng = SimRng::new(seed);
let mut network_rng = rng.child(0);
let mut agent_rng   = rng.child(1);
// network_rng and agent_rng have independent, non-overlapping sequences
```

---

## 13. Design Decisions

### Why not event-driven?

Event-driven simulation (e.g. SIMPY-style) is efficient when events are sparse relative to the simulation horizon. At 5 M agents × 365 days × ~3 events/day = 5.5 B events, a heap-based priority queue would:

- Allocate ~5.5 B event structs
- Perform ~5.5 B heap push/pop operations (O(log N) each)
- Touch random memory locations on every heap operation (terrible cache behavior)

A `BTreeMap<Tick, Vec<AgentId>>` with pre-allocated inner Vecs is strictly cheaper for this workload.

### Why not parallel apply phase?

The apply phase writes to `WakeQueue`, `MobilityStore`, and the message buffer. Making these concurrent would require:

- Lock-free or atomic data structures
- Non-deterministic output (dependent on thread scheduling)
- Complex code

Since the intent phase does the heavy work (behavior computation), the sequential apply phase is a small fraction of total runtime. The architecture buys determinism for free.

### Why monomorphic BehaviorModel instead of `dyn BehaviorModel`?

A `dyn Trait` vtable dispatch costs ~1 ns per call. At 5 M agents × 24 ticks/day × 365 days = 43.8 B `replan` calls over a full year simulation, that's ~44 seconds of pure dispatch overhead. Monomorphization eliminates this entirely, and also enables inlining of small behavior methods.

### Why `u32` for node/agent IDs?

`u32` allows up to ~4.3 B agents and ~4.3 B network nodes, more than any foreseeable simulation. Using `u32` instead of `u64` halves the memory for ID arrays — for 5 M agents, that's 20 MB saved just in the ID vec.

### Why `f32` for geographic coordinates?

`f32` gives ~1 m accuracy at any point on Earth (±0.00001°). City-scale simulations don't require sub-meter precision. Using `f32` instead of `f64` halves the memory for position arrays — for 5 M agents, that's 40 MB saved.
