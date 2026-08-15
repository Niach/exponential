import { useEffect, useState } from "react"
import { formatDateForMutation } from "@/lib/domain"

// A clock that re-renders the caller on a coarse interval — for time-relative
// UI that must eventually update without any data change (e.g. hiding a stale
// coding-session badge once its liveness window elapses, EXP-153). Default
// 60s: plenty for hour-scale windows, negligible render cost.
export function useNow(intervalMs = 60_000): Date {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), intervalMs)
    return () => clearInterval(id)
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
