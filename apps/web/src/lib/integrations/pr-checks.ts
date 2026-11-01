import { and, eq, inArray, isNull, lt, ne, notInArray, or, sql } from "drizzle-orm"
import {
  aggregateChecksState,
  checkConclusionFromGithub,
  checkStatusFromGithub,
  checksBrief,
  checksQueuedText,
  checksWakeText,
  isFailingCheck,
  PR_CHECKS_QUEUED_FLAG_MINUTES,
  type PrChecksBrief,
  type PrChecksState,
} from "@exp/domain-contract/pr-checks"
import { db } from "@/db/connection"
import { messages, pullRequestChecks, pullRequests, repositories, sessions } from "@/db/schema"
import { githubApiHeaders, resolveRepoInstallationToken } from "@/lib/integrations/github-app"
import { githubFetch, isGithubRateLimited } from "@/lib/integrations/github-client"
import { insertMessage, inSession } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { generateTxId } from "@/lib/trpc"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"
import { LIVE_PR_STATES } from "@/lib/pr-merge-guard"

// EXP-1610 (fixtures/pr-checks.json): CI on a PR, provider-neutral. The ONE
// writer of `pull_request_checks` + `pull_requests.checks_state`: GitHub's
// check_run webhook, the repair read (check_suite/workflow_run completed, the
// next `exponential_pr` op) and — later — eCode's own CI all land here. When
// the aggregate turns success/failure, ONE `pr_checks` line wakes the PR's
// session: that is what replaces agents watching CI.

export interface CheckInput {
  name: string
  status: `queued` | `in_progress` | `completed`
  conclusion: string | null
  startedAt: Date | null
  completedAt: Date | null
  detailsUrl: string | null
}

type CheckRow = typeof pullRequestChecks.$inferSelect

export interface ChecksWrite {
  before: PrChecksState
  after: PrChecksState
  headSha: string
  /** false = a late delivery for an older head, dropped. */
  applied: boolean
}

