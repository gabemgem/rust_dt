"""rust_dt visualization backend.

FastAPI app serving static data, CSV snapshots, and live simulation SSE.

Start:
    cd viz/backend
    uvicorn main:app --reload --port 8000
"""

from __future__ import annotations

from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, HTTPException, Query
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

import csv_parser
import sim_runner

# ── Snapshot index registry (lazy-loaded per example) ─────────────────────────

_indexes: dict[str, csv_parser.SnapshotIndex] = {}

DEFAULT_EXAMPLE = "xsmall"


def get_index(example: str) -> csv_parser.SnapshotIndex:
    """Return a loaded SnapshotIndex for *example*, loading on first access."""
    if example not in _indexes:
        idx = csv_parser.SnapshotIndex(example)
        idx.load()
        _indexes[example] = idx
    return _indexes[example]


@asynccontextmanager
async def lifespan(app: FastAPI):  # type: ignore[type-arg]
    # Pre-load whichever examples already have data.
    for name in csv_parser.available_examples():
        get_index(name)
    yield


app = FastAPI(title="rust_dt viz", lifespan=lifespan)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# ── Discovery ──────────────────────────────────────────────────────────────────


@app.get("/api/examples")
async def get_examples() -> list[str]:
    """List examples that have at least tick_summaries.csv in output/{name}/."""
    return csv_parser.available_examples()


# ── Static data endpoints ──────────────────────────────────────────────────────


@app.get("/api/nodes")
async def get_nodes(example: str = Query(DEFAULT_EXAMPLE)) -> list[dict[str, Any]]:
    return csv_parser.load_node_coords(example)


@app.get("/api/edges")
async def get_edges(example: str = Query(DEFAULT_EXAMPLE)) -> list[dict[str, Any]]:
    return csv_parser.load_network_edges(example)


@app.get("/api/load")
async def get_manifest(example: str = Query(DEFAULT_EXAMPLE)) -> dict[str, Any]:
    return get_index(example).manifest()


@app.get("/api/ticks")
async def get_tick_summaries(example: str = Query(DEFAULT_EXAMPLE)) -> list[dict[str, Any]]:
    return csv_parser.load_tick_summaries(example)


@app.get("/api/snapshots/{tick}")
async def get_snapshot(tick: int, example: str = Query(DEFAULT_EXAMPLE)) -> list[dict[str, Any]]:
    idx = get_index(example)
    rows = idx.get(tick)
    if not rows and tick not in idx.available_ticks:
        raise HTTPException(status_code=404, detail=f"No snapshot for tick {tick} in example '{example}'")
    return rows


# ── Live simulation endpoints ──────────────────────────────────────────────────


class RunRequest(BaseModel):
    example: str = DEFAULT_EXAMPLE


@app.post("/api/run")
async def post_run(req: RunRequest) -> dict[str, str]:
    try:
        started = await sim_runner.launch(req.example)
    except FileNotFoundError as exc:
        raise HTTPException(status_code=503, detail=str(exc)) from exc
    return {"status": "started" if started else "already_running"}


@app.get("/api/stream")
async def get_stream(example: str = Query(DEFAULT_EXAMPLE)) -> StreamingResponse:
    return StreamingResponse(
        sim_runner.sse_stream(example),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache", "X-Accel-Buffering": "no"},
    )


# ── Dev entry point ────────────────────────────────────────────────────────────

if __name__ == "__main__":
    import uvicorn
    uvicorn.run("main:app", host="0.0.0.0", port=8000, reload=True)
