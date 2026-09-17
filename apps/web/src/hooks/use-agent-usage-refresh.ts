// EXP-817/EXP-862, moved here by EXP-909: the Devices list keeps its own
// numbers current while it is open, and says so nowhere.
//
// Every tick, the logins that CAN report usage (one of the caller's own
// machines, online, signed in, monitored) and whose last report is past the
// DEVICE's own rate-limit floor (`refreshAllowedAt`) queue an
// `agent_usage_refresh` on the machine that holds them — never while one is in
// flight, never twice inside `AUTO_REFRESH_RETRY_MS`, and never more than
// `REFRESH_BUCKET_LIMIT` commands a minute for the whole PAGE (EXP-875: eight
// machines × three logins used to multiply one 60 s floor into a command a
// second). The candidates are served oldest-attempt-first, so a page over the
// bucket still gets round the whole list.
//
// A login that keeps answering NOTHING (no `usage` ever lands) is asked
// `MAX_BLIND_ATTEMPTS` times and then left alone until its row actually
// changes — a machine that cannot read its own numbers will not learn to by
// being asked every minute for an afternoon.
//
// It fails QUIETLY, and it cannot pile up: `agent_usage_refresh` is one of the
// IDEMPOTENT command kinds (`lib/trpc/devices.ts`), so a command still queued
// from the last round is REUSED rather than refused — asking twice asks for
// the same end state. Genuine failures skip the global error toast: there is
// nothing for a reader to do about them.
//
// It also stands down entirely while the tab is in the background: a page
// nobody is looking at has no numbers to keep current.
//
// This used to hang off the cross-device Accounts section (`agent-usage-page.tsx`,
// keyed by account group). It is keyed by `row.key` (`device:agent:profile`)
// now: with the accounts folded under their machines, the LOGIN is the thing
// being refreshed.
import { useEffect, useRef, useState } from "react"
import type { contract } from "@exp/domain-contract"
import { trpc } from "@/lib/trpc-client"
import { refreshAllowedAt, type AgentProfileUsageRow } from "@/lib/agent-usage"

/** How long a queued refresh counts as in flight before giving up on the
 * device answering (it answers by re-reporting on its next beat). */
export const REFRESH_PENDING_MS = 45_000

/** Never re-ask for the same login faster than this. */
export const AUTO_REFRESH_RETRY_MS = 60_000

/** EXP-875: the PAGE's own ceiling — at most this many refresh commands go out
 * per `REFRESH_BUCKET_WINDOW_MS`, however many logins are on screen. */
export const REFRESH_BUCKET_LIMIT = 4
export const REFRESH_BUCKET_WINDOW_MS = 60_000

/** EXP-875: how often one login is asked while its `usage` stays absent
 * before the loop gives up on it (until the row changes). */
export const MAX_BLIND_ATTEMPTS = 3

/** What a login has to BE for a refresh to mean anything: on one of the
 * caller's own machines (a teammate's server takes no commands from here),
 * signed in (a signed-out login can only report nothing) and monitored (an
 * `unmonitored` one is one the machine deliberately collects nothing for).
 * Online-ness and the device's `agent-usage-refresh` cap are the caller's to
 * add. */
export function usageRefreshEligible(
  row: Pick<AgentProfileUsageRow, `mine` | `signedIn` | `unmonitored`>
): boolean {
  return row.mine && row.signedIn && !row.unmonitored
}

/** One login's attempt history: the `usage.fetchedAt` it carried when it was
 * last asked, how many times it has been asked against THAT unchanged stamp,
 * and when the last ask went out. */
export interface UsageRefreshAttempt {
  stamp: string | null
  count: number
  at: number
}

/** The page-level loop state: per-login attempts plus the shared token
 * bucket. Carried in a ref across ticks; `planUsageRefresh` advances it. */
export interface UsageRefreshState {
  attempts: Map<string, UsageRefreshAttempt>
  /** When each command inside the bucket window went out. */
  sent: number[]
}

export function createUsageRefreshState(): UsageRefreshState {
  return { attempts: new Map(), sent: [] }
}

/**
 * Which logins get a command THIS tick — the whole rate story in one pure
 * function (the hook only has to send what it returns). `state` is ADVANCED:
 * the returned keys are recorded as attempts and spend bucket tokens.
 */
