import { useCallback, useEffect, useState, useSyncExternalStore } from "react"
import {
  getConnectivitySnapshot,
  getServerConnectivitySnapshot,
  probeServerHealth,
  subscribeConnectivity,
  type SyncHealth,
} from "@/lib/connectivity"

/** While offline, re-probe this often. It doubles as the thing that keeps
 *  `lastErrorAt` fresh: without it a persistent outage would fall past the
 *  300s staleness window and the banner would clear itself while still
 *  offline. */
const OFFLINE_PROBE_INTERVAL_MS = 15_000

/**
 * EXP-533: the offline banner's state. Shape polls and tRPC calls feed the
 * model on their own; this hook adds the two things a passive reader cannot
 * do, an explicit Retry and a probe on every wake-up.
 */
export function useConnectivity(): {
  health: SyncHealth
  retry: () => void
  retrying: boolean
} {
  const health = useSyncExternalStore(
    subscribeConnectivity,
    getConnectivitySnapshot,
    getServerConnectivitySnapshot
  )
  const [retrying, setRetrying] = useState(false)

  const retry = useCallback(() => {
    setRetrying(true)
    void probeServerHealth().finally(() => setRetrying(false))
  }, [])

  const offline = health === `offline`

  useEffect(() => {
    if (!offline) return
    const id = setInterval(() => {
      void probeServerHealth()
    }, OFFLINE_PROBE_INTERVAL_MS)
    return () => clearInterval(id)
  }, [offline])

  // The wake-ups (mirrors `steer-session-store.ts`): a tab that was frozen or
  // a machine that slept comes back with sockets the OS quietly killed, and
  // nothing notices until the next poll times out. Probe unconditionally —
  // it also clears a stale banner the instant the network returns.
  useEffect(() => {
    if (typeof window === `undefined`) return
    const probe = () => {
      void probeServerHealth()
    }
    const onVisible = () => {
      if (document.visibilityState === `visible`) probe()
    }
    window.addEventListener(`online`, probe)
    document.addEventListener(`visibilitychange`, onVisible)
    return () => {
      window.removeEventListener(`online`, probe)
      document.removeEventListener(`visibilitychange`, onVisible)
    }
  }, [])

  return { health, retry, retrying }
}
