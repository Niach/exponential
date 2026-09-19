import { useCallback, useSyncExternalStore } from "react"
import {
  EMPTY_WORK_TABS,
  parseWorkTabsState,
  workTabsStorageKey,
  type WorkTabsState,
} from "@/lib/work-tabs"
import { forgetClosedTabs } from "@/lib/work-tab-memory"

// EXP-870: the work tabs' store — one state per TEAM, persisted in
// `sessionStorage` (per browser window, like a browser's own tabs: a second
// window starts clean, a reload keeps them). A module-level external store so
// the sync component, the strip and the face toggles all read one snapshot
// through `useSyncExternalStore`. Storage can throw (private mode, blocked
// site data) or hold junk: both read as an empty strip and the store keeps
// working in memory.

const cache = new Map<string, WorkTabsState>()
const listeners = new Map<string, Set<() => void>>()

function read(teamId: string): WorkTabsState {
  const cached = cache.get(teamId)
  if (cached) return cached
  let raw: string | null = null
  try {
    raw = window.sessionStorage.getItem(workTabsStorageKey(teamId))
  } catch {
    raw = null
  }
  const state = parseWorkTabsState(raw)
  cache.set(teamId, state)
  return state
}

/** Apply a pure transition (`lib/work-tabs.ts`) to a team's tabs. A
 * transition that returns the same object writes nothing. */
export function updateWorkTabs(
  teamId: string,
  transition: (state: WorkTabsState) => WorkTabsState
): void {
  const current = read(teamId)
  const next = transition(current)
  if (next === current) return
  // EXP-894: a closed (or pruned) tab takes its remembered drafts with it.
  forgetClosedTabs(current.tabs, next.tabs)
  cache.set(teamId, next)
  try {
    window.sessionStorage.setItem(workTabsStorageKey(teamId), JSON.stringify(next))
  } catch {
    // Memory-only for this window — the strip still works.
  }
  for (const listener of listeners.get(teamId) ?? []) listener()
}

export function useWorkTabs(teamId: string | undefined): WorkTabsState {
  const subscribe = useCallback(
    (listener: () => void) => {
      if (!teamId) return () => {}
      let set = listeners.get(teamId)
      if (!set) {
        set = new Set()
        listeners.set(teamId, set)
      }
      set.add(listener)
      return () => {
        set.delete(listener)
      }
    },
    [teamId]
  )
  const snapshot = useCallback(
    () => (teamId ? read(teamId) : EMPTY_WORK_TABS),
    [teamId]
  )
  return useSyncExternalStore(subscribe, snapshot, snapshot)
}

/** Test seam: forget every cached team state. */
export function resetWorkTabsStore(): void {
  cache.clear()
}
