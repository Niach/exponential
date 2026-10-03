import type { CodingSession, Issue } from "@/db/schema"

/** EXP-734: what a run's Merge control acts on. An issue-scoped run merges
 * through its issue; every issue-LESS run (batch, action or chat) that opened
 * a PR merges through the SESSION row itself, which carries
 * prUrl/prNumber/prState (a batch run's combined PR too, unless a covered
 * issue carries it; the server fans the merge out to its linked issues). */
export type SessionMergeTarget =
  | { kind: `issue`; issue: Issue }
  | { kind: `session`; session: CodingSession }

/** EXP-734: issue run → the issue; any issue-less run that stamped its OWN
 * PR (batch, action and chat runs) → the session row. EXP-1165: a BATCH run
 * whose combined PR a covered issue carries (same url, still open) merges
 * through that issue instead, so the stack dialog and the "Fix conflicts"
 * recovery the issue path offers (Reviews) reach the run view too. */
export function resolveSessionMergeTarget(
  session: CodingSession,
  issue: Issue | undefined,
  batchIssues: readonly Issue[]
): SessionMergeTarget | undefined {
  if (session.issueId) {
    return issue ? { kind: `issue`, issue } : undefined
  }
  if (!session.prUrl || session.prNumber == null) return undefined
  const carrier = batchIssues.find(
    (covered) => covered.prUrl === session.prUrl && covered.prState === `open`
  )
  if (carrier) return { kind: `issue`, issue: carrier }
  return { kind: `session`, session }
}
