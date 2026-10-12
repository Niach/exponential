import { and, eq, isNotNull } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  boards,
  codingSessions,
  issues,
  teams,
  threadItems,
  threads,
} from "@/db/schema"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import {
  appBaseUrl,
  buildIssueDeepLinkPath,
  buildSessionDeepLinkPath,
} from "@/lib/notification-email-policy"
import { PR_BODY_RESULTS_LINK_LABEL, prBodyFromResults } from "@/lib/pr-body-from-results"
import { stampUntaggedResults } from "@/lib/session-result-writes"
import {
  guideOfRun,
  loadGuideOwner,
  lockRunGuide,
  ownsGuideEntry,
  writeRunGuide,
} from "@/lib/thread-guide"
import { getThreadOfSession } from "@/lib/threads"
import { patchPullDescription } from "@/lib/trpc/pr-update"

// EXP-1154: the I/O half of "the PR body IS the run's report" (EXP-1251: its
// Guide). `pr_open` asks `runPrBody` for the body it sends; every text write
// (MCP `sessions_guide`) calls `syncRunPrBody` to re-patch the run's open PR.
// EXP-1251: each PR's body keeps only its own topics (`prUrl`) plus the
// untagged ones; `stampRunResultsPrUrl` tags the untagged ones when the run
// opens a NEW PR, so a later stacked PR never claims them.
// EXP-1284: the Guide is the run's THREAD's (`threads.results`, else the
// run's own results); the PR is still the one stamped on the run's row, and
// its body takes THAT session's entries (its resume chain's, the ones tagged
// with its PR, the unstamped legacy ones): another session's sections in the
// same thread belong to that session's PR.
// One module so the MCP tests mock all of it at once; the pure projection is
// `pr-body-from-results.ts`.

export interface RunReport {
  results: unknown
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  /** The issue's Guide (a run without an issue: the run's page). */
  resultsUrl: string | null
  /** The session and its resume chain: whose entries the body takes. */
  sessionIds: string[]
}

export async function loadRunReport(sessionId: string): Promise<RunReport | null> {
  const [row] = await db
    .select({
      id: codingSessions.id,
      results: codingSessions.results,
      threadResults: threads.results,
      prUrl: codingSessions.prUrl,
      prNumber: codingSessions.prNumber,
      prState: codingSessions.prState,
      issueIdentifier: issues.identifier,
      boardSlug: boards.slug,
      teamSlug: teams.slug,
      threadId: threadItems.threadId,
    })
    .from(codingSessions)
    .leftJoin(issues, eq(issues.id, codingSessions.issueId))
    .leftJoin(boards, eq(boards.id, issues.boardId))
    .leftJoin(teams, eq(teams.id, codingSessions.teamId))
    .leftJoin(threadItems, eq(threadItems.sessionId, codingSessions.id))
    .leftJoin(threads, eq(threads.id, threadItems.threadId))
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
  const owner = await loadGuideOwner(db, row.id, row.threadId ?? null, row.prUrl ?? null)
  return {
    results: guideOfRun(row),
    prUrl: row.prUrl ?? null,
    prNumber: row.prNumber ?? null,
    prState: row.prState ?? null,
    resultsUrl,
    sessionIds: owner.chainIds,
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
      ? prBodyFromResults(report.results, {
          resultsUrl: report.resultsUrl,
          prUrl: null,
          sessionIds: report.sessionIds,
        })
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
 *  GitHub/DB error, never throws. Serialised per run id.
 *  EXP-1284: the Guide is the THREAD's, so a Guide write also re-patches the
 *  open PRs of the thread's OTHER runs (each body keeps its own topics), in
 *  the background; a targeted sync (`expectPrUrl`, pr_open) stays on the run. */
export function syncRunPrBody(
  sessionId: string | null,
  opts: SyncRunPrBodyOptions = {}
): Promise<`synced` | `skipped` | `failed`> {
  if (!sessionId) return Promise.resolve(`skipped`)
  const own = chainRunPrBodySync(sessionId, opts)
  if (opts.expectPrUrl === undefined) {
    void syncThreadSiblingPrBodies(sessionId, opts.removal === true)
  }
  return own
}

function chainRunPrBodySync(
  sessionId: string,
  opts: SyncRunPrBodyOptions
): Promise<`synced` | `skipped` | `failed`> {
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

/** The other runs of the caller's thread holding an OPEN PR other than the
 *  caller's (one per PR), each re-patched on its own chain. Never throws. */
async function syncThreadSiblingPrBodies(
  sessionId: string,
  removal: boolean
): Promise<void> {
  try {
    const ref = await getThreadOfSession(db, sessionId)
    if (!ref) return
    const rows = await db
      .select({ id: codingSessions.id, prUrl: codingSessions.prUrl })
      .from(threadItems)
      .innerJoin(codingSessions, eq(codingSessions.id, threadItems.sessionId))
      .where(
        and(
          eq(threadItems.threadId, ref.id),
          eq(codingSessions.prState, `open`),
          isNotNull(codingSessions.prUrl)
        )
      )
    const own = rows.find((row) => row.id === sessionId)?.prUrl ?? null
    const seen = new Set<string>(own ? [own] : [])
    for (const row of rows) {
      if (row.id === sessionId || !row.prUrl || seen.has(row.prUrl)) continue
      seen.add(row.prUrl)
      void chainRunPrBodySync(row.id, removal ? { removal } : {})
    }
  } catch (err) {
    console.warn(`[run-pr-body] thread PR body sync failed for ${sessionId}`, err)
  }
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
      sessionIds: report.sessionIds,
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
 * its url (under the run's row lock and its thread's, a jsonb
 * read-modify-write of the thread's Guide, EXP-1284). Tagged topics keep
 * theirs. Best effort: false on any error, never throws.
 *
 * `priorPrUrl` = the url the row carried BEFORE this PR (the caller's
 * snapshot; the park has overwritten the column by now). A run that already
 * had a PR wrote its untagged topics for THAT one (a report filed before the
 * tags existed), so they get the first url, never the new PR's; only a run
 * with no prior PR tags them with the new one.
 */
export async function stampRunResultsPrUrl(
  sessionId: string,
  prUrl: string,
  opts: { priorPrUrl?: string | null } = {}
): Promise<boolean> {
  const target = opts.priorPrUrl?.trim() || prUrl
  try {
    return await db.transaction(async (tx) => {
      const locked = await lockRunGuide(tx, sessionId)
      if (!locked) return false
      // Only this session's untagged topics (review T1): another session's
      // in the same thread are its own PR's.
      const stamped = stampUntaggedResults(locked.results, target, (row) =>
        ownsGuideEntry(locked.owner, row)
      )
      if (!stamped.changed) return false
      await writeRunGuide(tx, locked, stamped.results)
      return true
    })
  } catch (err) {
    console.warn(`[run-pr-body] prUrl stamp failed for ${sessionId}`, err)
    return false
  }
}
