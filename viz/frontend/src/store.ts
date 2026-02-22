/** Zustand global state for the visualizer. */

import { create } from 'zustand'
import type { AgentSnapshot, NetworkEdge, NodeCoord, TickSummary, PlaybackMode } from './types'

interface State {
  // Map data
  nodes: NodeCoord[]
  edges: NetworkEdge[]
  nodeMap: Map<number, [number, number]>  // node_id → [lon, lat]

  // Time series
  tickSummaries: TickSummary[]
  availableTicks: number[]

  // Per-tick snapshots
  snapshotsByTick: Map<number, AgentSnapshot[]>

  // Playback state
  currentTickIndex: number
  playing: boolean
  speed: number
  mode: PlaybackMode

  // Example selection
  selectedExample: string
  availableExamples: string[]

  // Actions
  setNodes: (nodes: NodeCoord[]) => void
  setEdges: (edges: NetworkEdge[]) => void
  setTickSummaries: (summaries: TickSummary[]) => void
  setAvailableTicks: (ticks: number[]) => void
  setSnapshot: (tick: number, agents: AgentSnapshot[]) => void
  addSnapshot: (tick: number, agents: AgentSnapshot[]) => void
  setCurrentTickIndex: (idx: number) => void
  setPlaying: (p: boolean) => void
  setSpeed: (s: number) => void
  setMode: (m: PlaybackMode) => void
  clearSnapshots: () => void
  setSelectedExample: (name: string) => void
  setAvailableExamples: (names: string[]) => void
}

export const useStore = create<State>((set) => ({
  nodes: [],
  edges: [],
  nodeMap: new Map(),
  tickSummaries: [],
  availableTicks: [],
  snapshotsByTick: new Map(),
  currentTickIndex: 0,
  playing: false,
  speed: 1,
  mode: 'load',
  selectedExample: 'xsmall',
  availableExamples: [],

  setNodes: (nodes) =>
    set(() => {
      const nodeMap = new Map<number, [number, number]>()
      nodes.forEach((n) => nodeMap.set(n.node_id, [n.lon, n.lat]))
      return { nodes, nodeMap }
    }),

  setEdges: (edges) => set({ edges }),

  setTickSummaries: (tickSummaries) => set({ tickSummaries }),

  setAvailableTicks: (availableTicks) => set({ availableTicks }),

  setSnapshot: (tick, agents) =>
    set((s) => {
      const next = new Map(s.snapshotsByTick)
      next.set(tick, agents)
      return { snapshotsByTick: next }
    }),

  addSnapshot: (tick, agents) =>
    set((s) => {
      const next = new Map(s.snapshotsByTick)
      next.set(tick, agents)
      const ticks = Array.from(next.keys()).sort((a, b) => a - b)
      return { snapshotsByTick: next, availableTicks: ticks }
    }),

  setCurrentTickIndex: (currentTickIndex) => set({ currentTickIndex }),
  setPlaying: (playing) => set({ playing }),
  setSpeed: (speed) => set({ speed }),

  setMode: (mode) => set({ mode, playing: false }),

  clearSnapshots: () =>
    set({ snapshotsByTick: new Map(), availableTicks: [], currentTickIndex: 0, playing: false }),

  setSelectedExample: (selectedExample) =>
    set({ selectedExample, snapshotsByTick: new Map(), availableTicks: [], currentTickIndex: 0, playing: false, nodes: [], edges: [], nodeMap: new Map(), tickSummaries: [] }),

  setAvailableExamples: (availableExamples) => set({ availableExamples }),
}))