function parseDate(value: string | null | undefined): Date | null {
  if (!value) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

/** One GitHub check run (webhook `check_run` or the REST list) → a row. */
export function checkFromGithubRun(run: {
  name?: string
  status?: string | null
  conclusion?: string | null
  started_at?: string | null
  completed_at?: string | null
  details_url?: string | null
  html_url?: string | null
}): CheckInput | null {
  if (!run.name) return null
  const status = checkStatusFromGithub(run.status)
  return {
    name: run.name.slice(0, 255),
    status,
    conclusion: checkConclusionFromGithub(status, run.conclusion),
    startedAt: parseDate(run.started_at),
    completedAt: status === `completed` ? parseDate(run.completed_at) : null,
    // The run's page on GitHub (its logs); `details_url` may point at an
    // external CI, which is the logs link then.
    detailsUrl: run.details_url || run.html_url || null,
  }
}

/** Write the checks of `pullRequestId` at `headSha`. `snapshot` = the
 *  complete list (the repair read): names it lacks are dropped. A delivery
 *  for an older head (its newest start older than the current head's) is
 *  dropped; a newer head replaces the older head's rows. */
export async function writePullRequestChecks(args: {
  pullRequestId: string
  headSha: string
  checks: readonly CheckInput[]
  snapshot?: boolean
  required?: ReadonlySet<string>
  now?: Date
}): Promise<ChecksWrite | null> {
  const now = args.now ?? new Date()
  const result = await db.transaction(async (tx) => {
    const [pr] = await tx
      .select({ id: pullRequests.id, teamId: pullRequests.teamId, checksState: pullRequests.checksState })
      .from(pullRequests)
      .where(eq(pullRequests.id, args.pullRequestId))
      .limit(1)
      .for(`update`)
    if (!pr) return null
    const existing = await tx
      .select()
      .from(pullRequestChecks)
      .where(eq(pullRequestChecks.pullRequestId, pr.id))
    const before = pr.checksState as PrChecksState
    const older = existing.filter((row) => row.headSha !== args.headSha)
    if (older.length > 0) {
      // A late delivery for an older head started before the current head's
      // newest check: drop it. Without start times the newer write wins.
      const oldStarts = older.flatMap((row) => (row.startedAt ? [row.startedAt.getTime()] : []))
      const incoming = args.checks.flatMap((check) => (check.startedAt ? [check.startedAt.getTime()] : []))
      if (oldStarts.length > 0 && incoming.length > 0 && Math.max(...incoming) < Math.max(...oldStarts)) {
        return { before, after: before, headSha: args.headSha, applied: false }
      }
      await tx
        .delete(pullRequestChecks)
        .where(and(eq(pullRequestChecks.pullRequestId, pr.id), ne(pullRequestChecks.headSha, args.headSha)))
    }
    for (const check of args.checks) {
      const required = args.required?.has(check.name) ?? existing.find((row) => row.name === check.name)?.required ?? false
      const values = {
        headSha: args.headSha,
        status: check.status,
        conclusion: check.conclusion as CheckRow[`conclusion`],
        required,
        startedAt: check.startedAt,
        completedAt: check.completedAt,
        detailsUrl: check.detailsUrl,
      }
      await tx
        .insert(pullRequestChecks)
        .values({ teamId: pr.teamId, pullRequestId: pr.id, name: check.name, ...values })
        .onConflictDoUpdate({
          target: [pullRequestChecks.pullRequestId, pullRequestChecks.name],
          set: {
            ...values,
            // Still queued keeps the once-only flag; once it ran, a later
            // re-run's queue wait may be flagged again.
            queuedFlaggedAt: check.status === `queued` ? sql`${pullRequestChecks.queuedFlaggedAt}` : null,
            updatedAt: now,
          },
        })
    }
    if (args.snapshot) {
      const names = args.checks.map((check) => check.name)
      await tx
        .delete(pullRequestChecks)
        .where(
          and(
            eq(pullRequestChecks.pullRequestId, pr.id),
            ...(names.length > 0 ? [notInArray(pullRequestChecks.name, names)] : [])
          )
        )
    }
    const rows = await tx.select().from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, pr.id))
    const after = aggregateChecksState(rows)
    if (after !== before) {
      await generateTxId(tx)
      await tx.update(pullRequests).set({ checksState: after, updatedAt: now }).where(eq(pullRequests.id, pr.id))
    }
    return { before, after, headSha: args.headSha, applied: true }
  })
  if (result?.applied && result.after !== result.before && (result.after === `success` || result.after === `failure`)) {
    await postChecksWake({ pullRequestId: args.pullRequestId, headSha: result.headSha, state: result.after })
  }
  return result
}

// ── wake ─────────────────────────────────────────────────────────────────

const sessionCols = {
  id: sessions.id,
  rootId: sessions.rootId,
  teamId: sessions.teamId,
  shared: sessions.shared,
  userId: sessions.userId,
  status: sessions.status,
}

/** The live session a PR's CI line wakes: its opener while live, else the
 *  thread's root; null when both ended (nobody to tell). */
async function wakeTarget(
  tx: Tx,
  pr: { rootSessionId: string | null; sessionId: string | null }
) {
  for (const id of [pr.sessionId, pr.rootSessionId]) {
    if (!id) continue
    const [row] = await tx.select(sessionCols).from(sessions).where(eq(sessions.id, id)).limit(1)
    if (row && row.status !== `ended`) return row
  }
  return null
}

async function postPrChecksLine(
  tx: Tx,
  args: {
    pullRequestId: string
    body: string
    checksState: `success` | `failure` | `queued`
    headSha: string | null
    /** Skip when this PR's line for the same head and state exists. */
    dedupe: boolean
  }
): Promise<boolean> {
  const [pr] = await tx
    .select({ rootSessionId: pullRequests.rootSessionId, sessionId: pullRequests.sessionId })
    .from(pullRequests)
    .where(eq(pullRequests.id, args.pullRequestId))
    .limit(1)
  if (!pr) return false
  const target = await wakeTarget(tx, pr)
  if (!target) return false
  if (args.dedupe) {
    const [seen] = await tx
      .select({ id: messages.id })
      .from(messages)
      .where(
        and(
          inSession(target.id),
          sql`${messages.card}->>'event' = 'pr_checks'`,
          sql`${messages.card}->>'pullRequestId' = ${args.pullRequestId}`,
          sql`${messages.card}->>'checksState' = ${args.checksState}`,
          args.headSha ? sql`${messages.card}->>'headSha' = ${args.headSha}` : undefined
        )
      )
      .limit(1)
    if (seen) return false
  }
  await generateTxId(tx)
  // Pending input like a refused merge (merge-refusal.ts): the session
  // wakes and reads the line; nobody watches CI.
  await insertMessage(tx, target, {
    authorKind: `user`,
    authorId: target.userId,
    body: args.body,
    card: {
      kind: `event`,
      event: `pr_checks`,
      pullRequestId: args.pullRequestId,
      checksState: args.checksState,
      ...(args.headSha ? { headSha: args.headSha } : {}),
    },
  })
  return true
}

