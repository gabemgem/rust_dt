/** Playback controls: play/pause, scrubber, speed, mode toggle. */

import { useStore } from '../store'
import { usePlayback } from '../hooks/usePlayback'
import { useLiveStream } from '../hooks/useLiveStream'

const SPEEDS = [0.25, 0.5, 1, 2, 5, 10]

function formatDate(unixSecs: number): string {
  return new Date(unixSecs * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC'
}

export function Controls() {
  usePlayback()

  const mode = useStore((s) => s.mode)
  const setMode = useStore((s) => s.setMode)
  const playing = useStore((s) => s.playing)
  const setPlaying = useStore((s) => s.setPlaying)
  const speed = useStore((s) => s.speed)
  const setSpeed = useStore((s) => s.setSpeed)
  const currentTickIndex = useStore((s) => s.currentTickIndex)
  const setCurrentTickIndex = useStore((s) => s.setCurrentTickIndex)
  const availableTicks = useStore((s) => s.availableTicks)
  const tickSummaries = useStore((s) => s.tickSummaries)
  const clearSnapshots = useStore((s) => s.clearSnapshots)

  const selectedExample = useStore((s) => s.selectedExample)
  const setSelectedExample = useStore((s) => s.setSelectedExample)
  const availableExamples = useStore((s) => s.availableExamples)

  const { start: startLive } = useLiveStream()

  const currentTick = availableTicks[currentTickIndex] ?? null
  const summaryRow = tickSummaries.find((r) => r.tick === currentTick)

  const tickLabel = currentTick !== null
    ? `Tick ${currentTick} / ${availableTicks[availableTicks.length - 1] ?? '?'}${summaryRow ? '  |  ' + formatDate(summaryRow.unix_time_secs) : ''}`
    : 'No data loaded'

  const handleModeToggle = (m: 'load' | 'live') => {
    if (m === mode) return
    clearSnapshots()
    setMode(m)
  }

  const handleLiveStart = async () => {
    if (mode !== 'live') return
    await startLive()
  }

  return (
    <div className="controls">
      {/* Example selector */}
      <select
        value={selectedExample}
        onChange={(e) => setSelectedExample(e.target.value)}
        className="example-select"
        title="Select example"
      >
        {(availableExamples.length > 0 ? availableExamples : [selectedExample]).map((name) => (
          <option key={name} value={name}>{name}</option>
        ))}
      </select>

      {/* Mode toggle */}
      <div className="mode-toggle">
        <button
          className={mode === 'load' ? 'active' : ''}
          onClick={() => handleModeToggle('load')}
        >
          Load
        </button>
        <button
          className={mode === 'live' ? 'active' : ''}
          onClick={() => handleModeToggle('live')}
        >
          Live
        </button>
      </div>

      {/* Play/pause */}
      {mode === 'load' ? (
        <button className="play-btn" onClick={() => setPlaying(!playing)} disabled={availableTicks.length === 0}>
          {playing ? '⏸' : '▶'}
        </button>
      ) : (
        <button className="run-btn" onClick={handleLiveStart}>
          ▶ Run sim
        </button>
      )}

      {/* Scrubber */}
      <input
        type="range"
        min={0}
        max={Math.max(0, availableTicks.length - 1)}
        value={currentTickIndex}
        onChange={(e) => {
          setPlaying(false)
          setCurrentTickIndex(Number(e.target.value))
        }}
        disabled={availableTicks.length === 0}
        className="scrubber"
      />

      {/* Tick label */}
      <span className="tick-label">{tickLabel}</span>

      {/* Speed */}
      <select
        value={speed}
        onChange={(e) => setSpeed(Number(e.target.value))}
        className="speed-select"
      >
        {SPEEDS.map((s) => (
          <option key={s} value={s}>
            {s}×
          </option>
        ))}
      </select>
    </div>
  )
}
