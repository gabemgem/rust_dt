"""Asyncio subprocess launcher + SSE generator for live simulation mode."""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path
from typing import AsyncGenerator

REPO_ROOT = Path(__file__).parent.parent.parent
OUTPUT_BASE = REPO_ROOT / "output"

EXT = ".exe" if sys.platform == "win32" else ""

# Map example name → release binary path.
BINARIES: dict[str, Path] = {
    name: REPO_ROOT / "target" / "release" / f"{name}{EXT}"
    for name in ("xsmall", "large", "xlarge")
}

_process: asyncio.subprocess.Process | None = None


async def launch(example: str) -> bool:
    """Start the simulation subprocess.  Returns False if already running."""
    global _process
    if _process is not None and _process.returncode is None:
        return False  # already running

    binary = BINARIES.get(example)
    if binary is None or not binary.exists():
        raise FileNotFoundError(
            f"Binary not found: {binary}. Run `cargo build --release -p {example}` first."
        )

    _process = await asyncio.create_subprocess_exec(
        str(binary),
        cwd=str(REPO_ROOT),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.STDOUT,
    )
    return True


async def sse_stream(example: str) -> AsyncGenerator[str, None]:
    """Yield SSE-formatted events while the sim is running.

    Events:
      - ``event: tick``     — one tick_summaries row (JSON)
      - ``event: snapshot`` — agent snapshot tick available (JSON ``{tick}``)
      - ``event: done``     — sim finished
    """
    import json
    import time

    global _process

    if _process is None:
        yield _sse("error", json.dumps({"message": "No simulation running"}))
        return

    out_dir = OUTPUT_BASE / example
    summaries_path = out_dir / "tick_summaries.csv"
    snapshots_path = out_dir / "agent_snapshots.csv"

    await asyncio.sleep(0.5)

    seen_summary_lines: int = 0
    seen_snapshot_ticks: set[int] = set()
    header_skipped = False
    deadline = time.monotonic() + 300

    while time.monotonic() < deadline:
        if summaries_path.exists():
            try:
                lines = summaries_path.read_text().splitlines()
            except OSError:
                lines = []
            if not header_skipped and len(lines) > 1:
                header_skipped = True
            data_lines = lines[1:] if len(lines) > 1 else []
            for line in data_lines[seen_summary_lines:]:
                parts = line.split(",")
                if len(parts) >= 3:
                    try:
                        yield _sse("tick", json.dumps({
                            "tick": int(parts[0]),
                            "unix_time_secs": int(parts[1]),
                            "woken_agents": int(parts[2]),
                        }))
                    except ValueError:
                        pass
            seen_summary_lines += len(data_lines) - seen_summary_lines

        if snapshots_path.exists():
            try:
                snap_lines = snapshots_path.read_text().splitlines()
            except OSError:
                snap_lines = []
            for line in snap_lines[1:]:
                parts = line.split(",")
                if len(parts) >= 2:
                    try:
                        tick = int(parts[1])
                        if tick not in seen_snapshot_ticks:
                            seen_snapshot_ticks.add(tick)
                            yield _sse("snapshot", json.dumps({"tick": tick}))
                    except ValueError:
                        pass

        if _process.returncode is not None:
            yield _sse("done", json.dumps({"exit_code": _process.returncode}))
            return

        await asyncio.sleep(0.25)

    yield _sse("error", json.dumps({"message": "Stream timeout"}))


def _sse(event: str, data: str) -> str:
    return f"event: {event}\ndata: {data}\n\n"