/** CI left pending: `CI on #N: success / failure (names)`, once per PR,
 *  head and state. Never throws. */
export async function postChecksWake(args: {
  pullRequestId: string
  headSha: string
  state: `success` | `failure`
}): Promise<boolean> {
  try {
    return await db.transaction(async (tx) => {
      const [pr] = await tx
        .select({ number: pullRequests.number, state: pullRequests.state })
        .from(pullRequests)
        .where(eq(pullRequests.id, args.pullRequestId))
        .limit(1)
      if (!pr || !(LIVE_PR_STATES as readonly string[]).includes(pr.state)) return false
      const rows = await tx.select().from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, args.pullRequestId))
      const failing = rows.filter(isFailingCheck).map((row) => ({ name: row.name, detailsUrl: row.detailsUrl }))
      return postPrChecksLine(tx, {
        pullRequestId: args.pullRequestId,
        body: checksWakeText(pr.number, args.state, failing),
        checksState: args.state,
        headSha: args.headSha,
        dedupe: true,
      })
    })
  } catch (err) {
    console.error(`[pr-checks] wake for ${args.pullRequestId} failed`, err)
    return false
  }
}

/** Watchdog: a check still queued `queuedFlagMinutes` after it was created
 *  is flagged into the PR's session ONCE (no runner took it). → how many
 *  PRs were flagged. */
export async function flagQueuedChecks(now: Date = new Date()): Promise<number> {
  const cutoff = new Date(now.getTime() - PR_CHECKS_QUEUED_FLAG_MINUTES * 60_000)
  const stale = await db
    .select({ id: pullRequestChecks.id, pullRequestId: pullRequestChecks.pullRequestId, name: pullRequestChecks.name })
    .from(pullRequestChecks)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestChecks.pullRequestId))
    .where(
      and(
        eq(pullRequestChecks.status, `queued`),
        isNull(pullRequestChecks.queuedFlaggedAt),
        // GitHub stamps a queued run's start at creation; eCode may not.
        lt(sql`coalesce(${pullRequestChecks.startedAt}, ${pullRequestChecks.createdAt})`, cutoff),
        inArray(pullRequests.state, [...LIVE_PR_STATES])
      )
    )
    .limit(200)
  const byPr = new Map<string, typeof stale>()
  for (const row of stale) byPr.set(row.pullRequestId, [...(byPr.get(row.pullRequestId) ?? []), row])
  let flagged = 0
  for (const [pullRequestId, rows] of byPr) {
    try {
      const posted = await db.transaction(async (tx) => {
        // The conditional stamp claims the rows (a concurrent tick gets none).
        const claimed = await tx
          .update(pullRequestChecks)
          .set({ queuedFlaggedAt: now })
          .where(and(inArray(pullRequestChecks.id, rows.map((row) => row.id)), isNull(pullRequestChecks.queuedFlaggedAt)))
          .returning({ id: pullRequestChecks.id, name: pullRequestChecks.name })
        if (claimed.length === 0) return false
        const [pr] = await tx
          .select({ number: pullRequests.number })
          .from(pullRequests)
          .where(eq(pullRequests.id, pullRequestId))
          .limit(1)
        if (!pr) return false
        return postPrChecksLine(tx, {
          pullRequestId,
          body: checksQueuedText(pr.number, claimed.map((row) => row.name)),
          checksState: `queued`,
          headSha: null,
          // The conditional stamp above is the once-only guard.
          dedupe: false,
        })
      })
      if (posted) flagged++
    } catch (err) {
      console.error(`[pr-checks] queued flag for ${pullRequestId} failed`, err)
    }
  }
  return flagged
}

