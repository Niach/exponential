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
// The covered set is `coding_sessions.batch_issue_ids`, written at start
// (the composer's order, preserved), so the name is right from the run's
// first second. NULL = "not a batch" (or nothing to name it by); rows from
// before the column existed were backfilled once from their branch's issues
// (migration 0133, EXP-972), so no client reads the branch any more.
//
// Pure, no React, no queries: the caller hands whatever issues it has synced.
// ×4 lockstep — desktop `run_rows::batch_run_name`, iOS `BatchRun.name`,
// Android `batchRunName`: same order, same `+N`, same fallback string, same
// test names (`names a batch after its covered issues`, `falls back to Batch
// run`).

/** The one string a batch with no knowable issues shows. Byte-identical ×4. */
export const BATCH_RUN_FALLBACK = `Batch run`

/** The session columns the rule reads. */
export interface BatchRunSession {
  issueId: string | null
  actionName: string | null
  batchIssueIds: string[] | null
}

/** The issue columns the rule reads. */
export interface BatchRunIssue {
  id: string
  identifier: string
  title: string
}

/** An issue-less, action-less run — the batch. (A chat run carries the
 *  reserved `Chat` snapshot, an action run its own, EXP-615.) */
export function isBatchRun(session: Pick<BatchRunSession, `issueId` | `actionName`>): boolean {
  return session.issueId == null && session.actionName == null
}

/**
 * The issues a batch run covers, in NAMING order: the order the row stored.
 * An id whose issue the caller has not synced is skipped, never invented.
 */
export function batchRunIssues<I extends BatchRunIssue>(
  session: BatchRunSession,
  issues: readonly I[]
): I[] {
  if (!isBatchRun(session)) return []
  const ids = session.batchIssueIds ?? []
  if (ids.length === 0) return []
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  return ids
    .map((id) => byId.get(id))
    .filter((issue): issue is I => issue !== undefined)
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
 * or a row whose issues left the viewer's teams) keeps the old generic label
 * rather than inventing one.
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
