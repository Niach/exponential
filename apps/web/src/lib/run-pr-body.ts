import { eq } from "drizzle-orm"
import type { CodingSessionResult } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { boards, codingSessions, issues, teams } from "@/db/schema"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import {
  appBaseUrl,
  buildIssueDeepLinkPath,
  buildSessionDeepLinkPath,
} from "@/lib/notification-email-policy"
import { PR_BODY_RESULTS_LINK_LABEL, prBodyFromResults } from "@/lib/pr-body-from-results"
import { stampUntaggedResults } from "@/lib/session-result-writes"
import { patchPullDescription } from "@/lib/trpc/pr-update"

// EXP-1154: the I/O half of "the PR body IS the run's report" (EXP-1251: its
// Guide). `pr_open` asks `runPrBody` for the body it sends; every text write
// (MCP `sessions_guide`) calls `syncRunPrBody` to re-patch the run's open PR.
// EXP-1251: each PR's body keeps only its own topics (`prUrl`) plus the
// untagged ones; `stampRunResultsPrUrl` tags the untagged ones when the run
// opens a NEW PR, so a later stacked PR never claims them.
// One module so the MCP tests mock all of it at once; the pure projection is
// `pr-body-from-results.ts`.

export interface RunReport {
  results: unknown
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  /** The issue's Guide (a run without an issue: the run's page). */
  resultsUrl: string | null
}

export async function loadRunReport(sessionId: string): Promise<RunReport | null> {
  const [row] = await db
    .select({
      id: codingSessions.id,
      results: codingSessions.results,
      prUrl: codingSessions.prUrl,
      prNumber: codingSessions.prNumber,
      prState: codingSessions.prState,
      issueIdentifier: issues.identifier,
      boardSlug: boards.slug,
      teamSlug: teams.slug,
    })
    .from(codingSessions)
    .leftJoin(issues, eq(issues.id, codingSessions.issueId))
    .leftJoin(boards, eq(boards.id, issues.boardId))
    .leftJoin(teams, eq(teams.id, codingSessions.teamId))
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  if (!row) return null
  let resultsUrl: string | null = null
  if (row.teamSlug && row.issueIdentifier && row.boardSlug) {
    resultsUrl = `${appBaseUrl()}${buildIssueDeepLinkPath({
      teamSlug: row.teamSlug,
      boardSlug: row.boardSlug,
      identifier: row.issueIdentifier,
    })}?view=guide`
  } else if (row.teamSlug) {
    // Batch, action and chat runs: the run's own page.
    resultsUrl = `${appBaseUrl()}${buildSessionDeepLinkPath(row.teamSlug, row.id)}?view=guide`
  }
  return {
    results: row.results,
    prUrl: row.prUrl ?? null,
    prNumber: row.prNumber ?? null,
    prState: row.prState ?? null,
    resultsUrl,
  }
}

/** The body `pr_open` sends: the run's report when it has text, else the
 *  caller's `fallback`. The PR does not exist yet, so only the UNTAGGED
 *  topics count (EXP-1251). Never throws: a failed read keeps the fallback. */
export async function runPrBody(
  sessionId: string | null,
  fallback: string | undefined
): Promise<{ body: string | undefined; fromResults: boolean }> {
  if (!sessionId) return { body: fallback, fromResults: false }
  try {
    const report = await loadRunReport(sessionId)
    const derived = report
      ? prBodyFromResults(report.results, { resultsUrl: report.resultsUrl, prUrl: null })
      : null
    if (derived) return { body: derived, fromResults: true }
  } catch (err) {
    console.warn(`[run-pr-body] report read failed for ${sessionId}`, err)
  }
  return { body: fallback, fromResults: false }
}

/** True when the run has report text, i.e. its PR body follows the report. */
export async function runHasReportBody(sessionId: string | null): Promise<boolean> {
  return (await runPrBody(sessionId, undefined)).fromResults
}

function repoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
  return match ? match[1]! : null
}

