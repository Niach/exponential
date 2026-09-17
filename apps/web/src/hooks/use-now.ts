import { useEffect, useState } from "react"
import { formatDateForMutation } from "@/lib/domain"

// A clock that re-renders the caller on a coarse interval — for time-relative
// UI that must eventually update without any data change (e.g. hiding a stale
// coding-session badge once its liveness window elapses, EXP-153). Default
// 60s: plenty for hour-scale windows, negligible render cost.
//
// EXP-875: it STANDS STILL while the tab is hidden, and jumps to the real time
// the moment it comes back. Nobody is reading a background tab's captions, and
// the loops that ride this beat (the Devices page's usage refresh) must not
// keep talking to a machine on behalf of a window nobody is looking at.
export function useNow(intervalMs = 60_000): Date {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    let id: ReturnType<typeof setInterval> | null = null
    const stop = () => {
      if (id !== null) clearInterval(id)
      id = null
    }
    const start = () => {
      if (id === null) id = setInterval(() => setNow(new Date()), intervalMs)
    }
    const visible = () =>
      typeof document === `undefined` || document.visibilityState === `visible`
    const onVisibility = () => {
      if (!visible()) {
        stop()
        return
      }
      // Back in front: the caller's captions are however stale the tab was.
      setNow(new Date())
      start()
    }
    if (visible()) start()
    if (typeof document !== `undefined`) {
      document.addEventListener(`visibilitychange`, onVisibility)
    }
    return () => {
      stop()
      if (typeof document !== `undefined`) {
        document.removeEventListener(`visibilitychange`, onVisibility)
      }
    }
  }, [intervalMs])
  return now
}

// The local calendar date as a mutation-format string, crossing midnight
// without a remount. Polls on a coarse interval but only re-renders the
// caller when the date actually flips, so long-lived lists don't churn
// every minute.
export function useToday(): string {
  const [today, setToday] = useState(() => formatDateForMutation(new Date())!)
  useEffect(() => {
    const id = setInterval(() => {
      const next = formatDateForMutation(new Date())!
      setToday((prev) => (prev === next ? prev : next))
    }, 60_000)
    return () => clearInterval(id)
  }, [])
  return today
}
