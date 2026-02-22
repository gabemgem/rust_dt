/** requestAnimationFrame tick loop for load-mode playback. */

import { useEffect, useRef } from 'react'
import { useStore } from '../store'

export function usePlayback(): void {
  const playing = useStore((s) => s.playing)
  const speed = useStore((s) => s.speed)
  const availableTicks = useStore((s) => s.availableTicks)
  const currentTickIndex = useStore((s) => s.currentTickIndex)
  const setCurrentTickIndex = useStore((s) => s.setCurrentTickIndex)
  const setPlaying = useStore((s) => s.setPlaying)

  const lastTimeRef = useRef<number | null>(null)
  const rafRef = useRef<number | null>(null)

  useEffect(() => {
    if (!playing) {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current)
        rafRef.current = null
      }
      lastTimeRef.current = null
      return
    }

    const msPerTick = 1000 / speed

    const tick = (now: number) => {
      if (lastTimeRef.current === null) {
        lastTimeRef.current = now
      }

      const elapsed = now - lastTimeRef.current
      if (elapsed >= msPerTick) {
        lastTimeRef.current = now - (elapsed % msPerTick)
        const nextIndex = currentTickIndex + Math.floor(elapsed / msPerTick)

        if (nextIndex >= availableTicks.length - 1) {
          setCurrentTickIndex(availableTicks.length - 1)
          setPlaying(false)
          return
        }
        setCurrentTickIndex(nextIndex)
      }

      rafRef.current = requestAnimationFrame(tick)
    }

    rafRef.current = requestAnimationFrame(tick)

    return () => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current)
        rafRef.current = null
      }
    }
  }, [playing, speed, currentTickIndex, availableTicks, setCurrentTickIndex, setPlaying])
}
