# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Check a specific crate
cargo check -p dt-core

# Lint (warnings are errors)
cargo clippy -p dt-core -- -D warnings

# Run all tests
cargo test

# Run a single crate's tests
cargo test -p dt-core

# Run a single test by name
cargo test -p dt-core rng::deterministic_same_seed

# Run tests with optional features enabled (dt-agent example)
cargo test -p dt-agent --features spatial,schedule,mobility

# Build release
cargo build --release

# Format
cargo fmt
```

## Project: rust_dt — Digital Twin Agent-Based Modelling Framework

A Rust workspace for simulating millions of agents moving through a city.
Target: 5 M agents × 365 days × 1 tick/hour finishing in < 5 min on a 50-core, 128 GB workstation.

### Workspace layout

```
crates/
  dt-core/      ← foundational types (IDs, GeoPoint, Tick, SimClock, AgentRng)
  dt-agent/     ← SoA agent storage + component system
  dt-spatial/   ← OSM road graph (CSR), Dijkstra routing
  dt-schedule/  ← activity plans, wake queue, CSV schedule loading
  dt-behavior/  ← BehaviorModel trait, Intent enum, SimContext, NoopBehavior
  dt-mobility/  ← MovementState, MobilityStore, MobilityEngine<R>
  dt-sim/       ← Sim<B,R>, SimBuilder, SimObserver, two-phase tick loop
  dt-output/    ← CSV/Parquet/SQLite writers                   [planned]
  dt-checkpoint/ ← checkpoint/restart via serde + bincode      [planned]
  dt-viz/       ← visualization file writer                    [planned]
  dt-sim/       ← tick loop orchestrator, Rayon parallelism    [planned]
  dt-macros/    ← proc macros for ergonomic component defs     [planned]
examples/
  mobile_al/    ← MVP application (Mobile, AL ~400 K agents)   [planned]
