/** Root application component. */

import { useEffect, useState } from 'react'
import { api } from '../api'
import { useStore } from '../store'
import { MapPane } from './MapPane'
import { ChartPane } from './ChartPane'
import { Controls } from './Controls'

export function App() {
  const setNodes = useStore((s) => s.setNodes)
  const setEdges = useStore((s) => s.setEdges)
  const setTickSummaries = useStore((s) => s.setTickSummaries)
  const setAvailableTicks = useStore((s) => s.setAvailableTicks)
  const setSnapshot = useStore((s) => s.setSnapshot)
  const setAvailableExamples = useStore((s) => s.setAvailableExamples)
  const selectedExample = useStore((s) => s.selectedExample)

  const [status, setStatus] = useState<string>('Loading…')
  const [error, setError] = useState<string | null>(null)

  // Fetch example list once on mount.
  useEffect(() => {
    api.examples().then(setAvailableExamples).catch(() => setAvailableExamples(['xsmall']))
  }, [setAvailableExamples])

  // Reload all data whenever selectedExample changes.
  useEffect(() => {
    let cancelled = false

    async function load() {
      setError(null)
      try {
        setStatus('Loading network…')
        const [nodes, edges] = await Promise.all([
          api.nodes(selectedExample),
          api.edges(selectedExample),
        ])
        if (cancelled) return
        setNodes(nodes)
        setEdges(edges)

        setStatus('Loading tick summaries…')
        const summaries = await api.tickSummaries(selectedExample)
        if (cancelled) return
        setTickSummaries(summaries)

        setStatus('Loading snapshot manifest…')
        const manifest = await api.manifest(selectedExample)
        if (cancelled) return
        setAvailableTicks(manifest.available_ticks)

        setStatus(`Loading ${manifest.available_ticks.length} snapshots…`)
        await Promise.all(
          manifest.available_ticks.map(async (tick) => {
            const agents = await api.snapshot(tick, selectedExample)
            if (!cancelled) setSnapshot(tick, agents)
          }),
        )

        if (!cancelled) setStatus('')
      } catch (e) {
        if (!cancelled) {
          const msg = e instanceof Error ? e.message : String(e)
          setError(msg)
          setStatus('')
        }
      }
    }

    load()
    return () => { cancelled = true }
  }, [selectedExample, setNodes, setEdges, setTickSummaries, setAvailableTicks, setSnapshot])

  return (
    <div className="app">
      {status && <div className="status-bar">{status}</div>}
      {error && (
        <div className="error-bar">
          {error}
        </div>
      )}
      <div className="main-layout">
        <div className="map-pane">
          <MapPane />
        </div>
        <div className="right-pane">
          <ChartPane />
        </div>
      </div>
      <div className="controls-bar">
        <Controls />
      </div>
    </div>
  )
}
