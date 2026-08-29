/**
 * EXP-533: "can we reach the server" for the web app — a direct port of the
 * desktop's `crates/sync/src/health.rs` (itself a port of iOS `SyncDebug`), so
 * all four clients decide "offline" by the same rule and show the same banner.
 *
 * Times are epoch milliseconds (wall clock), NOT a monotonic counter: the
 * staleness window exists exactly for suspend/freeze gaps, and a clock that
 * paused with the machine would make an hours-old streak look fresh on wake.
 * Clock skew saturates to zero.
 *
 * The pure model at the top has no browser dependency and is what the tests
 * drive; the module store below feeds it from the three signals the web has:
 * every Electric shape poll (`shapeFetch` in `collections.ts`), every tRPC
 * round trip (`connectivityLink` in `trpc-client.ts`) and `navigator.onLine`.
 */

/** How long a failure streak must persist (with no intervening success)
 *  before the banner may alarm. TIME-based by design: on wake every shape
 *  long-poll fails at once before the first fresh success, so any consecutive
 *  failure COUNT would trip instantly on a healthy server. */
export const FAILURE_STREAK_GRACE_MS = 12_000

/** An error older than this no longer alarms, and a failure GAP this long
 *  breaks the streak's continuity: while genuinely failing, the retry loops
 *  report far more often than this, so a longer gap means they were not
 *  running (tab frozen, machine suspended) and the first fresh failure must
 *  RESTART the debounce instead of inheriting an hours-old streak start. */
export const ERROR_STALENESS_WINDOW_MS = 300_000

export type SyncHealth = `ok` | `offline`

export interface ConnectivityState {
  lastSuccessAt: number | null
  lastErrorAt: number | null
  /** Start of the CURRENT uninterrupted failure streak: set on the first
   *  failure after a success (or ever), left alone while failures repeat,
   *  cleared by ANY success, restarted after a staleness-sized quiet gap. */
  failureStreakStartedAt: number | null
  /** The most recent failure's display string, for diagnostics. */
  lastError: string | null
}

export function initialConnectivityState(): ConnectivityState {
  return {
    lastSuccessAt: null,
    lastErrorAt: null,
    failureStreakStartedAt: null,
    lastError: null,
  }
}

/** `now - t`, saturating to zero on clock skew. */
function elapsed(t: number, now: number): number {
  return Math.max(0, now - t)
}

export function recordSuccess(
  state: ConnectivityState,
  now: number
): ConnectivityState {
  return { ...state, lastSuccessAt: now, failureStreakStartedAt: null }
}

/** Whether a fresh failure starts a NEW streak instead of extending the
 *  current one (see `ERROR_STALENESS_WINDOW_MS`). */
function streakBroken(state: ConnectivityState, now: number): boolean {
  if (state.failureStreakStartedAt === null) return true
  if (state.lastErrorAt === null) return true
  return elapsed(state.lastErrorAt, now) >= ERROR_STALENESS_WINDOW_MS
}

export function recordFailure(
  state: ConnectivityState,
  now: number,
  error: string
): ConnectivityState {
  return {
    ...state,
    failureStreakStartedAt: streakBroken(state, now)
      ? now
      : state.failureStreakStartedAt,
    lastErrorAt: now,
    lastError: error,
  }
}

/** PURE READ — mirrors desktop `AccountHealth::health` exactly. */
export function health(state: ConnectivityState, now: number): SyncHealth {
  const err = state.lastErrorAt
  if (err === null) return `ok`
  // ANY success after the last failure clears instantly.
  if (state.lastSuccessAt !== null && state.lastSuccessAt > err) return `ok`
  // Staleness guard: an error that stopped repeating long ago (the retry
  // loops died with the tab frozen) must not alarm on wake.
  if (elapsed(err, now) >= ERROR_STALENESS_WINDOW_MS) return `ok`
  const start = state.failureStreakStartedAt
  if (start === null) return `ok`
  return elapsed(start, now) >= FAILURE_STREAK_GRACE_MS ? `offline` : `ok`
}

/**
 * The next epoch-ms instant at which `health()` can flip WITHOUT a new event,
 * or null when only an event can change it. Lets the store arm ONE timer
 * instead of polling.
 */
