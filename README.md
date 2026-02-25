# rust_dt

A Rust workspace for simulating millions of agents moving through a city.

**Target:** 5 M agents × 365 days × 1 tick/hour finishing in < 5 min on a 50-core, 128 GB workstation.

## Documentation

| Document | Description |
|----------|-------------|
| [Getting Started](docs/getting-started.md) | Installation, first simulation, running the examples |
| [Application Building Guide](docs/guide.md) | Step-by-step guide covering all framework features |
| [Architecture](docs/architecture.md) | Tick loop, SoA layout, determinism, borrow patterns |
| [API Reference](docs/api-reference.md) | All public types, methods, traits, and feature flags |

## Quick Start

```bash
# 8 agents, synthetic 5-node network, CSV output (~13 ms)
cargo run -p xsmall --release

# 1 M agents, 100×100 grid network, Rayon parallel (~1.1 s)
cargo run -p large --release

# 4 M agents, 10×10 grid network, Rayon parallel (~4.5 s)
cargo run -p xlarge --release
```

## Workspace Layout

```
crates/
  dt-core/      ← IDs, GeoPoint, Tick, SimClock, SimConfig, AgentRng
  dt-agent/     ← SoA agent storage + component system
  dt-spatial/   ← OSM road graph (CSR), Dijkstra routing, R-tree index
  dt-schedule/  ← activity plans, wake queue, CSV schedule loading
  dt-behavior/  ← BehaviorModel trait, Intent enum, SimContext
  dt-mobility/  ← MovementState, MobilityStore, MobilityEngine
  dt-sim/       ← Sim<B,R>, SimBuilder, SimObserver, two-phase tick loop
  dt-output/    ← CSV / Parquet / SQLite writers
docs/
  getting-started.md
  guide.md
  architecture.md
  api-reference.md
examples/
  xsmall/       ← 8 agents commuting on a synthetic 5-node network
  large/        ← 1 M agents × 7 days on a 100×100 grid
  xlarge/       ← 4 M agents × 7 days on a 10×10 grid
viz/            ← FastAPI backend + Vite/React/Deck.gl visualization
```

## Architecture

### Time model

Fixed timestep (default 1 hour) with sparse active-agent processing. A `BTreeMap<Tick, Vec<AgentId>>` wake queue skips idle agents — only agents with something to do are processed each tick.

### Memory layout

Structure of Arrays (SoA) in `dt-agent`. Each field (position, schedule state, transport mode, …) is a separate `Vec` indexed by `AgentId` — critical for Rayon cache efficiency during parallel iteration.

### Two-phase tick loop

1. **Intent phase** — read-only, fully parallel via Rayon (with `--features parallel`). Each woken agent's `BehaviorModel::replan` returns a list of `Intent`s.
2. **Apply phase** — ordered by `AgentId` for determinism. Intents are processed sequentially after all agents have been queried.

### Determinism

Per-agent `AgentRng` seeded as `global_seed XOR (agent_id * GOLDEN_RATIO)`. Rayon results are sorted by `AgentId` before the apply phase. Identical output is guaranteed for any thread count.

### Extensibility

Application behavior is injected via the `BehaviorModel` trait (monomorphized — zero dynamic-dispatch overhead). Custom agent data is added via the `ComponentMap` system. Cargo feature flags gate optional subsystems so unused crates compile to nothing.

## Crate Feature Flags

| Crate | Feature | Effect |
|-------|---------|--------|
| `dt-agent` | `spatial` | `node_id`, `edge_id`, `edge_progress` per-agent |
| `dt-agent` | `schedule` | `next_event_tick`, `current_activity` per-agent |
| `dt-agent` | `mobility` | `transport_mode` per-agent |
| `dt-spatial` | `osm` | Load road networks from OSM PBF files |
| `dt-sim` | `parallel` | Rayon-parallel intent phase |
| `dt-sim` | `fx-hash` | FxHashMap for contact index (20–50% faster) |
| `dt-output` | `sqlite` | SQLite writer via rusqlite |
| `dt-output` | `parquet` | Parquet writer via Arrow + Snappy |

## Performance

Measured on release builds with the `parallel` Rayon feature enabled:

| Example  | Agents | Days | Throughput        |
|----------|-------:|-----:|-------------------|
| `large`  | 1 M    | 7    | ~5.6 M wake-ups/s |
| `xlarge` | 4 M    | 7    | ~5.4 M wake-ups/s |

## Crate Dependency Graph

```
dt-core
  └── dt-agent
        ├── dt-spatial
        ├── dt-schedule
        └── dt-behavior  ──── dt-agent, dt-schedule
              └── dt-mobility ── dt-spatial, dt-behavior
                    └── dt-sim ── all of the above
                          └── dt-output
```

## Testing

```bash
# All crates
cargo test --workspace

# Single crate
cargo test -p dt-core

# With optional features
cargo test -p dt-agent --features spatial,schedule,mobility
cargo test -p dt-sim --features parallel
```

## License

MIT