```

### Key architectural decisions

**Time model**: Fixed timestep (default 1 hour, configurable) with sparse active-agent processing. A `BTreeMap<Tick, Vec<AgentId>>` wake queue skips idle agents in O(1). Event-driven was rejected because 5 B+ heap operations exceed fixed-tick cost at this agent count.

**Memory layout**: Structure of Arrays (SoA) in `dt-agent`. Each field (position, schedule state, mode, …) is a separate `Vec` indexed by `AgentId`. This is critical for Rayon cache efficiency during parallel iteration.

**Tick loop (two-phase)**:
1. Compute intents (read-only, fully parallel via Rayon)
2. Apply intents (write phase, ordered by `AgentId` for determinism)

**Determinism**: Per-agent `AgentRng` seeded as `global_seed XOR (agent_id * GOLDEN_RATIO_CONST)`. Rayon results are collected into `AgentId`-sorted `Vec`s before applying.

**Extensibility**: Application behavior injected via the `BehaviorModel` trait (monomorphized, zero dynamic-dispatch overhead). Cargo feature flags gate optional subsystems so unused crates compile to nothing.

### dt-agent module summary

| Module        | Key types / structs                                              |
|---------------|------------------------------------------------------------------|
| `component`   | `ComponentVec` (trait), `TypedComponentVec<T>`, `ComponentMap`  |
| `store`       | `AgentStore` (SoA arrays), `AgentRngs` (separate RNG vec)       |
| `builder`     | `AgentStoreBuilder` (fluent, returns `(AgentStore, AgentRngs)`) |

**Features**: `spatial` (node/edge arrays), `schedule` (tick/activity arrays), `mobility` (transport mode array). All off by default.

**Borrow-split design**: `AgentRngs` is a separate struct so `dt-sim` can hold `&mut AgentRngs` + `&AgentStore` simultaneously. Rayon's `par_iter_mut()` on `AgentRngs::inner` gives each worker exclusive `&mut AgentRng` while the store is shared immutably.

**Component system**: `ComponentMap` stores `Box<dyn ComponentVec>` keyed by `TypeId`. Applications register their own data types (`register_component::<T>()` on the builder). Access is `store.component::<T>()` → `&[T]`, indexed by `agent.index()`.

### dt-core module summary

| Module        | Key types                                    |
|---------------|----------------------------------------------|
| `ids`         | `AgentId(u32)`, `NodeId(u32)`, `EdgeId(u32)`, `ActivityId(u16)` |
| `geo`         | `GeoPoint { lat: f32, lon: f32 }`, haversine distance |
| `time`        | `Tick(u64)`, `SimClock`, `SimConfig`         |
| `rng`         | `AgentRng` (per-agent), `SimRng` (global)    |
| `transport`   | `TransportMode` enum                         |
| `error`       | `DtError`, `DtResult<T>`                     |

### dt-sim module summary

`Sim<B: BehaviorModel, R: Router>` — all fields `pub` for inspection.

**Construction**: `SimBuilder::new(config, agents, rngs, behavior, router)` with optional `.plans()`, `.network()`, `.initial_positions()`.

**Running**: `sim.run(&mut observer)` — processes ticks 0..total_ticks.  `sim.run_ticks(n, &mut observer)` — runs exactly N ticks from current position (useful for tests).

**Tick loop**:
1. `mobility.tick_arrivals(now)` — mark arrived agents stationary, re-insert into wake queue via `plans[agent].next_wake_tick(now)`.
2. `wake_queue.drain_tick(now)` — get agents woken this tick.
3. Intent phase: sequential, or parallel with `--features parallel` (Rayon via `AgentRngs::get_many_mut`).
4. Apply phase: `WakeAt(t)` → push to queue (guards `t > now`); `TravelTo{dest,mode}` → `mobility.begin_travel`, push `arrival_tick`; `SendMessage` → TODO.

**Key invariant**: Wake queue `drain_tick` always returns `AgentId`s in ascending order (BTreeMap). This is what makes the apply phase deterministic regardless of whether the intent phase ran in parallel.

**Parallel feature**: `cargo test -p dt-sim --features parallel`. Uses `AgentRngs::get_many_mut` (unsafe, with disjoint-index safety invariant) to zip woken agents with their RNG refs for `rayon::par_iter()`.

### dt-behavior and dt-mobility module summaries

**dt-behavior** (depends on dt-core, dt-agent, dt-schedule):

| Module    | Key types                                                      |
|-----------|----------------------------------------------------------------|
| `intent`  | `Intent` enum: `TravelTo`, `WakeAt`, `SendMessage`             |
| `context` | `SimContext<'a>` — read-only tick snapshot (tick, agents, plans)|
| `contact` | `ContactEvent`, `ContactKind` (Node / Edge)                    |
| `model`   | `BehaviorModel` trait: `replan` (required), `on_contacts`/`on_message` (defaulted) |
| `noop`    | `NoopBehavior` — always returns empty intents                  |

`BehaviorModel` is `Send + Sync + 'static`; the intent phase can run in parallel via Rayon.  All state accessed read-only through `&SimContext`; writes happen in the apply phase.

**dt-mobility** (depends on dt-core, dt-agent, dt-spatial, dt-behavior):

| Module   | Key types                                                         |
|----------|-------------------------------------------------------------------|
| `state`  | `MovementState` — `in_transit`, departure/destination nodes, `departure_tick`/`arrival_tick`, `progress(now) -> f32` |
| `store`  | `MobilityStore` — `Vec<MovementState>` + `HashMap<AgentId, Route>` (sparse) |
| `engine` | `MobilityEngine<R: Router>` — `place`, `begin_travel`, `tick_arrivals`, `visual_position` |

**Movement model**: "teleport at arrival" — agents stay logically at `departure_node` until `arrival_tick`, then appear at `destination_node`.  Routes are stored in `MobilityStore::routes` for visualization interpolation only.

### Rust edition 2024 gotcha

`gen` is a reserved keyword in edition 2024. Use `r#gen()` to call `rand::Rng::gen` on inner `SmallRng` fields, and name wrapper methods `random()` instead of `gen()`.
