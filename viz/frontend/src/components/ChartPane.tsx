/** Chart.js side-pane with four charts. */

import { useEffect, useRef } from 'react'
import {
  Chart,
  CategoryScale,
  LinearScale,
  PointElement,
  LineController,
  LineElement,
  DoughnutController,
  ArcElement,
  Filler,
  Tooltip,
  Legend,
} from 'chart.js'
import annotationPlugin from 'chartjs-plugin-annotation'
import { useStore } from '../store'
import type { AgentSnapshot } from '../types'

Chart.register(
  CategoryScale,
  LinearScale,
  PointElement,
  LineController,
  LineElement,
  DoughnutController,
  ArcElement,
  Filler,
  Tooltip,
  Legend,
  annotationPlugin,
)

// ── shared chart defaults ──────────────────────────────────────────────────

const DARK_SCALES = {
  x: { ticks: { color: '#aaa' }, grid: { color: '#333' } },
  y: { ticks: { color: '#aaa' }, grid: { color: '#333' } },
}
const DARK_LEGEND = { labels: { color: '#ccc' } }

// ── line chart hook ────────────────────────────────────────────────────────

interface LineData {
  labels: string[]
  values: number[]
  label: string
  color: string
  currentLabel: string
}

function useLineChart(canvasRef: React.RefObject<HTMLCanvasElement | null>, d: LineData) {
  const chartRef = useRef<Chart | null>(null)

  // Create chart once on mount.
  useEffect(() => {
    if (!canvasRef.current) return
    chartRef.current = new Chart(canvasRef.current, {
      type: 'line',
      data: {
        labels: [],
        datasets: [{ label: d.label, data: [], borderColor: d.color, backgroundColor: d.color.replace(')', ',0.15)').replace('rgb', 'rgba'), fill: true, pointRadius: 0, tension: 0.3 }],
      },
      options: {
        animation: false,
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: DARK_LEGEND,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          annotation: { annotations: {} } as any,
        },
        scales: DARK_SCALES,
      },
    })
    return () => { chartRef.current?.destroy(); chartRef.current = null }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // Update data on every change.
  useEffect(() => {
    const chart = chartRef.current
    if (!chart) return
    chart.data.labels = d.labels
    chart.data.datasets[0].data = d.values
    chart.data.datasets[0].label = d.label
    // Update annotation line.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ann = (chart.options.plugins as any).annotation
    if (ann) {
      ann.annotations = d.currentLabel
        ? {
            currentTick: {
              type: 'line',
              xMin: d.currentLabel,
              xMax: d.currentLabel,
              borderColor: 'rgba(255,200,0,0.7)',
              borderWidth: 2,
            },
          }
        : {}
    }
    chart.update('none')
  }, [d.labels, d.values, d.currentLabel, d.label])
}

// ── doughnut chart hook ────────────────────────────────────────────────────

function useDoughnutChart(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  stationary: number,
  inTransit: number,
) {
  const chartRef = useRef<Chart | null>(null)

  useEffect(() => {
    if (!canvasRef.current) return
    chartRef.current = new Chart(canvasRef.current, {
      type: 'doughnut',
      data: {
        labels: ['Stationary', 'In-Transit'],
        datasets: [{ data: [0, 0], backgroundColor: ['rgba(30,144,255,0.8)', 'rgba(255,140,0,0.8)'], borderColor: '#222' }],
      },
      options: {
        animation: false,
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: DARK_LEGEND },
      },
    })
    return () => { chartRef.current?.destroy(); chartRef.current = null }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const chart = chartRef.current
    if (!chart) return
    chart.data.datasets[0].data = [stationary, inTransit]
    chart.update('none')
  }, [stationary, inTransit])
}

// ── component ──────────────────────────────────────────────────────────────

export function ChartPane() {
  const tickSummaries = useStore((s) => s.tickSummaries)
  const availableTicks = useStore((s) => s.availableTicks)
  const currentTickIndex = useStore((s) => s.currentTickIndex)
  const snapshotsByTick = useStore((s) => s.snapshotsByTick)

  const currentTick = availableTicks[currentTickIndex] ?? null
  const currentAgents: AgentSnapshot[] =
    currentTick !== null ? (snapshotsByTick.get(currentTick) ?? []) : []

  const snapTicks = Array.from(snapshotsByTick.keys()).sort((a, b) => a - b)
  const inTransitCounts = snapTicks.map((t) => (snapshotsByTick.get(t) ?? []).filter((a) => a.in_transit).length)

  const summaryLabels = tickSummaries.map((r) => String(r.tick))
  const wokenData = tickSummaries.map((r) => r.woken_agents)

  let cumulative = 0
  const cumulativeData = tickSummaries.map((r) => { cumulative += r.woken_agents; return cumulative })

  const stationaryCount = currentAgents.filter((a) => !a.in_transit).length
  const inTransitCount = currentAgents.filter((a) => a.in_transit).length
  const currentLabel = currentTick !== null ? String(currentTick) : ''

  const wokeRef = useRef<HTMLCanvasElement>(null)
  const transitRef = useRef<HTMLCanvasElement>(null)
  const doughnutRef = useRef<HTMLCanvasElement>(null)
  const cumulRef = useRef<HTMLCanvasElement>(null)

  useLineChart(wokeRef, { labels: summaryLabels, values: wokenData, label: 'Woken Agents', color: 'rgb(79,195,247)', currentLabel })
  useLineChart(transitRef, { labels: snapTicks.map(String), values: inTransitCounts, label: 'In-Transit', color: 'rgb(255,183,77)', currentLabel })
  useLineChart(cumulRef, { labels: summaryLabels, values: cumulativeData, label: 'Cumulative Wakeups', color: 'rgb(165,214,167)', currentLabel })
  useDoughnutChart(doughnutRef, stationaryCount, inTransitCount)

  return (
    <div className="chart-pane">
      <div className="chart-box">
        <p className="chart-title">Woken Agents / Tick</p>
        <canvas ref={wokeRef} />
      </div>
      <div className="chart-box">
        <p className="chart-title">In-Transit Count (snapshots)</p>
        <canvas ref={transitRef} />
      </div>
      <div className="chart-box doughnut-box">
        <p className="chart-title">Current Status</p>
        <canvas ref={doughnutRef} />
      </div>
      <div className="chart-box">
        <p className="chart-title">Cumulative Wakeups</p>
        <canvas ref={cumulRef} />
      </div>
    </div>
  )
}
