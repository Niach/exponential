import { useSyncExternalStore } from "react"
// EXP-1156: the column numbers are the shared `sidebar` token group, read
// straight from the canonical tokens.json (@exp/design-tokens is not a
// dependency of this app; the IDE gets the same group generated into Rust).
import designTokens from "../../../../packages/design-tokens/tokens.json" with { type: "json" }
import { safeLocalStorage } from "@/lib/local-storage"

// EXP-1156: the DRAGGED width of every sidebar column, per panel, md+ only.
//
// Units: the tokens are 16px DESIGN units, the same numbers the IDE draws in
// px. The web column renders them as rem (`sidebarWidthCss`), so 272 stays
// the 17rem it always was at the md+ root font (1.15625rem, styles.css) and
// the whole chrome keeps scaling together; the drag converts pointer pixels
// back through `sidebarUnitScale()`.
//
// The width is a LOCAL chrome preference: one localStorage key, never synced.
// A module store on the `recent-runs-panel.ts` rails, because the handle (in
// the sidebar) and the provider's `--sidebar-width` (the team layout) are two
// subtrees that must agree on every frame of a drag.

const tokens = designTokens.sidebar

export const SIDEBAR_RAIL_WIDTH: number = tokens.railWidth
export const SIDEBAR_MIN_WIDTH: number = tokens.minWidth
export const SIDEBAR_MAX_WIDTH: number = tokens.maxWidth
export const SIDEBAR_HANDLE_WIDTH: number = tokens.handleWidth
export const SIDEBAR_KEYBOARD_STEP: number = tokens.keyboardStep

/** The panels a column can hold — `SidebarOccupant['kind']`. */
export type SidebarPanelKey = `main` | `list` | `review` | `settings` | `recent`

export const SIDEBAR_PANEL_KEYS: readonly SidebarPanelKey[] = [
  `main`,
  `list`,
  `review`,
  `settings`,
  `recent`,
]

const DEFAULTS: Record<SidebarPanelKey, number> = {
  main: tokens.defaultMain,
  list: tokens.defaultList,
  review: tokens.defaultReview,
  settings: tokens.defaultSettings,
  recent: tokens.defaultRecent,
}

export const SIDEBAR_WIDTHS_STORAGE_KEY = `exp.sidebarWidths`

export function sidebarDefaultWidth(key: SidebarPanelKey): number {
  return DEFAULTS[key]
}

/** The main menu is FIXED at its default: only the panels beside the rail
 * drag, and a stored `main` (an older build's) is never read. */
export function sidebarResizable(key: SidebarPanelKey): boolean {
  return key !== `main`
}

/** The rail is beside every panel but the main menu. */
export function sidebarHasRail(key: SidebarPanelKey): boolean {
  return key !== `main`
}

/**
 * The bounds a panel's width may take: the shared min/max, and — given the
 * viewport (design units) — the whole column (rail + panel, or the main menu
 * alone) at most half of it, never below the minimum.
 */
export function sidebarWidthBounds(
  opts: { viewportWidth?: number | null; withRail?: boolean } = {}
): { min: number; max: number } {
  let max = SIDEBAR_MAX_WIDTH
  if (opts.viewportWidth != null && Number.isFinite(opts.viewportWidth)) {
    const rail = opts.withRail ? SIDEBAR_RAIL_WIDTH : 0
    max = Math.min(max, Math.floor(opts.viewportWidth / 2) - rail)
  }
  return { min: SIDEBAR_MIN_WIDTH, max: Math.max(SIDEBAR_MIN_WIDTH, max) }
}

export function clampSidebarWidth(
  width: number,
  opts: { viewportWidth?: number | null; withRail?: boolean } = {}
): number {
  const { min, max } = sidebarWidthBounds(opts)
  return Math.round(Math.min(Math.max(width, min), max))
}

/** Tolerant: garbage, non-finite numbers and unknown keys are dropped. */
export function parseSidebarWidths(
  raw: string | null | undefined
): Partial<Record<SidebarPanelKey, number>> {
  if (!raw) return {}
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    return {}
  }
  if (typeof parsed !== `object` || parsed === null || Array.isArray(parsed)) {
    return {}
  }
  const out: Partial<Record<SidebarPanelKey, number>> = {}
  for (const key of SIDEBAR_PANEL_KEYS) {
    if (!sidebarResizable(key)) continue
    const value = (parsed as Record<string, unknown>)[key]
    if (typeof value === `number` && Number.isFinite(value)) out[key] = value
  }
  return out
}

/** CSS px per design unit — the root font over 16 (1.15625 at md+). */
export function sidebarUnitScale(): number {
  if (typeof window === `undefined`) return 1
  const px = parseFloat(getComputedStyle(document.documentElement).fontSize)
  return Number.isFinite(px) && px > 0 ? px / 16 : 1
}

/** A design-unit width as the CSS length the column renders. */
export function sidebarWidthCss(width: number): string {
  return `${width / 16}rem`
}

/* ------------------------------------------------------------------ store */

type Widths = Record<SidebarPanelKey, number>

interface Snapshot {
  /** Each panel's width, stored or default, clamped to its bounds. */
  widths: Widths
  /** Each panel's current maximum (the viewport rule included). */
  max: Widths
}

let stored: Partial<Widths> | null = null
let viewport: number | null = null
let snapshot: Snapshot | null = null
const listeners = new Set<() => void>()

function load(): Partial<Widths> {
  if (stored === null) {
    let raw: string | null = null
    try {
      raw = safeLocalStorage()?.getItem(SIDEBAR_WIDTHS_STORAGE_KEY) ?? null
    } catch {
      raw = null
    }
    stored = parseSidebarWidths(raw)
  }
  return stored
}

