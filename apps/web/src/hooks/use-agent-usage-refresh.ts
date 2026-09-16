// EXP-817/EXP-862, moved here by EXP-909: the Devices list keeps its own
// numbers current while it is open, and says so nowhere.
//
// Every tick, each login whose last report is past the DEVICE's own rate-limit
// floor (`refreshAllowedAt`) gets ONE `agent_usage_refresh` queued on the
// machine that holds it — never while one is in flight, never twice inside
// `AUTO_REFRESH_RETRY_MS`. The floor is the machine's own 429 budget, so this
// can never out-poll what it allows itself: no refresh button, no "Refreshes
// every 5 minutes" caption to read past.
//
// It fails QUIETLY. A command still queued from the last round answers
// CONFLICT, and the next tick simply looks again — which is also why the global
// error toast is skipped.
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

export function useAgentUsageRefresh(
  rows: readonly AgentProfileUsageRow[],
  /** Which rows may run one at all: mine, online, and advertising the cap. */
  canRefresh: (row: AgentProfileUsageRow) => boolean,
  /** The caller's ticking clock — the loop runs on its beat. */
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

  const attempts = useRef(new Map<string, number>())
  useEffect(() => {
    const at = Date.now()
    for (const row of rows) {
      if (!canRefresh(row)) continue
      if (row.key in refreshing) continue
      if (refreshAllowedAt(row.usage, now) !== null) continue
      const last = attempts.current.get(row.key) ?? 0
      if (at - last < AUTO_REFRESH_RETRY_MS) continue
      attempts.current.set(row.key, at)
      setRefreshing((current) => ({
        ...current,
        [row.key]: { fetchedAt: row.usage?.fetchedAt ?? null, at },
      }))
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
            delete next[row.key]
            return next
          })
        })
    }
    // `refreshing` is read, not a trigger: a mark appearing or clearing must
    // not start another round on its own.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, now])
}
