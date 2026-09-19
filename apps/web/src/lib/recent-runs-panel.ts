import { useCallback, useSyncExternalStore } from "react"

// EXP-923: the Agent page's RECENT runs panel — open state, and nothing else.
//
// The page is the composer alone now; its history sits behind one ghost
// button that slides the sidebar's panel slot in beside the compact rail. The
// flag is deliberately NOT in the URL (a disclosure, not a destination, and a
// shared link must not reopen it) and deliberately NOT React state (the
// button lives on the Agent page, the panel in the sidebar — two subtrees
// with the team layout between them), so it is a three-line external store on
// the `use-work-tabs.ts` rails.
//
// It is dropped whenever the Agent page unmounts, so leaving the route always
// closes the panel and returning shows the composer at full width.

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
