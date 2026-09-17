import { useCallback, useSyncExternalStore } from "react"
import {
  EMPTY_WORK_TABS,
  parseCollapsedGroups,
  parseWorkTabsState,
  workTabGroupsStorageKey,
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

// ── EXP-877: the collapsed agent groups ──────────────────────────────────────
// A second store on the same rails: which live-run groups are folded to their
// brand mark, per team and per window. Values are plain agent ids; a snapshot
// is cached so `useSyncExternalStore` sees a stable array.

const EMPTY_GROUPS: string[] = []
const groupCache = new Map<string, string[]>()
const groupListeners = new Map<string, Set<() => void>>()

function readGroups(teamId: string): string[] {
  const cached = groupCache.get(teamId)
  if (cached) return cached
  let raw: string | null = null
  try {
    raw = window.sessionStorage.getItem(workTabGroupsStorageKey(teamId))
  } catch {
    raw = null
  }
  const collapsed = parseCollapsedGroups(raw)
  groupCache.set(teamId, collapsed)
  return collapsed
}

/** Fold or unfold one agent's group. A no-op write notifies nobody. */
export function setTabGroupCollapsed(
  teamId: string,
  agent: string,
  collapsed: boolean
): void {
  const current = readGroups(teamId)
  if (current.includes(agent) === collapsed) return
  const next = collapsed
    ? [...current, agent]
    : current.filter((id) => id !== agent)
  groupCache.set(teamId, next)
  try {
    window.sessionStorage.setItem(
      workTabGroupsStorageKey(teamId),
      JSON.stringify(next)
    )
  } catch {
    // Memory-only for this window — the strip still folds.
  }
  for (const listener of groupListeners.get(teamId) ?? []) listener()
}

export function useCollapsedTabGroups(teamId: string | undefined): string[] {
  const subscribe = useCallback(
    (listener: () => void) => {
      if (!teamId) return () => {}
      let set = groupListeners.get(teamId)
      if (!set) {
        set = new Set()
        groupListeners.set(teamId, set)
      }
      set.add(listener)
      return () => {
        set.delete(listener)
      }
    },
    [teamId]
  )
  const snapshot = useCallback(
    () => (teamId ? readGroups(teamId) : EMPTY_GROUPS),
    [teamId]
  )
  return useSyncExternalStore(subscribe, snapshot, snapshot)
}

/** Test seam: forget every cached team state. */
export function resetWorkTabsStore(): void {
  cache.clear()
  groupCache.clear()
}
