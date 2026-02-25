/** WebGL map with agent dots (Deck.gl) on OSM tiles (MapLibre). */

import DeckGL from '@deck.gl/react'
import { ScatterplotLayer, LineLayer } from '@deck.gl/layers'
import { Map as MapLibreMap } from 'react-map-gl/maplibre'
import { useStore } from '../store'
import type { AgentSnapshot, NetworkEdge, NodeCoord } from '../types'

const INITIAL_VIEW = {
  longitude: -88.05,
  latitude: 30.69,
  zoom: 12,
  pitch: 0,
  bearing: 0,
}

const BASEMAP = 'https://basemaps.cartocdn.com/gl/dark-matter-gl-style/style.json'

// u32::MAX sentinel written by the Rust sim for "no destination"
const INVALID_NODE = 0xFFFFFFFF

// Deterministic jitter so co-located agents spread into a small circle (~60 m radius).
// Uses a simple hash of agent_id so positions are stable across ticks and during
// GPU-side transitions (the offset is baked into both keyframes, so it stays fixed).
const JITTER_DEG = 0.0006
function jitteredPosition(base: [number, number], agentId: number): [number, number] {
  const h = (agentId * 2654435761) >>> 0  // Knuth multiplicative hash
  const angle = (h & 0xFFFF) / 0xFFFF * 2 * Math.PI
  const r = JITTER_DEG * (0.4 + 0.6 * ((h >>> 16) & 0xFF) / 255)
  return [base[0] + Math.cos(angle) * r, base[1] + Math.sin(angle) * r]
}

// Visual position for an agent.
//
// The simulation stores agents at their *departure* node until arrival (teleport-at-arrival
// model). For smooth frontend animation we instead display in-transit agents at their
// *destination* node. Deck.gl's GPU transition then interpolates from the previous
// keyframe position to this one over the duration of one playback tick, producing a
// smooth glide without any per-frame JS work.
//
// Why this works across the state machine:
//   stationary A  →  in_transit A→B  : prev=A, next=B  → glides A→B ✓
//   in_transit A→B →  stationary B   : prev=B, next=B  → no jump   ✓  (already at B)
//   in_transit A→B →  in_transit A→B : prev=B, next=B  → stays at B ✓  (long trip)
//   stationary A  →  stationary A    : prev=A, next=A  → no movement ✓
function visualPosition(d: AgentSnapshot, nodeMap: Map<number, [number, number]>): [number, number] {
  const targetNode = (d.in_transit && d.destination_node !== INVALID_NODE)
    ? d.destination_node
    : d.departure_node
  const base = nodeMap.get(targetNode) ?? ([0, 0] as [number, number])
  return jitteredPosition(base, d.agent_id)
}

function agentColor(agent: AgentSnapshot): [number, number, number, number] {
  return agent.in_transit ? [255, 140, 0, 220] : [30, 144, 255, 200]
}

export function MapPane() {
  const nodeMap = useStore((s) => s.nodeMap)
  const edges = useStore((s) => s.edges)
  const nodes = useStore((s) => s.nodes)
  const availableTicks = useStore((s) => s.availableTicks)
  const currentTickIndex = useStore((s) => s.currentTickIndex)
  const snapshotsByTick = useStore((s) => s.snapshotsByTick)
  const speed = useStore((s) => s.speed)
  const playing = useStore((s) => s.playing)

  const currentTick = availableTicks[currentTickIndex] ?? null
  const agents: AgentSnapshot[] =
    currentTick !== null ? (snapshotsByTick.get(currentTick) ?? []) : []

  // Transition duration matches the playback rate so each tick's position change
  // completes exactly as the next tick begins — giving the appearance of continuous
  // movement. When paused or scrubbing, duration=0 gives instant position snapping.
  const msPerTick = 1000 / speed
  const transitionDuration = playing ? msPerTick : 0

  const agentLayer = new ScatterplotLayer<AgentSnapshot>({
    id: 'agents',
    data: agents,
    getPosition: (d) => visualPosition(d, nodeMap),
    getFillColor: agentColor,
    getRadius: 14,
    radiusUnits: 'pixels',
    pickable: true,
    transitions: {
      // Deck.gl interpolates the GPU position attribute from the previous keyframe
      // to the new one. This is entirely GPU-side — no JS runs per frame.
      getPosition: { duration: transitionDuration, easing: (t: number) => t },
    },
    updateTriggers: {
      getPosition: [currentTick, transitionDuration],
      getFillColor: currentTick,
    },
  })

  const edgeLayer = new LineLayer<NetworkEdge>({
    id: 'edges',
    data: edges,
    getSourcePosition: (d) => nodeMap.get(d.from_node) ?? [0, 0],
    getTargetPosition: (d) => nodeMap.get(d.to_node) ?? [0, 0],
    getColor: [80, 80, 80, 180],
    getWidth: 2,
    widthUnits: 'pixels',
  })

  const nodeLayer = new ScatterplotLayer<NodeCoord>({
    id: 'nodes',
    data: nodes,
    getPosition: (d) => [d.lon, d.lat],
    getFillColor: [200, 200, 200, 150],
    getRadius: 6,
    radiusUnits: 'pixels',
  })

  return (
    // position:absolute + inset:0 relative to .map-pane (position:relative)
    // gives DeckGL real pixel dimensions to work with
    <DeckGL
      style={{ position: 'absolute', top: '0', left: '0', right: '0', bottom: '0' }}
      initialViewState={INITIAL_VIEW}
      controller
      layers={[edgeLayer, nodeLayer, agentLayer]}
      getTooltip={(info) => {
        const agent = info.object as AgentSnapshot | undefined
        if (!agent) return null
        return {
          html: `<b>Agent ${agent.agent_id}</b><br/>${agent.in_transit ? 'In transit → node ' + agent.destination_node : 'Stationary at node ' + agent.departure_node}`,
          style: {
            background: '#1a1a2e',
            color: '#eee',
            padding: '6px 10px',
            borderRadius: '4px',
          },
        }
      }}
    >
      <MapLibreMap mapStyle={BASEMAP} />
    </DeckGL>
  )
}
