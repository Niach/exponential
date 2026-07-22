// Per-device agent-dock panel height (EXP-234). The expanded dock viewer is
// drag-resizable like the IDE terminal; the chosen height persists per device
// in localStorage — a layout preference, not server state.
//
// Every accessor guards via `safeLocalStorage` and degrades to the default
// height instead of breaking the dock.

import { safeLocalStorage } from "@/lib/local-storage"

const STORAGE_KEY = `exp.agentDockHeight`

// h-96 — the pre-resizable fixed panel height.
export const AGENT_DOCK_DEFAULT_HEIGHT = 384
export const AGENT_DOCK_MIN_HEIGHT = 160
// Fraction of the viewport the panel may cover — keeps the tab strip and app
// chrome reachable; fullscreen is the "all the way up" affordance.
export const AGENT_DOCK_MAX_VIEWPORT_FRACTION = 0.85

export function clampAgentDockHeight(
  height: number,
  viewportHeight: number
): number {
  const max = Math.max(
    AGENT_DOCK_MIN_HEIGHT,
    Math.round(viewportHeight * AGENT_DOCK_MAX_VIEWPORT_FRACTION)
  )
  return Math.min(max, Math.max(AGENT_DOCK_MIN_HEIGHT, Math.round(height)))
}

export function readAgentDockHeight(): number {
  const store = safeLocalStorage()
  if (!store) return AGENT_DOCK_DEFAULT_HEIGHT
  try {
    const raw = store.getItem(STORAGE_KEY)
    const parsed = raw === null ? Number.NaN : Number(raw)
    if (!Number.isFinite(parsed)) return AGENT_DOCK_DEFAULT_HEIGHT
    return Math.max(AGENT_DOCK_MIN_HEIGHT, Math.round(parsed))
  } catch {
    return AGENT_DOCK_DEFAULT_HEIGHT
  }
}

export function writeAgentDockHeight(height: number): void {
  const store = safeLocalStorage()
  if (!store) return
  try {
    store.setItem(STORAGE_KEY, String(Math.round(height)))
  } catch {
    // Quota/privacy failures just mean the height resets next visit.
  }
}
