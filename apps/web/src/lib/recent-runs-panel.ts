import { useCallback, useEffect, useSyncExternalStore } from "react"
import { useRouterState } from "@tanstack/react-router"

// EXP-923: the Agent page's RECENT runs panel — open state, and nothing else.
//
// The page is the composer alone; its history sits behind one ghost button
// that slides the sidebar's panel slot in beside the compact rail. The flag is
// deliberately NOT in the URL (a disclosure, not a destination, and a shared
// link must not reopen it) and deliberately NOT React state (the button lives
// on the Agent page, the panel in the sidebar — two subtrees with the team
// layout between them), so it is a three-line external store on the
// `use-work-tabs.ts` rails.
//
// EXP-1246: Recent is a LIST-DETAIL host like the Inbox — a run opened from it
// (`?from=agent:recent`) keeps the list beside it, and Back lands on the Agent
// page with the panel still open. So the flag no longer drops when the page
// unmounts: it survives every `agent:recent` detail and drops only once the
// location is neither (`recentPanelSurvives`, applied by
// `useRecentRunsPanelRouteGuard` in the sidebar).

let open = false
const listeners = new Set<() => void>()

export function setRecentRunsPanelOpen(next: boolean): void {
  if (open === next) return
  open = next
  for (const listener of listeners) listener()
}

export function toggleRecentRunsPanel(): void {
  setRecentRunsPanelOpen(!open)
}

export function useRecentRunsPanelOpen(): boolean {
  const subscribe = useCallback((listener: () => void) => {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }, [])
  const snapshot = useCallback(() => open, [])
  // Server render: the panel is always shut.
  return useSyncExternalStore(subscribe, snapshot, () => false)
}

/** The `?from=` token of a detail opened from the Recent panel. */
export const RECENT_RUNS_ORIGIN = `agent:recent`

/**
 * EXP-1246: what a location does to the flag — `open` on a detail opened
 * from Recent (a deep link lands with its list, and Back finds it open),
 * `keep` on the Agent page itself (its history button decides), `close`
 * everywhere else. Pure, so the lifecycle is a test.
 */
export function recentPanelSurvives(
  pathname: string,
  from: string | null | undefined
): `open` | `keep` | `close` {
  const rest = pathname.match(/^\/t\/[^/]+(\/.*)?$/)?.[1]?.replace(/\/$/, ``)
  if (rest === undefined) return `close`
  if (rest === `/agent`) return `keep`
  const detail =
    /^\/sessions\/[^/]+$/.test(rest) ||
    /^\/boards\/[^/]+\/issues\/[^/]+$/.test(rest)
  return detail && from === RECENT_RUNS_ORIGIN ? `open` : `close`
}

/** Applies `recentPanelSurvives` on every location change. Mounted ONCE, in
 *  the team sidebar (which lives for the whole team layout). */
export function useRecentRunsPanelRouteGuard(): void {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const from = useRouterState({
    select: (s) => {
      const value = (s.location.search as { from?: unknown }).from
      return typeof value === `string` ? value : null
    },
  })
  useEffect(() => {
    const verdict = recentPanelSurvives(pathname, from)
    if (verdict === `open`) setRecentRunsPanelOpen(true)
    if (verdict === `close`) setRecentRunsPanelOpen(false)
  }, [pathname, from])
}
