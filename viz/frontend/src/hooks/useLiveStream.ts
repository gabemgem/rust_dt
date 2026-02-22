/** SSE consumer for live simulation mode. */

import { useCallback } from 'react'
import { api } from '../api'
import { useStore } from '../store'

export function useLiveStream() {
  const addSnapshot = useStore((s) => s.addSnapshot)
  const setTickSummaries = useStore((s) => s.setTickSummaries)
  const setPlaying = useStore((s) => s.setPlaying)
  const selectedExample = useStore((s) => s.selectedExample)

  const start = useCallback(async () => {
    await api.run(selectedExample)

    const es = new EventSource(`/api/stream?example=${encodeURIComponent(selectedExample)}`)
    const accumulatedSummaries: Array<{ tick: number; unix_time_secs: number; woken_agents: number }> = []

    es.addEventListener('tick', (e) => {
      try {
        accumulatedSummaries.push(JSON.parse(e.data))
        setTickSummaries([...accumulatedSummaries])
      } catch { /* ignore */ }
    })

    es.addEventListener('snapshot', (e) => {
      try {
        const { tick } = JSON.parse(e.data)
        api.snapshot(tick, selectedExample)
          .then((agents) => { addSnapshot(tick, agents); setPlaying(true) })
          .catch(() => { /* snapshot may not be ready yet */ })
      } catch { /* ignore */ }
    })

    es.addEventListener('done', () => { es.close(); setPlaying(false) })
    es.addEventListener('error', () => { es.close(); setPlaying(false) })
  }, [selectedExample, addSnapshot, setTickSummaries, setPlaying])

  return { start }
}
