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

// Deterministic jitter so co-located agents spread into a small circle (~60 m radius).
// Uses a simple hash of agent_id so positions are stable across ticks.
const JITTER_DEG = 0.0006
function jitteredPosition(base: [number, number], agentId: number): [number, number] {
  const h = (agentId * 2654435761) >>> 0  // Knuth multiplicative hash
  const angle = (h & 0xFFFF) / 0xFFFF * 2 * Math.PI
  const r = JITTER_DEG * (0.4 + 0.6 * ((h >>> 16) & 0xFF) / 255)
  return [base[0] + Math.cos(angle) * r, base[1] + Math.sin(angle) * r]
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

  const currentTick = availableTicks[currentTickIndex] ?? null
  const agents: AgentSnapshot[] =
    currentTick !== null ? (snapshotsByTick.get(currentTick) ?? []) : []

  const agentLayer = new ScatterplotLayer<AgentSnapshot>({
    id: 'agents',
    data: agents,
    getPosition: (d) => {
      const base = nodeMap.get(d.departure_node) ?? ([0, 0] as [number, number])
      return jitteredPosition(base, d.agent_id)
    },
    getFillColor: agentColor,
    getRadius: 14,
    radiusUnits: 'pixels',
    pickable: true,
    updateTriggers: {
      getPosition: currentTick,
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
