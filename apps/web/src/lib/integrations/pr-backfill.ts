/**
 * EXP-1705 (merge train part 0): a one-off backfill that re-reads every live
 * (`open`/`draft`) `pull_requests` row against its forge and applies the
 * merges and closes Exponential missed, so the train never queues a PR that
 * already landed. A merge goes through `applyPullRequestMerged` with the
 * forge's own `merged_at` (the `pr_merged` events, the merge automation, the
 * thread's card); a close through `applyPullRequestClosed`. A reopen is not
 * possible here: only live rows are read.
 *
 * The forge read sits behind `ForgePullReader` (GitHub today; #EXP-1709's
 * forge seam replaces `githubPullReader`). Dry run by default: the plan
 * lists each PR's forge state and the linked issues with their current and
 * target status, without writing anything.
 *
 * Part two (`findOrphanedMerges`): the audit's misses were not live rows at
 * all but MERGED rows whose issue links were already gone, so the merge
 * stamped nothing. They get their `pr_merged` event; only the issues the
 * operator names also take the merge automation.
 */
import { eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { issueEvents, issueStatuses, issues, pullRequestIssues, pullRequests } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { decidePrPollAction, parseRepoFromPrUrl } from "@/lib/bootstrap-self-hosted"
import { fetchPullState, resolveRepoToken } from "@/lib/integrations/github-pr"
import { resolveAppUserForGithubActor } from "@/lib/integrations/github-identity"
import {
  applyPrEventToIssuesInTx,
  applyPullRequestClosed,
  applyPullRequestMerged,
  resolveAutomationTarget,
} from "@/lib/integrations/pr-sync"
import { LIVE_PR_STATES } from "@/lib/pr-merge-guard"

/** What a forge says about one PR, provider-neutral. */
export interface ForgePullState {
  state: `open` | `closed`
  merged: boolean
  mergedAt: Date | null
  baseRef: string | null
  /** The forge's account that merged it, resolved to an app user. */
  mergedByUserId: string | null
}

export type ForgePullReader = (pr: {
  url: string
  number: number
  teamId: string
}) => Promise<ForgePullState | null>

/** GitHub: null for a URL that is not a GitHub PR (another forge's row). */
export const githubPullReader: ForgePullReader = async (pr) => {
  const repo = parseRepoFromPrUrl(pr.url)
  if (!repo) return null
  const token = await resolveRepoToken({ teamId: pr.teamId, repo })
  const state = await fetchPullState(repo, pr.number, token)
  return {
    state: state.state,
    merged: state.merged,
    mergedAt: state.mergedAt ?? null,
    baseRef: state.baseRef,
    mergedByUserId: state.merged ? await resolveAppUserForGithubActor(state.mergedBy) : null,
  }
}

export interface BackfillIssue {
  identifier: string
  from: string
  to: string | null
  /** After an applied run: the status it ended at. */
  after?: string
}

export interface BackfillEntry {
  url: string
  number: number
  localState: string
  action: `merge` | `close` | `none` | `skipped` | `failed`
  forge: string
  mergedAt: Date | null
  /** Linked issues with their status now and the merge automation's target
   *  (null = the automation is off). A close moves no issue. */
  issues: BackfillIssue[]
  error?: string
}

/** The forge state as one word: open, draft-or-open stays `open`. */
export function forgeLabel(state: ForgePullState | null): string {
  if (!state) return `unsupported forge`
  if (state.merged) return `merged`
  return state.state
}

async function linkedIssues(pullRequestId: string, teamId: string): Promise<BackfillIssue[]> {
  const rows = await db
    .select({ identifier: issues.identifier, status: issueStatuses.name })
    .from(pullRequestIssues)
    .innerJoin(issues, eq(issues.id, pullRequestIssues.issueId))
    .leftJoin(issueStatuses, eq(issueStatuses.id, issues.statusId))
    .where(eq(pullRequestIssues.pullRequestId, pullRequestId))
  const automation = await db.transaction((tx) => resolveAutomationTarget(tx, teamId, `merged`))
  const to = automation.enabled ? (automation.target?.name ?? null) : null
  return rows.map((row) => ({ identifier: row.identifier, from: row.status ?? `?`, to }))
}

/** One pass over every live row. `apply: false` = the dry-run plan. */
export async function runLivePrBackfill(opts: {
  apply: boolean
  read?: ForgePullReader
}): Promise<BackfillEntry[]> {
  const read = opts.read ?? githubPullReader
  const rows = await db
    .select({
      id: pullRequests.id,
      url: pullRequests.url,
      number: pullRequests.number,
      state: pullRequests.state,
      teamId: pullRequests.teamId,
    })
    .from(pullRequests)
    .where(inArray(pullRequests.state, [...LIVE_PR_STATES]))
    .orderBy(pullRequests.createdAt)
  const out: BackfillEntry[] = []
  for (const row of rows) {
    const entry: BackfillEntry = {
      url: row.url,
      number: row.number,
      localState: row.state,
      action: `none`,
      forge: `?`,
      mergedAt: null,
      issues: [],
    }
    out.push(entry)
    try {
      const state = await read(row)
      entry.forge = forgeLabel(state)
      if (!state) {
        entry.action = `skipped`
        continue
      }
      const action = decidePrPollAction(row.state, state)
      if (action !== `merge` && action !== `close`) continue
      entry.action = action
      entry.mergedAt = state.mergedAt
      if (action === `merge`) entry.issues = await linkedIssues(row.id, row.teamId)
      if (!opts.apply) continue
      if (action === `merge`) {
        await applyPullRequestMerged({
          pullRequestId: row.id,
          mergedAt: state.mergedAt ?? new Date(),
          base: state.baseRef,
          actorUserId: null,
          githubActorUserId: state.mergedByUserId,
        })
        const after = await linkedIssues(row.id, row.teamId)
        for (const issue of entry.issues) {
          issue.after = after.find((a) => a.identifier === issue.identifier)?.from
        }
      } else {
        await applyPullRequestClosed({ pullRequestId: row.id })
      }
    } catch (err) {
      entry.action = `failed`
      entry.error = err instanceof Error ? err.message : String(err)
    }
  }
  return out
}

/** A merged PR an issue opened (its `pr_opened` event names the URL) that
 *  never recorded `pr_merged` on that issue: the link was gone by the merge,
 *  so `applyPullRequestMerged` found no issue to stamp (#1933 → EXP-1679). */
export interface OrphanedMerge {
  pullRequestId: string
  url: string
  number: number
  mergedAt: Date | null
  issueId: string
  identifier: string
  teamId: string
  status: string
  /** The issue's status category is already completed or cancelled. */
  settled: boolean
}

export async function findOrphanedMerges(): Promise<OrphanedMerge[]> {
  const result = await db.execute(sql`
    SELECT DISTINCT p.id AS pull_request_id, p.url, p.number, p.merged_at,
      i.id AS issue_id, i.identifier, i.team_id, s.name AS status,
      s.category::text IN ('completed', 'cancelled', 'canceled') AS settled
    FROM ${issueEvents} e
    JOIN ${pullRequests} p ON p.url = e.payload->>'prUrl' AND p.state = 'merged'
    JOIN ${issues} i ON i.id = e.issue_id
    LEFT JOIN ${issueStatuses} s ON s.id = i.status_id
    WHERE e.type = 'pr_opened'
      AND NOT EXISTS (
        SELECT 1 FROM ${issueEvents} m
        WHERE m.issue_id = i.id AND m.type = 'pr_merged' AND m.payload->>'prUrl' = p.url
      )
      AND NOT EXISTS (
        SELECT 1 FROM ${pullRequestIssues} pi
        WHERE pi.issue_id = i.id AND pi.pull_request_id = p.id
      )
    ORDER BY p.merged_at, i.identifier
  `)
  return (result.rows as Array<Record<string, unknown>>).map((row) => ({
    pullRequestId: String(row.pull_request_id),
    url: String(row.url),
    number: Number(row.number),
    mergedAt: row.merged_at ? new Date(String(row.merged_at)) : null,
    issueId: String(row.issue_id),
    identifier: String(row.identifier),
    teamId: String(row.team_id),
    status: String(row.status ?? `?`),
    settled: Boolean(row.settled),
  }))
}

/** Records the missing `pr_merged` events, dated at the real merge (no
 *  notification: these merges are days old). `move` = the identifiers that
 *  also take the team's merge automation; every other issue keeps its
 *  status. */
export async function applyOrphanedMerges(
  orphans: OrphanedMerge[],
  move: ReadonlySet<string>
): Promise<string[]> {
  const moved: string[] = []
  for (const orphan of orphans) {
    await db.transaction(async (tx) => {
      await generateTxId(tx)
      await tx.insert(issueEvents).values({
        issueId: orphan.issueId,
        teamId: orphan.teamId,
        boardId: sql`(select ${issues.boardId} from ${issues} where ${issues.id} = ${orphan.issueId})`,
        actorUserId: null,
        type: `pr_merged`,
        payload: { prUrl: orphan.url, backfill: `EXP-1705` },
        ...(orphan.mergedAt ? { createdAt: orphan.mergedAt, updatedAt: orphan.mergedAt } : {}),
      })
      if (!move.has(orphan.identifier) || moved.includes(orphan.identifier)) return
      const ids = await applyPrEventToIssuesInTx(tx, {
        teamId: orphan.teamId,
        issueIds: [orphan.issueId],
        event: `merged`,
        actorUserId: null,
      })
      if (ids.length > 0) moved.push(orphan.identifier)
    })
  }
  return moved
}
