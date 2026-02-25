# Getting Started with rust_dt

rust_dt is a Rust framework for agent-based digital twin simulations — specifically designed to simulate millions of agents moving through a city. This guide gets you from zero to a running simulation in minutes.

## Prerequisites

- **Rust** 1.85 or later (edition 2024 is required)
- **Cargo** (included with Rust)

Check your version:

```bash
rustc --version   # should be >= 1.85
```

## Running the Bundled Examples

The repository includes three ready-to-run examples at increasing scale. Clone the repo and run any of them:

```bash
git clone https://github.com/gabemgem/rust_dt.git
cd rust_dt

# 8 agents, synthetic 5-node network, daily commute, CSV output (~13 ms)
cargo run -p xsmall --release

# 1 M agents, 100×100 grid network, Rayon parallel (~1.1 s)
cargo run -p large --release

# 4 M agents, 10×10 grid network, Rayon parallel (~4.5 s)
cargo run -p xlarge --release
```

Output CSV files are written to `output/<example>/`.

## Adding rust_dt as a Dependency

rust_dt is a workspace of small, independently usable crates. In your application's `Cargo.toml`, depend only on what you need:

```toml
[dependencies]
# Required for every application
dt-core    = { path = "/path/to/rust_dt/crates/dt-core" }
dt-agent   = { path = "/path/to/rust_dt/crates/dt-agent" }
dt-sim     = { path = "/path/to/rust_dt/crates/dt-sim" }

# Add these as your application grows
dt-spatial  = { path = "/path/to/rust_dt/crates/dt-spatial" }
dt-schedule = { path = "/path/to/rust_dt/crates/dt-schedule" }
dt-behavior = { path = "/path/to/rust_dt/crates/dt-behavior" }
dt-mobility = { path = "/path/to/rust_dt/crates/dt-mobility" }
dt-output   = { path = "/path/to/rust_dt/crates/dt-output" }

# Optional features
[features]
parallel = ["dt-sim/parallel"]   # Rayon-parallel intent phase
```

For `dt-agent`, enable only the subsystems you use:

```toml
dt-agent = { path = "...", features = ["spatial", "schedule", "mobility"] }
```

| Feature    | Adds to AgentStore                                         |
|------------|------------------------------------------------------------|
| `spatial`  | `node_id`, `edge_id`, `edge_progress` fields per agent     |
| `schedule` | `next_event_tick`, `current_activity` fields per agent     |
| `mobility` | `transport_mode` field per agent                           |

## Your First Simulation (30 lines)

A minimal simulation with no network or schedule — just agents waking up every tick and returning a no-op intent:

```rust
use dt_core::SimConfig;
use dt_agent::AgentStoreBuilder;
use dt_behavior::NoopBehavior;
use dt_sim::{SimBuilder, NoopObserver};
use dt_spatial::DijkstraRouter;

fn main() {
    let config = SimConfig {
        start_unix_secs:       0,
        tick_duration_secs:    3_600,  // 1 tick = 1 hour
        total_ticks:           24,     // run for 1 day
        seed:                  42,
        num_threads:           None,   // use all CPU cores
        output_interval_ticks: 0,      // no snapshots
    };

    let (store, rngs) = AgentStoreBuilder::new(100, config.seed).build();

    let mut sim = SimBuilder::new(config, store, rngs, NoopBehavior, DijkstraRouter)
        .build()
        .expect("sim build failed");

    sim.run(&mut NoopObserver).expect("sim failed");

    println!("Done. {} agents simulated.", sim.agents.count);
}
```

This compiles, runs, and completes instantly. From here, see the **[Application Building Guide](guide.md)** to add real behavior, road networks, schedules, and output.

## Next Steps

| Goal | Guide section |
|------|---------------|
| Add custom per-agent data | [Custom Agent Components](guide.md#3-custom-agent-components) |
| Build or load a road network | [Road Networks](guide.md#4-building-a-road-network) |
| Define activity schedules | [Activity Plans](guide.md#5-activity-plans) |
| Write agent decision logic | [Behavior Models](guide.md#6-the-behavior-model) |
| Handle agent contacts | [Contact Events](guide.md#8-contact-events) |
| Write output files | [Output Writers](guide.md#10-output-writers) |
| Scale to millions of agents | [Performance](guide.md#12-performance-guide) |
| Load a real city from OSM | [OSM Networks](guide.md#13-loading-real-osm-networks) |

## Running Tests

```bash
# All crates
cargo test --workspace

# One crate
cargo test -p dt-core

# With features
cargo test -p dt-agent --features spatial,schedule,mobility
cargo test -p dt-sim --features parallel
```
