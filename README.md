# rust_dt

A Rust workspace for simulating millions of agents moving through a city.

**Target**: 5 M agents × 365 days × 1 tick/hour finishing in < 5 min on a 50-core, 128 GB workstation.

## Workspace layout

```
crates/
  dt-core/      ← foundational types (IDs, GeoPoint, Tick, SimClock, AgentRng)
  dt-agent/     ← SoA agent storage + component system
  dt-spatial/   ← OSM road graph (CSR), Dijkstra routing
  dt-schedule/  ← activity plans, wake queue, CSV schedule loading
  dt-behavior/  ← BehaviorModel trait, Intent enum, SimContext, NoopBehavior
  dt-mobility/  ← MovementState, MobilityStore, MobilityEngine
  dt-sim/       ← Sim<B,R>, SimBuilder, SimObserver, two-phase tick loop
  dt-output/    ← CSV / Parquet / SQLite writers
examples/
  mobile_al/    ← MVP: 8 agents commuting on a synthetic Mobile, AL network
  large/        ← benchmark: 1 M agents × 7 days
  xlarge/       ← benchmark: 4 M agents × 7 days
```

## Architecture

### Time model

Fixed timestep (default 1 hour) with sparse active-agent processing. A `BTreeMap<Tick, Vec<AgentId>>` wake queue skips idle agents. Event-driven simulation was rejected because 5 B+ heap operations exceed fixed-tick cost at this agent count.

### Memory layout

Structure of Arrays (SoA) in `dt-agent`. Each field (position, schedule state, transport mode, …) is a separate `Vec` indexed by `AgentId` — critical for Rayon cache efficiency during parallel iteration.

### Two-phase tick loop

1. **Intent phase** — read-only, fully parallel via Rayon. Each agent's `BehaviorModel::replan` returns a list of `Intent`s.
2. **Apply phase** — ordered by `AgentId` for determinism. Intents are collected into a sorted `Vec` before applying.

### Determinism

Per-agent `AgentRng` seeded as `global_seed XOR (agent_id * GOLDEN_RATIO_CONST)`. Rayon results are sorted by `AgentId` before the apply phase.

### Extensibility

Application behavior is injected via the `BehaviorModel` trait (monomorphized — zero dynamic-dispatch overhead). Cargo feature flags gate optional subsystems so unused crates compile to nothing.

## Quick start

```bash
# Run the MVP example (8 agents, synthetic Mobile AL network, CSV output)
cargo run -p mobile_al --release

# 1 M agent scheduling benchmark (parallel Rayon)
cargo run -p large --release

# 4 M agent benchmark
cargo run -p xlarge --release
```

## Testing

```bash
# All tests
cargo test --workspace

# Single crate
cargo test -p dt-core

# With optional features
cargo test -p dt-agent --features spatial,schedule,mobility

# Parallel sim
cargo test -p dt-sim --features parallel
```

## Performance

Measured on a release build with the `parallel` Rayon feature enabled:

| Example  | Agents | Days | Throughput        |
|----------|-------:|-----:|-------------------|
| `large`  |  1 M   |  7   | ~5.6 M wake-ups/s |
| `xlarge` |  4 M   |  7   | ~5.4 M wake-ups/s |

## Output formats

`dt-output` supports three backends (all feature-gated):

| Feature   | Format  | Notes                              |
|-----------|---------|------------------------------------|
| *(default)* | CSV   | `agent_snapshots.csv`, `tick_summaries.csv` |
| `sqlite`  | SQLite  | via `rusqlite` (bundled)           |
| `parquet` | Parquet | via Arrow + Snappy compression     |

## Crate dependency graph

```
dt-core
  └── dt-agent
        ├── dt-spatial
        ├── dt-schedule
        ├── dt-behavior  ──── dt-agent, dt-schedule
        │     └── dt-mobility ── dt-spatial, dt-behavior
        │           └── dt-sim ── all of the above
        │                 └── dt-output
        └── (examples)
```

## License

MIT
