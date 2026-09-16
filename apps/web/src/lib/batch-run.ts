// EXP-876: how a BATCH run is NAMED. Every batch row used to read "Batch
// run" — one string for every batch this team ever ran, so two of them in a
// list (or one running beside last night's) could not be told apart at all.
//
// A batch names itself after the issues it covers: the first one's identifier
// with `+N` for the rest in the mono slot, that issue's title as the subject —
// the same two-part row an issue run renders, so one layout keeps serving
// every kind (EXP-874). The trailing control is still none: EXP-893 took the
// open-issue circle off every session row on every client, and it was exactly
// the control a multi-issue run could never answer.
//
// The covered set has two sources, in this order:
//   1. `coding_sessions.batch_issue_ids` — written at start, so the name is
//      right from the run's first second (the composer's order, preserved).
//   2. the issues sharing the row's `branch` — what `pr_open` stamped on both
//      sides (EXP-545), which names batches started before the column existed
//      or by a client too old to send it, from the moment their PR opens.
//
// Pure, no React, no queries: the caller hands whatever issues it has synced.
// ×4 lockstep — desktop `run_rows::batch_run_name`, iOS `BatchRun.name`,
// Android `batchRunName`: same order, same `+N`, same fallback string, same
// test names (`names a batch after its covered issues`, `falls back to the
// branch`, `falls back to Batch run`).

/** The one string a batch with no knowable issues shows. Byte-identical ×4. */
export const BATCH_RUN_FALLBACK = `Batch run`

/** The launcher's batch branch marker (`exp/batch-<id8>`), deliberately
 *  lowercase so it can never parse as an issue branch. */
export const BATCH_BRANCH_PREFIX = `exp/batch-`

/** The session columns the rule reads. */
export interface BatchRunSession {
  issueId: string | null
  actionName: string | null
  batchIssueIds: string[] | null
  branch: string | null
}

/** The issue columns the rule reads. */
export interface BatchRunIssue {
  id: string
  identifier: string
  title: string
  branch: string | null
  createdAt: Date | string
}

/** An issue-less, action-less run — the batch. (A chat run carries the
 *  reserved `Chat` snapshot, an action run its own, EXP-615.) */
export function isBatchRun(session: Pick<BatchRunSession, `issueId` | `actionName`>): boolean {
  return session.issueId == null && session.actionName == null
}

function stamp(value: Date | string): number {
  const at = typeof value === `string` ? new Date(value) : value
  const ms = at.getTime()
  return Number.isNaN(ms) ? 0 : ms
}

/**
 * The issues a batch run covers, in NAMING order: the stored order when the
 * row recorded it, else the branch-mates oldest first (a deterministic order
 * every client reaches the same way — `created_at` is on every issue row,
 * identifiers break the tie).
 */
export function batchRunIssues<I extends BatchRunIssue>(
  session: BatchRunSession,
  issues: readonly I[]
): I[] {
  if (!isBatchRun(session)) return []
  const ids = session.batchIssueIds ?? []
  if (ids.length > 0) {
    const byId = new Map(issues.map((issue) => [issue.id, issue]))
    return ids
      .map((id) => byId.get(id))
      .filter((issue): issue is I => issue !== undefined)
  }
  const branch = session.branch
  if (!branch || !branch.startsWith(BATCH_BRANCH_PREFIX)) return []
  return issues
    .filter((issue) => issue.branch === branch)
    .sort(
      (a, b) =>
        stamp(a.createdAt) - stamp(b.createdAt) ||
        a.identifier.localeCompare(b.identifier)
    )
}

/** What a batch row shows: `EXP-874 +2` beside the first issue's title. */
export interface BatchRunName {
  /** The mono lead-in — null when no covered issue is known. */
  identifier: string | null
  subject: string
}

/**
 * Name a batch run. `issues` is whatever the caller has synced; only the
 * covered ones are read. A batch whose issues are all unknown (no stored ids,
 * no PR yet — or a row whose issues left the viewer's teams) keeps the old
 * generic label rather than inventing one.
 */
export function batchRunName<I extends BatchRunIssue>(
  session: BatchRunSession,
  issues: readonly I[]
): BatchRunName {
  const covered = batchRunIssues(session, issues)
  const first = covered[0]
  if (!first) return { identifier: null, subject: BATCH_RUN_FALLBACK }
  // The STORED count wins over the resolved one: a batch of three whose
  // middle issue has not synced is still a batch of three, and "+1" would
  // quietly understate what the run is working on.
  const total = Math.max(session.batchIssueIds?.length ?? 0, covered.length)
  return {
    identifier: total > 1 ? `${first.identifier} +${total - 1}` : first.identifier,
    subject: first.title.trim() || `Untitled issue`,
  }
}