function sameWidths(a: Widths, b: Widths): boolean {
  return SIDEBAR_PANEL_KEYS.every((key) => a[key] === b[key])
}

function resolve(): Snapshot {
  const values = load()
  const widths = {} as Widths
  const max = {} as Widths
  for (const key of SIDEBAR_PANEL_KEYS) {
    const opts = { viewportWidth: viewport, withRail: sidebarHasRail(key) }
    widths[key] = clampSidebarWidth(values[key] ?? DEFAULTS[key], opts)
    max[key] = sidebarWidthBounds(opts).max
  }
  return { widths, max }
}

/**
 * Recompute; notify only when something moved. An unchanged half keeps its
 * object, so `useSidebarWidths` consumers skip a viewport-only change.
 */
function refresh(): void {
  const next = resolve()
  if (snapshot) {
    const widthsSame = sameWidths(snapshot.widths, next.widths)
    const maxSame = sameWidths(snapshot.max, next.max)
    if (widthsSame && maxSame) return
    if (widthsSame) next.widths = snapshot.widths
    if (maxSame) next.max = snapshot.max
  }
  snapshot = next
  for (const listener of listeners) listener()
}

function persist(): void {
  try {
    safeLocalStorage()?.setItem(
      SIDEBAR_WIDTHS_STORAGE_KEY,
      JSON.stringify(load())
    )
  } catch {
    // Blocked storage: the width still holds for this page's life.
  }
}

function measureViewport(): number | null {
  if (typeof window === `undefined`) return null
  return window.innerWidth / sidebarUnitScale()
}

function onWindowResize(): void {
  viewport = measureViewport()
  refresh()
}

/**
 * Sets a panel's width in memory at once (every frame of a drag); `persist`
 * writes the whole map to storage — the drag passes it on commit only.
 */
export function setSidebarWidth(
  key: SidebarPanelKey,
  width: number,
  { persist: write = true }: { persist?: boolean } = {}
): void {
  if (!Number.isFinite(width) || !sidebarResizable(key)) return
  load()[key] = Math.round(width)
  if (write) persist()
  refresh()
}

/** Back to the token default: the key leaves storage entirely. */
export function resetSidebarWidth(key: SidebarPanelKey): void {
  delete load()[key]
  persist()
  refresh()
}

function subscribe(listener: () => void): () => void {
  if (typeof window !== `undefined`) {
    // The first subscriber starts the (cheap) resize watch; the viewport is
    // measured then, while nobody listened it may have moved.
    if (listeners.size === 0) {
      window.addEventListener(`resize`, onWindowResize)
      viewport = null
    }
    if (viewport === null) {
      viewport = measureViewport()
      refresh()
    }
  }
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0 && typeof window !== `undefined`) {
      window.removeEventListener(`resize`, onWindowResize)
    }
  }
}

function getSnapshot(): Snapshot {
  if (snapshot === null) snapshot = resolve()
  return snapshot
}

const SERVER_SNAPSHOT: Snapshot = {
  widths: { ...DEFAULTS },
  max: resolveServerMax(),
}

function resolveServerMax(): Widths {
  const max = {} as Widths
  for (const key of SIDEBAR_PANEL_KEYS) max[key] = SIDEBAR_MAX_WIDTH
  return max
}

/**
 * Every panel's width (stored or default), clamped to the bounds and to half
 * the current viewport; one stable object until a width moves.
 */
export function useSidebarWidths(): Widths {
  return useSyncExternalStore(
    subscribe,
    () => getSnapshot().widths,
    () => SERVER_SNAPSHOT.widths
  )
}

export function useSidebarWidth(key: SidebarPanelKey): number {
  return useSidebarWidths()[key]
}

/** A panel's live drag bounds — what its `ResizeHandle` takes. */
export function useSidebarWidthBounds(key: SidebarPanelKey): {
  min: number
  max: number
} {
  const max = useSyncExternalStore(
    subscribe,
    () => getSnapshot().max[key],
    () => SERVER_SNAPSHOT.max[key]
  )
  return { min: SIDEBAR_MIN_WIDTH, max }
}

/* --------------------------------------------------------------- dragging */

// Whether a sidebar drag is in progress: the layout sets `data-resizing` on
// the sidebar wrapper from it, which switches every width transition in the
// column off (the column must track the pointer, not ease after it).
let resizing = false
const resizingListeners = new Set<() => void>()

export function setSidebarResizing(next: boolean): void {
  if (resizing === next) return
  resizing = next
  for (const listener of resizingListeners) listener()
}

function subscribeResizing(listener: () => void): () => void {
  resizingListeners.add(listener)
  return () => {
    resizingListeners.delete(listener)
  }
}

export function useSidebarResizing(): boolean {
  return useSyncExternalStore(
    subscribeResizing,
    () => resizing,
    () => false
  )
}

/**
 * The whole column's width for an occupant — the main menu alone, or the
 * folded rail plus the panel beside it. What `--sidebar-width` carries.
 */
export function sidebarColumnWidth(
  widths: Record<SidebarPanelKey, number>,
  key: SidebarPanelKey
): number {
  return sidebarHasRail(key) ? SIDEBAR_RAIL_WIDTH + widths[key] : widths[key]
}

/** Tests only: forget the in-memory map so the next read re-parses storage. */
export function resetSidebarWidthsStoreForTests(): void {
  stored = null
  snapshot = null
  viewport = null
}