// ── GitHub: webhook routing + the repair read ─────────────────────────────

/** The tracked live PRs a GitHub check delivery is about: the repo's PRs by
 *  number (the payload's `pull_requests`) or, when GitHub sends none (a fork
 *  or a push before the PR), by head branch. Every team that connected the
 *  repo has its own row. */
export async function pullRequestsForCheck(opts: {
  repoFullName: string
  numbers: readonly number[]
  headBranch: string | null
}): Promise<Array<{ id: string; repositoryId: string; base: string }>> {
  const repoIds = db.select({ id: repositories.id }).from(repositories).where(eq(repositories.fullName, opts.repoFullName))
  const match = [
    ...(opts.numbers.length > 0 ? [inArray(pullRequests.number, [...opts.numbers])] : []),
    ...(opts.headBranch ? [eq(pullRequests.head, opts.headBranch)] : []),
  ]
  if (match.length === 0) return []
  return db
    .select({ id: pullRequests.id, repositoryId: pullRequests.repositoryId, base: pullRequests.base })
    .from(pullRequests)
    .where(
      and(
        inArray(pullRequests.repositoryId, repoIds),
        inArray(pullRequests.state, [...LIVE_PR_STATES]),
        or(...match)
      )
    )
}

// The base branch's required checks (classic branch protection + rulesets),
// cached 10 min per repo+branch; conditional GETs make the refresh free. A
// read the App may not do (no permission, no protection) = none required.
const requiredCache = new TtlPromiseCache<Set<string>>({ ttlMs: 10 * 60_000, maxEntries: 500 })

export function requiredChecksFor(repoFullName: string, base: string, token: string): Promise<Set<string>> {
  return requiredCache.get(`${repoFullName}#${base}`, async () => {
    const names = new Set<string>()
    const branch = encodeURIComponent(base)
    try {
      const res = await githubFetch(`/repos/${repoFullName}/branches/${branch}`, { headers: githubApiHeaders(token) })
      if (res.ok) {
        const data = (await res.json()) as {
          protection?: { required_status_checks?: { contexts?: string[]; checks?: Array<{ context?: string }> } }
        }
        const rsc = data.protection?.required_status_checks
        for (const context of rsc?.contexts ?? []) names.add(context)
        for (const check of rsc?.checks ?? []) if (check.context) names.add(check.context)
      }
      const rules = await githubFetch(`/repos/${repoFullName}/rules/branches/${branch}`, { headers: githubApiHeaders(token) })
      if (rules.ok) {
        const data = (await rules.json()) as Array<{
          type?: string
          parameters?: { required_status_checks?: Array<{ context?: string }> }
        }>
        for (const rule of Array.isArray(data) ? data : []) {
          if (rule.type !== `required_status_checks`) continue
          for (const check of rule.parameters?.required_status_checks ?? []) if (check.context) names.add(check.context)
        }
      }
    } catch (err) {
      if (!isGithubRateLimited(err)) console.error(`[pr-checks] required checks of ${repoFullName}@${base}`, err)
    }
    return names
  })
}

/** GitHub's check runs at `ref` (a sha or branch), or null when they cannot
 *  be read (no checks:read yet, rate limited, gone): "no checks known". */
export async function fetchGithubCheckRuns(
  repoFullName: string,
  ref: string,
  token: string
): Promise<{ headSha: string | null; checks: CheckInput[] } | null> {
  const path = ref.split(`/`).map(encodeURIComponent).join(`/`)
  let res: Response
  try {
    res = await githubFetch(`/repos/${repoFullName}/commits/${path}/check-runs?filter=latest&per_page=100`, {
      headers: githubApiHeaders(token),
    })
  } catch (err) {
    if (!isGithubRateLimited(err)) console.error(`[pr-checks] check runs of ${repoFullName}@${ref}`, err)
    return null
  }
  // 403 = the App lacks checks:read (the human step); 404/422 = no such ref.
  if (!res.ok) return null
  const data = (await res.json()) as {
    check_runs?: Array<Parameters<typeof checkFromGithubRun>[0] & { head_sha?: string }>
  }
  const runs = data.check_runs ?? []
  // filter=latest already keeps one run per name; keep the first anyway.
  const byName = new Map<string, CheckInput>()
  for (const run of runs) {
    const check = checkFromGithubRun(run)
    if (check && !byName.has(check.name)) byName.set(check.name, check)
  }
  return { headSha: runs[0]?.head_sha ?? null, checks: [...byName.values()] }
}

