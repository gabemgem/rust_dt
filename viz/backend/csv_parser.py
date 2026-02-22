"""CSV loading helpers — per-example, grouped by tick for O(1) lookups."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pandas as pd

OUTPUT_BASE = Path(__file__).parent.parent.parent / "output"


def example_dir(example: str) -> Path:
    return OUTPUT_BASE / example


def available_examples() -> list[str]:
    """Return names of examples that have at least tick_summaries.csv."""
    if not OUTPUT_BASE.exists():
        return []
    return sorted(
        d.name
        for d in OUTPUT_BASE.iterdir()
        if d.is_dir() and (d / "tick_summaries.csv").exists()
    )


def load_node_coords(example: str) -> list[dict[str, Any]]:
    path = example_dir(example) / "node_coords.json"
    if not path.exists():
        return []
    with open(path) as f:
        return json.load(f)


def load_network_edges(example: str) -> list[dict[str, Any]]:
    path = example_dir(example) / "network_edges.json"
    if not path.exists():
        return []
    with open(path) as f:
        return json.load(f)


def load_tick_summaries(example: str) -> list[dict[str, Any]]:
    path = example_dir(example) / "tick_summaries.csv"
    if not path.exists():
        return []
    df = pd.read_csv(path)
    return df.to_dict(orient="records")


class SnapshotIndex:
    """Loads agent_snapshots.csv for one example and indexes rows by tick."""

    def __init__(self, example: str) -> None:
        self.example = example
        self._by_tick: dict[int, list[dict[str, Any]]] = {}
        self._loaded = False

    def load(self) -> None:
        path = example_dir(self.example) / "agent_snapshots.csv"
        if not path.exists():
            self._loaded = True
            return
        df = pd.read_csv(path)
        df["tick"] = df["tick"].astype(int)
        df["agent_id"] = df["agent_id"].astype(int)
        df["departure_node"] = df["departure_node"].astype(int)
        df["destination_node"] = df["destination_node"].astype(int)
        df["in_transit"] = df["in_transit"].astype(bool)
        for tick, group in df.groupby("tick"):
            self._by_tick[int(tick)] = group.to_dict(orient="records")
        self._loaded = True

    @property
    def available_ticks(self) -> list[int]:
        return sorted(self._by_tick.keys())

    def get(self, tick: int) -> list[dict[str, Any]]:
        return self._by_tick.get(tick, [])

    def manifest(self) -> dict[str, Any]:
        ticks = self.available_ticks
        tick_duration_secs = 3600
        summaries_path = example_dir(self.example) / "tick_summaries.csv"
        if summaries_path.exists():
            try:
                hdr = pd.read_csv(summaries_path, nrows=2)
                if len(hdr) >= 2:
                    tick_duration_secs = int(
                        hdr["unix_time_secs"].iloc[1] - hdr["unix_time_secs"].iloc[0]
                    )
            except Exception:
                pass
        agent_count = len(self._by_tick[ticks[0]]) if ticks else 0
        return {
            "available_ticks": ticks,
            "agent_count": agent_count,
            "tick_duration_secs": tick_duration_secs,
        }