export interface SyncRunPrBodyOptions {
  /** The sync follows a removed report text: with no text left the body
   *  shrinks to the footer link alone instead of keeping the stale words. */
  removal?: boolean
  /** Sync only while the row's PR is exactly this one (a reused PR whose
   *  stamp `parkSessionInReview` skipped must never re-patch another PR). */
  expectPrUrl?: string
}

// One chain per run: overlapping report writes PATCH in call order, and each
// link re-reads the committed report, so the last PATCH carries the latest.
const syncChains = new Map<string, Promise<unknown>>()

/** Re-patch the run's OPEN PR body from its report. Best effort: skipped
 *  without an open PR (or report text, unless `removal`), `failed` on any
 *  GitHub/DB error, never throws. Serialised per run id. */
export function syncRunPrBody(
  sessionId: string | null,
  opts: SyncRunPrBodyOptions = {}
): Promise<`synced` | `skipped` | `failed`> {
  if (!sessionId) return Promise.resolve(`skipped`)
  const previous = syncChains.get(sessionId) ?? Promise.resolve()
  const run = previous.then(
    () => syncRunPrBodyNow(sessionId, opts),
    () => syncRunPrBodyNow(sessionId, opts)
  )
  syncChains.set(sessionId, run)
  void run.finally(() => {
    if (syncChains.get(sessionId) === run) syncChains.delete(sessionId)
  })
  return run
}

/** Test seam: how many runs have a sync chain in flight. */
export function pendingRunPrBodySyncs(): number {
  return syncChains.size
}

async function syncRunPrBodyNow(
  sessionId: string,
  opts: SyncRunPrBodyOptions
): Promise<`synced` | `skipped` | `failed`> {
  try {
    const report = await loadRunReport(sessionId)
    if (!report || report.prState !== `open` || !report.prUrl || report.prNumber == null) {
      return `skipped`
    }
    if (opts.expectPrUrl !== undefined && report.prUrl !== opts.expectPrUrl) {
      return `skipped`
    }
    let body = prBodyFromResults(report.results, {
      resultsUrl: report.resultsUrl,
      prUrl: report.prUrl,
    })
    if (!body && opts.removal && report.resultsUrl) {
      body = `[${PR_BODY_RESULTS_LINK_LABEL}](${report.resultsUrl})`
    }
    if (!body) return `skipped`
    const repoFullName = repoFromPrUrl(report.prUrl)
    if (!repoFullName) return `skipped`
    const resolved = await resolveRepoInstallationTokenInfo(repoFullName)
    if (!resolved) return `failed`
    await patchPullDescription({
      repoFullName,
      prNumber: report.prNumber,
      token: resolved.token,
      fields: { body },
    })
    return `synced`
  } catch (err) {
    console.warn(`[run-pr-body] PR body sync failed for ${sessionId}`, err)
    return `failed`
  }
}

/**
 * EXP-1251: call once a run's NEW PR is stamped on its row (`pr_open`): the
 * text topics it filed so far without a `prUrl` are that PR's, so they get
 * its url (under the run's row lock, a jsonb read-modify-write). Tagged
 * topics keep theirs. Best effort: false on any error, never throws.
 */
export async function stampRunResultsPrUrl(
  sessionId: string,
  prUrl: string
): Promise<boolean> {
  try {
    return await db.transaction(async (tx) => {
      const [locked] = await tx
        .select({ results: codingSessions.results })
        .from(codingSessions)
        .where(eq(codingSessions.id, sessionId))
        .limit(1)
        .for(`update`)
      if (!locked) return false
      const stamped = stampUntaggedResults(
        (locked.results ?? null) as CodingSessionResult[] | null,
        prUrl
      )
      if (!stamped.changed) return false
      await tx
        .update(codingSessions)
        .set({ results: stamped.results, updatedAt: new Date() })
        .where(eq(codingSessions.id, sessionId))
      return true
    })
  } catch (err) {
    console.warn(`[run-pr-body] prUrl stamp failed for ${sessionId}`, err)
    return false
  }
}
