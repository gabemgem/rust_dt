# rust_dt Visualization Engine

Interactive WebGL visualization of the digital-twin simulation.

Stack: TypeScript + Vite + React + Deck.gl v9 (agent dots) + MapLibre GL JS v4 (OSM tiles) + Chart.js v4 (charts)
Backend: Python FastAPI + uvicorn (serves CSVs, runs sim subprocess, SSE stream)

---

## Quick start

### 1 — Build & run the simulation

```bash
# From repo root
cargo build --release -p mobile-al

# Export node/edge JSON for the map
cargo run -p mobile-al --bin export_nodes
# → output/node_coords.json  (5 nodes)
# → output/network_edges.json (12 edges)

# Run the simulation to generate CSV output
cargo run -p mobile-al --release
# → output/agent_snapshots.csv  (56 rows)
# → output/tick_summaries.csv   (168 rows)
```

### 2 — Start the Python backend

```bash
cd viz/backend
pip install -r requirements.txt
uvicorn main:app --reload --port 8000
```

The backend reads from `../../output/` relative to `viz/backend/`.

### 3 — Start the frontend dev server

```bash
cd viz/frontend
npm install
npm run dev
# Open http://localhost:5173
```

The Vite dev server proxies `/api/*` → `http://localhost:8000`.

---

## API endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/nodes` | Node coordinates (`node_coords.json`) |
| GET | `/api/edges` | Network edges (`network_edges.json`) |
| GET | `/api/load` | Manifest: available ticks, agent count, tick duration |
| GET | `/api/ticks` | Full `tick_summaries.csv` as JSON |
| GET | `/api/snapshots/{tick}` | Agent rows for one snapshot tick |
| POST | `/api/run` | Launch `target/release/mobile_al` subprocess |
| GET | `/api/stream` | SSE: `tick`, `snapshot`, `done` events |

---

## UI features

- **Load mode** — loads pre-generated CSVs; use scrubber or play button to animate
- **Live mode** — launches the sim via `/api/run`, renders dots tick-by-tick via SSE
- **Map** — dark CartoDB basemap, road-network edges, colored agent dots
  (blue = stationary, orange = in-transit)
- **Charts** — woken agents/tick, in-transit count, stationary/in-transit doughnut, cumulative wakeups
  Vertical annotation line tracks the current tick

---

## Directory structure

```
viz/
  backend/
    main.py          # FastAPI routes
    csv_parser.py    # pandas CSV loader + snapshot index
    sim_runner.py    # asyncio subprocess + SSE generator
    requirements.txt
  frontend/
    package.json
    vite.config.ts   # /api proxy
    src/
      types.ts       # TypeScript domain types
      store.ts       # Zustand state
      api.ts         # typed fetch wrappers
      hooks/
        usePlayback.ts   # rAF tick loop
        useLiveStream.ts # SSE EventSource consumer
      components/
        App.tsx
        MapPane.tsx
        ChartPane.tsx
        Controls.tsx
      styles.css
```