export function nextHealthChangeAt(
  state: ConnectivityState,
  now: number
): number | null {
  const err = state.lastErrorAt
  if (err === null) return null
  if (state.lastSuccessAt !== null && state.lastSuccessAt > err) return null
  const staleAt = err + ERROR_STALENESS_WINDOW_MS
  if (now >= staleAt) return null
  const start = state.failureStreakStartedAt
  if (start === null) return null
  const graceAt = start + FAILURE_STREAK_GRACE_MS
  if (now < graceAt) return Math.min(graceAt, staleAt)
  return staleAt
}

/** `navigator.onLine` — false is a hard, immediate "offline" that bypasses the
 *  debounce entirely (the OS already knows); true proves nothing on its own. */
export function networkUp(): boolean {
  if (typeof navigator === `undefined`) return true
  return navigator.onLine !== false
}

// ── Module store ────────────────────────────────────────────────────────────
// One process-wide signal, read through `useSyncExternalStore`. Listeners are
// attached with the FIRST subscriber and removed with the last, the same
// registry-owned lifecycle as `steer-session-store.ts`.

let state = initialConnectivityState()
let snapshot: SyncHealth = `ok`
const listeners = new Set<() => void>()
let timer: ReturnType<typeof setTimeout> | undefined

function clearTimer() {
  if (timer !== undefined) {
    clearTimeout(timer)
    timer = undefined
  }
}

function refresh() {
  const now = Date.now()
  const next: SyncHealth = networkUp() ? health(state, now) : `offline`
  const changed = next !== snapshot
  snapshot = next
  clearTimer()
  const at = nextHealthChangeAt(state, now)
  if (at !== null) {
    timer = setTimeout(refresh, Math.max(0, at - now))
  }
  if (changed) for (const listener of listeners) listener()
}

/** A completed round trip of any kind. A 403 or a 409 proves reachability just
 *  as well as a 200 does — only transport failures count against it. */
export function reportTransportSuccess(): void {
  state = recordSuccess(state, Date.now())
  refresh()
}

export function reportTransportFailure(error?: unknown): void {
  const message =
    error instanceof Error ? error.message : error === undefined ? `` : String(error)
  state = recordFailure(state, Date.now(), message)
  refresh()
}

export function getConnectivitySnapshot(): SyncHealth {
  return snapshot
}

/** Server render (and the first client render) always starts optimistic. */
export function getServerConnectivitySnapshot(): SyncHealth {
  return `ok`
}

const onOnline = () => refresh()
const onOffline = () => refresh()
let listenersAttached = false

function attachWindowListeners() {
  if (listenersAttached || typeof window === `undefined`) return
  listenersAttached = true
  window.addEventListener(`online`, onOnline)
  window.addEventListener(`offline`, onOffline)
}

function detachWindowListeners() {
  if (!listenersAttached) return
  listenersAttached = false
  window.removeEventListener(`online`, onOnline)
  window.removeEventListener(`offline`, onOffline)
}

export function subscribeConnectivity(listener: () => void): () => void {
  listeners.add(listener)
  attachWindowListeners()
  refresh()
  return () => {
    listeners.delete(listener)
    if (listeners.size === 0) {
      detachWindowListeners()
      clearTimer()
    }
  }
}

/**
 * The one explicit probe, behind the banner's Retry and the wake-ups: a
 * cheap GET that reports into the same model. Never throws.
 */
export async function probeServerHealth(): Promise<boolean> {
  if (typeof fetch === `undefined`) return false
  try {
    const res = await fetch(`/api/health`, { cache: `no-store` })
    if (res.ok) {
      reportTransportSuccess()
      return true
    }
    // A non-2xx still PROVES the server answered — but /api/health only
    // fails when the database is down, which is an outage the banner should
    // own just like an unreachable host.
    reportTransportFailure(`health probe returned ${res.status}`)
    return false
  } catch (error) {
    reportTransportFailure(error)
    return false
  }
}

/** Test-only: drop every listener, timer and recorded event. */
export function resetConnectivityForTests(): void {
  listeners.clear()
  detachWindowListeners()
  clearTimer()
  state = initialConnectivityState()
  snapshot = `ok`
}
