import { useEffect, useState } from "react"

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
