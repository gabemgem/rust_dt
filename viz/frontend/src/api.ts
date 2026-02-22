/** Typed fetch wrappers for the FastAPI backend. */

import type { AgentSnapshot, Manifest, NetworkEdge, NodeCoord, TickSummary } from './types'

const BASE = '/api'

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`)
  if (!res.ok) {
    const msg = await res.text().catch(() => res.statusText)
    throw new Error(`GET ${path} → ${res.status}: ${msg}`)
  }
  return res.json() as Promise<T>
}

function q(example: string) {
  return `?example=${encodeURIComponent(example)}`
}

export const api = {
  examples: () => get<string[]>('/examples'),
  nodes: (example: string) => get<NodeCoord[]>(`/nodes${q(example)}`),
  edges: (example: string) => get<NetworkEdge[]>(`/edges${q(example)}`),
  manifest: (example: string) => get<Manifest>(`/load${q(example)}`),
  tickSummaries: (example: string) => get<TickSummary[]>(`/ticks${q(example)}`),
  snapshot: (tick: number, example: string) => get<AgentSnapshot[]>(`/snapshots/${tick}${q(example)}`),

  run: (example: string) =>
    fetch(`${BASE}/run`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ example }),
    }).then((r) => r.json()),
}
