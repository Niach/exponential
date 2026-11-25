import { useSyncExternalStore } from "react"
import type { useNavigate } from "@tanstack/react-router"
import {
  LEGACY_WORK_TABS_PREFIX,
  migrateLegacyWorkTabs,
  NEW_RUN_PATH,
  openNewRunTab,
  openTabInBackground,
  PREVIOUS_WORK_TABS_STORAGE_KEY,
  WORK_TABS_STORAGE_KEY,
  type WorkTab,
  type WorkTabsState,
} from "@/lib/work-tabs"
import { forgetClosedTabs } from "@/lib/work-tab-memory"

// EXP-870: the work tabs' store — persisted in `sessionStorage` (per browser
// window, like a browser's own tabs: a second window starts clean, a reload
// keeps them). #EXP-2027: ONE state for the person across every member team
// (each tab carries its team); the first read merges the old per-team v3
// blobs into it and drops them (#EXP-2124: and the v4 blob into v5). A module-level external store so the sync
// component, the strip and the face toggles all read one snapshot through
// `useSyncExternalStore`. Storage can throw (private mode, blocked site
// data) or hold junk: both read as an empty strip and the store keeps
// working in memory.

let cache: WorkTabsState | null = null
const listeners = new Set<() => void>()

function persist(state: WorkTabsState): void {
  try {
    window.sessionStorage.setItem(WORK_TABS_STORAGE_KEY, JSON.stringify(state))
  } catch {
    // Memory-only for this window — the strip still works.
  }
}

function read(): WorkTabsState {
  if (cache) return cache
  let current: string | null = null
  let previous: string | null = null
  const legacy: { teamId: string; raw: string | null }[] = []
  try {
    const storage = window.sessionStorage
    current = storage.getItem(WORK_TABS_STORAGE_KEY)
    previous = storage.getItem(PREVIOUS_WORK_TABS_STORAGE_KEY)
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index)
      if (!key?.startsWith(LEGACY_WORK_TABS_PREFIX)) continue
      legacy.push({ teamId: key.slice(LEGACY_WORK_TABS_PREFIX.length), raw: storage.getItem(key) })
    }
  } catch {
    current = null
    previous = null
  }
  const state = migrateLegacyWorkTabs(current, legacy, previous)
  cache = state
  if (legacy.length > 0 || previous !== null) {
    persist(state)
    try {
      window.sessionStorage.removeItem(PREVIOUS_WORK_TABS_STORAGE_KEY)
      for (const entry of legacy) {
        window.sessionStorage.removeItem(`${LEGACY_WORK_TABS_PREFIX}${entry.teamId}`)
      }
    } catch {
      // The next window's first read merges them again: duplicates collapse.
    }
  }
  return state
}

/** Apply a pure transition (`lib/work-tabs.ts`) to the tabs. A transition
 * that returns the same object writes nothing. */
export function updateWorkTabs(transition: (state: WorkTabsState) => WorkTabsState): void {
  const current = read()
  const next = transition(current)
  if (next === current) return
  // EXP-894: a closed (or pruned) tab takes its remembered drafts with it.
  forgetClosedTabs(current.tabs, next.tabs)
  cache = next
  persist(next)
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function useWorkTabs(): WorkTabsState {
  return useSyncExternalStore(subscribe, read, read)
}

/** EXP-1255: open (or focus) the New session tab and show its page, Home
 *  (#EXP-2027) — Alt/Option+T (#EXP-2124: Home's tab; the strip's "+" opens
 *  the new-tab menu). */
export function openNewRunPage(navigate: ReturnType<typeof useNavigate>): void {
  updateWorkTabs(openNewRunTab)
  void navigate({ to: NEW_RUN_PATH })
}

/** #EXP-2124: a row's "Open in new tab": the tab at the end in the
 *  BACKGROUND (no navigation; the current tab stays), never twice. */
export function openWorkTabInBackground(tab: WorkTab): void {
  updateWorkTabs((state) => openTabInBackground(state, tab))
}

/** Test seam: forget the cached state. */
export function resetWorkTabsStore(): void {
  cache = null
}
