// EXP-988 contract / EXP-974: the resume chain of a coding session.
//
// A resume is the SAME run under a new row (`resumed_from_id` → predecessor,
// EXP-637/EXP-906), so every member of the succession wears ONE toggle and
// the `Runs` segment's menu is where the reader picks between them — the
// "Continues an earlier run" band that used to open the run view is gone
// (EXP-974). `runChain` is a pure SELECTOR over the synced `coding_sessions`
// rows: it follows `resumedFromId` BACKWARDS to the first row and FORWARDS to
// the latest, and returns the chain oldest-first, the named session included.
// An unknown id yields []. A fork (two rows resuming the same predecessor)
// follows the NEWEST successor by `createdAt` (ties by id, so the pick is
// stable); seen from an older sibling the chain is its own past plus itself.
// A predecessor the sweep deleted (a dangling `resumedFromId`) simply ends
// the backward walk. ×4 lockstep: desktop `queries::run_chain`, iOS
// `RunChain.chain`, Android `runChain` — same rules, same test names.
//
// Who reads it: the run menu behind the work header's `Runs` caret and the
// phone switcher's run rows (`selectIssueRuns` for an issue-bound run, whose
// own runs already include its resumes; THIS for an issue-less run, which
// has no issue to list runs under). EXP-902 must NOT touch the toggle — it
// only changes which face a tab opens on.
import type { CodingSession } from "@/db/schema"

export type SessionRow = Pick<
  CodingSession,
  `id` | `resumedFromId` | `createdAt`
>

function stamp(value: Date | string | null | undefined): number {
  if (!value) return 0
  const at = typeof value === `string` ? new Date(value) : value
  const ms = at.getTime()
  return Number.isNaN(ms) ? 0 : ms
}

/** The newest of several rows resuming the same predecessor: `createdAt`
 *  descending, then id descending, so two clients agree on the fork. */
function newest<T extends SessionRow>(rows: readonly T[]): T | undefined {
  let best: T | undefined
  for (const row of rows) {
    if (
      !best ||
      stamp(row.createdAt) > stamp(best.createdAt) ||
      (stamp(row.createdAt) === stamp(best.createdAt) && row.id > best.id)
    ) {
      best = row
    }
  }
  return best
}

export function runChain<T extends SessionRow>(
  rows: readonly T[],
  sessionId: string
): T[] {
  const byId = new Map(rows.map((row) => [row.id, row]))
  const start = byId.get(sessionId)
  if (!start) return []
  const seen = new Set<string>([start.id])

  // Backwards to the first row. A missing predecessor (swept) ends the walk;
  // a cycle (never written by the server, but a synced row is a synced row)
  // ends it too.
  const before: T[] = []
  let cursor: T | undefined = start
  while (cursor?.resumedFromId) {
    const previous = byId.get(cursor.resumedFromId)
    if (!previous || seen.has(previous.id)) break
    seen.add(previous.id)
    before.push(previous)
    cursor = previous
  }
  before.reverse()

  // Forwards to the latest, the newest successor at every fork.
  const after: T[] = []
  cursor = start
  while (cursor) {
    const current: T = cursor
    const next = newest(
      rows.filter((row) => row.resumedFromId === current.id && !seen.has(row.id))
    )
    if (!next) break
    seen.add(next.id)
    after.push(next)
    cursor = next
  }

  return [...before, start, ...after]
}
