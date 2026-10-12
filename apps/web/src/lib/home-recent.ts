import { HOME_RECENT } from "@exp/domain-contract/home-greeting"
import type { CodingSession } from "@/db/schema"

// EXP-1253 (EXP-1259 D1): the home column's RECENT band (composer-home.json
// `recent`): the caller's last `recent.count` OWN top-level runs in the
// active team (no parent run), any status, newest first by `updatedAt`, then
// `createdAt`. Review #20: a resume succession (`resumedFromId` chain) is
// ONE row, its newest member, the session tree's rule 1 (`runChain`, the
// newest successor at a fork). Pure, so the rule is a test; every client
// replays composer-home.json `recentCases`.

export type HomeRecentSession = Pick<
  CodingSession,
  | `id`
  | `userId`
  | `teamId`
  | `parentSessionId`
  | `resumedFromId`
  | `updatedAt`
  | `createdAt`
>

const time = (value: Date | string | null | undefined): number =>
  value ? new Date(value).getTime() : 0

const byCreatedThenId = (a: HomeRecentSession, b: HomeRecentSession): number =>
  time(a.createdAt) - time(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)

/** Each resume succession's newest member: the session tree's collapse
 *  (`runChain`'s rules: the newest successor by `createdAt`, then id, at a
 *  fork), oldest row first so the primary succession claims its members
 *  before an older fork sibling does. Final review #20: O(n) after the
 *  sort, the successors indexed ONCE and every row claimed once (a walk
 *  stops at a claimed row) instead of a `runChain` walk per row, since this
 *  reruns on every synced session update (heartbeats included). */
function newestOfEachSuccession<T extends HomeRecentSession>(rows: readonly T[]): T[] {
  const byId = new Map<string, T>()
  const successors = new Map<string, T[]>()
  for (const row of rows) {
    byId.set(row.id, row)
    if (!row.resumedFromId) continue
    const list = successors.get(row.resumedFromId)
    if (list) list.push(row)
    else successors.set(row.resumedFromId, [row])
  }
  const claimed = new Set<string>()
  const newest: T[] = []
  for (const row of [...rows].sort(byCreatedThenId)) {
    if (claimed.has(row.id)) continue
    claimed.add(row.id)
    // Back to the first row: its unclaimed predecessors are this
    // succession's too (a skewed clock can make one look newer).
    let back = row.resumedFromId ? byId.get(row.resumedFromId) : undefined
    while (back && !claimed.has(back.id)) {
      claimed.add(back.id)
      back = back.resumedFromId ? byId.get(back.resumedFromId) : undefined
    }
    // Forward to the latest, the newest unclaimed successor at each fork.
    let last = row
    for (;;) {
      let next: T | undefined
      for (const candidate of successors.get(last.id) ?? []) {
        if (claimed.has(candidate.id)) continue
        if (!next || byCreatedThenId(candidate, next) > 0) next = candidate
      }
      if (!next) break
      claimed.add(next.id)
      last = next
    }
    newest.push(last)
  }
  return newest
}

export function selectHomeRecent<T extends HomeRecentSession>(
  sessions: readonly T[],
  currentUserId: string | undefined,
  teamId: string | undefined,
  count: number = HOME_RECENT.count
): T[] {
  if (!currentUserId || !teamId) return []
  const own = sessions.filter(
    (session) =>
      session.userId === currentUserId &&
      session.teamId === teamId &&
      session.parentSessionId == null
  )
  return newestOfEachSuccession(own)
    .sort(
      (a, b) =>
        time(b.updatedAt) - time(a.updatedAt) ||
        time(b.createdAt) - time(a.createdAt) ||
        (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
    )
    .slice(0, count)
}
