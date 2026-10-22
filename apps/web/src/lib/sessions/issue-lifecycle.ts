import { and, eq, inArray, isNotNull, ne, sql } from "drizzle-orm"
import { issueLifecycleTarget } from "@exp/domain-contract/issue-lifecycle"
import { issues, sessionIssues, sessions, type Session } from "@/db/schema"
import { applyPrLifecycleStatusInTx, withLivePrInTx } from "@/lib/integrations/pr-sync"
import type { IssueEventOrigin } from "@/lib/integrations/event-origin"
import type { Tx } from "@/lib/sessions/tx"

// NEW-1 (plan 3.7): the session side of the issue lifecycle, ONE rule
// (fixtures/issue-lifecycle.json): a WORKER (a session with a repository)
// taking an issue moves it to In progress, from backlog or from In review;
// the last worker dropping an issue nobody reviews sends it back to
// backlog. The PR side (`opened`, `merged`) stays in pr-sync. Nothing else
// moves status.

export type WorkerKey = Pick<Session, `id` | `teamId` | `repositoryId`>

export const isWorker = (session: Pick<Session, `repositoryId`>): boolean =>
  session.repositoryId !== null

/** A worker takes `issueIds` (a row landed on it, a continuation took its
 *  rows, a person messaged it): backlog / in review → In progress. A session
 *  without a repository takes nothing. */
export async function applyIssuesTakenInTx(
  tx: Tx,
  opts: {
    session: WorkerKey
    issueIds: readonly string[]
    actorUserId: string | null
    origin?: IssueEventOrigin
  }
): Promise<string[]> {
  const ids = [...new Set(opts.issueIds)]
  if (ids.length === 0 || !isWorker(opts.session)) return []
  const rows = await tx
    .select({ id: issues.id, status: issues.status })
    .from(issues)
    .where(and(inArray(issues.id, ids), eq(issues.teamId, opts.session.teamId)))
  const live = await withLivePrInTx(tx, rows.map((row) => row.id))
  const moved: string[] = []
  for (const row of rows) {
    const target = issueLifecycleTarget(`taken`, {
      status: row.status,
      openPr: live.has(row.id),
      workers: 1,
    })
    if (target !== `in_progress`) continue
    const applied = await applyPrLifecycleStatusInTx(tx, {
      issueId: row.id,
      teamId: opts.session.teamId,
      actorUserId: opts.actorUserId,
      currentStatus: row.status,
      event: row.status === `in_review` ? `resumed` : `started`,
      payloadExtra: { event: `taken`, sessionId: opts.session.id },
      ...(opts.origin ? { origin: opts.origin } : {}),
    })
    if (applied) moved.push(row.id)
  }
  return moved
}

/** Live workers (a repository, not ended) still holding each issue. */
async function workersHoldingInTx(tx: Tx, issueIds: string[]): Promise<Map<string, number>> {
  const counts = new Map<string, number>()
  if (issueIds.length === 0) return counts
  const rows = await tx
    .select({ issueId: sessionIssues.issueId, count: sql<number>`count(*)::int` })
    .from(sessionIssues)
    .innerJoin(sessions, eq(sessions.id, sessionIssues.sessionId))
    .where(
      and(
        inArray(sessionIssues.issueId, issueIds),
        isNotNull(sessions.repositoryId),
        ne(sessions.status, `ended`)
      )
    )
    .groupBy(sessionIssues.issueId)
  for (const row of rows) counts.set(row.issueId, Number(row.count))
  return counts
}

/** Rows of `issueIds` were dropped (an unlink, an ended continuation's
 *  leftovers): an issue In progress that no live worker holds and no PR
 *  reviews goes back to backlog. Call AFTER the rows are gone. */
export async function applyIssuesUnlinkedInTx(
  tx: Tx,
  opts: {
    teamId: string
    issueIds: readonly string[]
    actorUserId: string | null
    origin?: IssueEventOrigin
  }
): Promise<string[]> {
  const ids = [...new Set(opts.issueIds)]
  if (ids.length === 0) return []
  const rows = await tx
    .select({ id: issues.id, status: issues.status })
    .from(issues)
    .where(and(inArray(issues.id, ids), eq(issues.teamId, opts.teamId)))
  const live = await withLivePrInTx(tx, rows.map((row) => row.id))
  const workers = await workersHoldingInTx(tx, rows.map((row) => row.id))
  const moved: string[] = []
  for (const row of rows) {
    const target = issueLifecycleTarget(`unlinked`, {
      status: row.status,
      openPr: live.has(row.id),
      workers: workers.get(row.id) ?? 0,
    })
    if (target !== `backlog`) continue
    const applied = await applyPrLifecycleStatusInTx(tx, {
      issueId: row.id,
      teamId: opts.teamId,
      actorUserId: opts.actorUserId,
      currentStatus: row.status,
      event: `unlinked`,
      ...(opts.origin ? { origin: opts.origin } : {}),
    })
    if (applied) moved.push(row.id)
  }
  return moved
}