// One repair read per PR per minute, however many ops/deliveries ask.
const REPAIR_THROTTLE_MS = 60_000
const lastRepair = new Map<string, number>()

/** Re-read a live PR's checks from GitHub (a missed delivery, the next `pr`
 *  op). Throttled; never throws. */
export async function repairPullRequestChecks(pullRequestId: string, now: number = Date.now()): Promise<void> {
  const last = lastRepair.get(pullRequestId)
  if (last !== undefined && now - last < REPAIR_THROTTLE_MS) return
  lastRepair.set(pullRequestId, now)
  if (lastRepair.size > 5_000) lastRepair.clear()
  try {
    const [row] = await db
      .select({
        id: pullRequests.id,
        head: pullRequests.head,
        base: pullRequests.base,
        state: pullRequests.state,
        fullName: repositories.fullName,
        installationId: repositories.installationId,
      })
      .from(pullRequests)
      .innerJoin(repositories, eq(repositories.id, pullRequests.repositoryId))
      .where(eq(pullRequests.id, pullRequestId))
      .limit(1)
    if (!row || !(LIVE_PR_STATES as readonly string[]).includes(row.state)) return
    const token = await resolveRepoInstallationToken(row.fullName, { fallbackInstallationId: row.installationId })
    if (!token) return
    const runs = await fetchGithubCheckRuns(row.fullName, row.head, token)
    if (!runs?.headSha) {
      // No run at the head at all: nothing known (an older head's rows go).
      if (runs && runs.checks.length === 0) {
        await db.delete(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, row.id))
        await db.update(pullRequests).set({ checksState: `none` }).where(and(eq(pullRequests.id, row.id), ne(pullRequests.checksState, `none`)))
      }
      return
    }
    const required = await requiredChecksFor(row.fullName, row.base, token)
    await writePullRequestChecks({ pullRequestId: row.id, headSha: runs.headSha, checks: runs.checks, snapshot: true, required })
  } catch (err) {
    console.error(`[pr-checks] repair of ${pullRequestId} failed`, err)
  }
}

/** What `exponential_pr` returns as `checks`. A live PR whose state is not
 *  settled (none/pending) is repaired first (throttled), so a lost delivery
 *  heals on the agent's next read. */
export async function pullRequestChecksBrief(
  pullRequestId: string,
  opts: { repair?: boolean } = {}
): Promise<PrChecksBrief> {
  const load = async () => {
    const [pr] = await db
      .select({ checksState: pullRequests.checksState, state: pullRequests.state })
      .from(pullRequests)
      .where(eq(pullRequests.id, pullRequestId))
      .limit(1)
    const rows = await db.select().from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, pullRequestId))
    return { pr, rows }
  }
  let { pr, rows } = await load()
  if (
    opts.repair !== false &&
    pr &&
    (LIVE_PR_STATES as readonly string[]).includes(pr.state) &&
    (pr.checksState === `none` || pr.checksState === `pending`)
  ) {
    await repairPullRequestChecks(pullRequestId)
    ;({ pr, rows } = await load())
  }
  return checksBrief((pr?.checksState ?? `none`) as PrChecksState, rows)
}

/** The rows the merge gate reads. */
export async function loadPullRequestChecks(pullRequestId: string): Promise<CheckRow[]> {
  return db.select().from(pullRequestChecks).where(eq(pullRequestChecks.pullRequestId, pullRequestId))
}

/** Test hook. */
export function resetPrChecksForTests(): void {
  lastRepair.clear()
  requiredCache.clear()
}
