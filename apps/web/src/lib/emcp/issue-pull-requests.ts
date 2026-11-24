import { and, desc, eq, inArray, ne, or, sql } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { statusPill, type HoldReason, type MergeWait, type PrMergeState } from "@exp/domain-contract/merge-train"
import { db } from "@/db/connection"
import { pullRequestIssues, pullRequests, sessionIssues, sessions } from "@/db/schema"

// #NEW-23: `exponential_issues {op: "get"}` names the issue's pull requests
// and its live sessions, so a reader outside the app (the Claude Code status
// line and pane, an agent) knows where the work stands without a second call. `status` = the header pill's
// label (merge-train.json `statusPill`), the same words every client draws.

/** How many PRs one issue's reply lists (newest first). */
export const ISSUE_PULL_REQUESTS_CAP = 10

export interface IssuePullRequestWire {
  pullRequestId: string
  number: number
  url: string
  state: string
  status: string
  checks: string
}

export interface IssuePullRequestRow {
  id: string
  number: number
  url: string
  state: string
  checksState: string
  mergeState: string
  mergeWait: string | null
  mergePosition: number | null
  mergeHold: string | null
}

/** The pill's label for a PR row; a closed PR has no pill, so it reads Closed. */
export function issuePullRequestStatus(row: IssuePullRequestRow, testingWith = 0): string {
  if (row.state === `closed`) return `Closed`
  return statusPill({
    state: (row.state === `merged` ? `merged` : row.mergeState) as PrMergeState,
    wait: (row.mergeWait as MergeWait | null) ?? null,
    position: row.mergePosition,
    batchSize: testingWith + 1,
    holdReason: (row.mergeHold as HoldReason | null) ?? null,
    stack: false,
    checks: 0,
  }).label
}

export function issuePullRequestWire(row: IssuePullRequestRow, testingWith = 0): IssuePullRequestWire {
  return {
    pullRequestId: row.id,
    number: row.number,
    url: row.url,
    state: row.state,
    status: issuePullRequestStatus(row, testingWith),
    checks: row.checksState,
  }
}

/** The PRs that close the issue (a `pr unlink` reference is not one). */
export async function loadIssuePullRequests(issueId: string): Promise<IssuePullRequestWire[]> {
  const rows = await db
    .select({
      id: pullRequests.id,
      number: pullRequests.number,
      url: pullRequests.url,
      state: pullRequests.state,
      checksState: pullRequests.checksState,
      mergeState: pullRequests.mergeState,
      mergeWait: pullRequests.mergeWait,
      mergePosition: pullRequests.mergePosition,
      mergeHold: pullRequests.mergeHold,
      mergeTrainId: pullRequests.mergeTrainId,
    })
    .from(pullRequestIssues)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestIssues.pullRequestId))
    .where(and(eq(pullRequestIssues.issueId, issueId), eq(pullRequestIssues.closes, true)))
    .orderBy(desc(pullRequests.createdAt))
    .limit(ISSUE_PULL_REQUESTS_CAP)
  const trainIds = [...new Set(rows.filter((row) => row.mergeState === `testing` && row.mergeTrainId).map((row) => row.mergeTrainId!))]
  const batches = trainIds.length
    ? await db
        .select({ trainId: pullRequests.mergeTrainId, count: sql<number>`count(*)::int` })
        .from(pullRequests)
        .where(and(inArray(pullRequests.mergeTrainId, trainIds), eq(pullRequests.mergeState, `testing`), ne(pullRequests.state, `merged`)))
        .groupBy(pullRequests.mergeTrainId)
    : []
  const batchSize = new Map(batches.map((batch) => [batch.trainId, batch.count]))
  return rows.map((row) =>
    issuePullRequestWire(row, row.mergeState === `testing` && row.mergeTrainId ? Math.max(0, (batchSize.get(row.mergeTrainId) ?? 1) - 1) : 0)
  )
}

/** How many live sessions one issue's reply lists (latest first). */
export const ISSUE_SESSIONS_CAP = 5

export interface IssueSessionWire {
  sessionId: string
  rootId: string
  status: string
  busy: boolean
  needsInput: boolean
  branch: string | null
  updatedAt: Date
}

/** The live sessions that hold the issue, under THE visibility rule
 *  (lib/sessions/access.ts: shared or the caller's own, on the row and its
 *  root). */
export async function loadIssueSessions(issueId: string, userId: string): Promise<IssueSessionWire[]> {
  const root = alias(sessions, `issue_session_root`)
  return db
    .select({
      sessionId: sessions.id,
      rootId: sessions.rootId,
      status: sessions.status,
      busy: sessions.busy,
      needsInput: sessions.needsInput,
      branch: sessions.branch,
      updatedAt: sessions.updatedAt,
    })
    .from(sessionIssues)
    .innerJoin(sessions, eq(sessions.id, sessionIssues.sessionId))
    .innerJoin(root, eq(root.id, sessions.rootId))
    .where(
      and(
        eq(sessionIssues.issueId, issueId),
        ne(sessions.status, `ended`),
        or(eq(sessions.shared, true), eq(sessions.userId, userId)),
        or(eq(root.shared, true), eq(root.userId, userId))
      )
    )
    .orderBy(desc(sessions.updatedAt))
    .limit(ISSUE_SESSIONS_CAP)
}
