import { HOME_RECENT } from "@exp/domain-contract/home-greeting"
import type { CodingSession } from "@/db/schema"
import { runChain } from "@/lib/sessions/run-chain"

// EXP-1253 (EXP-1259 D1): the home column's RECENT band (composer-home.json
// `recent`): the caller's last `recent.count` OWN top-level runs in the
// active team (no parent run), any status, newest first by `updatedAt`, then
// `createdAt`. Review #20: a resume succession (`resumedFromId` chain) is
// ONE row, its newest member, the session tree's rule 1 (`runChain`, the
// newest successor at a fork). Pure, so the rule is a test.

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

/** Each resume succession's newest member (the session tree's collapse:
 *  oldest row first, so the primary succession claims its members before
 *  an older fork sibling does). */
function newestOfEachSuccession<T extends HomeRecentSession>(rows: readonly T[]): T[] {
  const ordered = [...rows].sort(
    (a, b) => time(a.createdAt) - time(b.createdAt) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
  )
  const claimed = new Set<string>()
  const newest: T[] = []
  for (const row of ordered) {
    if (claimed.has(row.id)) continue
    const chain = runChain(rows, row.id).filter((member) => !claimed.has(member.id))
    for (const member of chain) claimed.add(member.id)
    newest.push(chain[chain.length - 1] ?? row)
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