export function planUsageRefresh(input: {
  rows: readonly AgentProfileUsageRow[]
  /** The caller's extra verdict (online, the device's cap). */
  eligible: (row: AgentProfileUsageRow) => boolean
  /** Logins with a command already in flight. */
  inFlight: ReadonlySet<string>
  /** The caller's ticking clock — what the device floor is measured against. */
  now: Date
  /** Wall clock for the page-level floors (the tests move it). */
  at: number
  state: UsageRefreshState
}): string[] {
  const { rows, eligible, inFlight, now, at, state } = input

  // Spend-down: forget the commands that have aged out of the window.
  state.sent = state.sent.filter((stamp) => at - stamp < REFRESH_BUCKET_WINDOW_MS)
  const allowance = REFRESH_BUCKET_LIMIT - state.sent.length
  if (allowance <= 0) return []

  const candidates: { key: string; last: number }[] = []
  for (const row of rows) {
    if (!usageRefreshEligible(row)) continue
    if (!eligible(row)) continue
    if (inFlight.has(row.key)) continue
    // The machine's own 429 budget: this loop can never out-poll it.
    if (refreshAllowedAt(row.usage, now) !== null) continue
    const attempt = state.attempts.get(row.key)
    const stamp = row.usage?.fetchedAt ?? null
    if (attempt && attempt.stamp === stamp) {
      if (at - attempt.at < AUTO_REFRESH_RETRY_MS) continue
      // Asked, and asked, and nothing came back: stop until the row moves.
      if (attempt.count >= MAX_BLIND_ATTEMPTS) continue
    }
    candidates.push({ key: row.key, last: attempt?.at ?? 0 })
  }
  if (candidates.length === 0) return []

  // Round-robin: the login waiting longest goes first, so a list bigger than
  // the bucket still comes round to every row instead of starving its tail.
  candidates.sort((a, b) => a.last - b.last || (a.key < b.key ? -1 : 1))
  const picked = candidates.slice(0, allowance)
  const rowByKey = new Map(rows.map((row) => [row.key, row]))
  for (const { key } of picked) {
    const stamp = rowByKey.get(key)?.usage?.fetchedAt ?? null
    const attempt = state.attempts.get(key)
    state.attempts.set(key, {
      stamp,
      count: attempt && attempt.stamp === stamp ? attempt.count + 1 : 1,
      at,
    })
    state.sent.push(at)
  }
  return picked.map((entry) => entry.key)
}

/** Whether this page is the one in front of the reader. */
export function pageIsVisible(): boolean {
  return typeof document === `undefined` || document.visibilityState === `visible`
}

export function useAgentUsageRefresh(
  rows: readonly AgentProfileUsageRow[],
  /** Which rows may run one at all: online, and the device advertising the
   * `agent-usage-refresh` cap. Ownership, sign-in and monitoring are checked
   * here (`usageRefreshEligible`) for every caller. */
  canRefresh: (row: AgentProfileUsageRow) => boolean,
  /** The caller's ticking clock — the loop runs on its beat (`useNow` itself
   * stands still while the tab is hidden). */
  now: Date
): void {
  // Refreshes in flight, keyed by login: the stamp the row carried when it was
  // queued — the device's re-report moves it, which clears the mark.
  const [refreshing, setRefreshing] = useState<
    Record<string, { fetchedAt: string | null; at: number }>
  >({})
  useEffect(() => {
    const keys = Object.keys(refreshing)
    if (keys.length === 0) return
    const next = { ...refreshing }
    let changed = false
    for (const key of keys) {
      const row = rows.find((candidate) => candidate.key === key)
      const stamp = row?.usage?.fetchedAt ?? null
      const entry = refreshing[key]!
      if (
        stamp !== entry.fetchedAt ||
        Date.now() - entry.at > REFRESH_PENDING_MS
      ) {
        delete next[key]
        changed = true
      }
    }
    if (changed) setRefreshing(next)
  }, [rows, refreshing, now])

  const canRefreshRef = useRef(canRefresh)
  canRefreshRef.current = canRefresh
  const stateRef = useRef<UsageRefreshState | null>(null)
  stateRef.current ??= createUsageRefreshState()
  useEffect(() => {
    // Nothing goes out from a tab nobody is looking at.
    if (!pageIsVisible()) return
    const at = Date.now()
    const keys = planUsageRefresh({
      rows,
      eligible: (row) => canRefreshRef.current(row),
      inFlight: new Set(Object.keys(refreshing)),
      now,
      at,
      state: stateRef.current!,
    })
    if (keys.length === 0) return
    const rowByKey = new Map(rows.map((row) => [row.key, row]))
    setRefreshing((current) => {
      const next = { ...current }
      for (const key of keys) {
        next[key] = { fetchedAt: rowByKey.get(key)?.usage?.fetchedAt ?? null, at }
      }
      return next
    })
    for (const key of keys) {
      const row = rowByKey.get(key)
      if (!row) continue
      void trpc.devices.createCommand
        .mutate(
          {
            deviceId: row.deviceId,
            kind: `agent_usage_refresh`,
            agent: row.agent as (typeof contract.codingAgent.values)[number],
            profileId: row.profileId,
          },
          { context: { skipErrorToast: true } }
        )
        .catch(() => {
          setRefreshing((current) => {
            const next = { ...current }
            delete next[key]
            return next
          })
        })
    }
    // `refreshing` is read, not a trigger: a mark appearing or clearing must
    // not start another round on its own.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, now])
}
