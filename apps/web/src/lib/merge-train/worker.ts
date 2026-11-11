// EXP-1709 (merge-train.json, epic #EXP-1704): THE MERGE TRAIN WORKER. It
// runs on the watchdog tick and on every lifecycle edge (a turn ending, a
// session ending, pr open/update, a checks change, a push), serialized per
// repository by an advisory lock, and applies the contract's pure half
// (`mergeStep`, `composeBatch`, `trainVerdict`, the texts and routing):
//   1. readiness + the 24 h clock for every live PR of the repo;
//   2. the repo's ONE live run: read the pipeline's report and the checks at
//      its head; green = fast-forward the base to the tested commit
//      (expected old sha; moved = recompose), close the PRs as landed and
//      apply `pr_merged`; red = re-run once, then bisect by units;
//      conflicts bounce; 3 bounces / 24 h = held;
//   3. no live run: compose the next batch and dispatch the pipeline (a
//      repo without it lands directly, serialized: lone PRs and stack
//      bottoms through the forge's own squash merge);
//   4. the queue positions (`Queued · 2nd`).
// Every outcome posts ONE waking `pr_merge` line (messages.ts). The train is
// the only merge, for every team and every repository (#EXP-1711).
import { and, asc, eq, inArray, isNotNull, ne, notInArray, sql } from "drizzle-orm"
import { requiredChecksState } from "@exp/domain-contract/pr-checks"
import {
  composeBatch,
  MERGE_TRAIN_LIMITS,
  conflictText,
  failingText,
  heldText,
  mergeStep,
  mergeWait,
  mergedText,
  noChecksVerdict,
  trainQueue,
  trainVerdict,
  type HoldReason,
  type MergeEvent,
  type MergeOutcome,
  type MergeWait,
  type MergePr,
  type PrMergeState,
  type TeamMergeMode,
  type TrainCheck,
  type TrainPipelineUnit,
  type TrainUnit,
} from "@exp/domain-contract/merge-train"
import { db, pool } from "@/db/connection"
import { mergeTrains, pullRequestChecks, pullRequests, repositories, sessions, teams } from "@/db/schema"
import { forgeFor, type Forge, type ForgeRepo } from "@/lib/ecode/forge"
import { readCommitChecks } from "@/lib/merge-train/commit-checks"
import { confirmLanding } from "@/lib/merge-train/confirm"
import {
  closedIssueIdentifiers,
  postAssistantBatch,
  postAssistantStuck,
  postPrMerge,
} from "@/lib/merge-train/messages"
import { buildUnits, decideRun, queuePositions, subtreeIdle, type PlanRun } from "@/lib/merge-train/plan"
import { LIVE_PR_STATES } from "@/lib/pr-merge-guard"
import type { Tx } from "@/lib/sessions/tx"
import { generateTxId } from "@/lib/trpc"

/** #EXP-1711: `MERGE_TRAIN_BATCH_MAX` caps a batch below merge-train.json
 *  `limits.batchMax` (8); unset or invalid = the limit. */
export const MERGE_TRAIN_BATCH_MAX_ENV = `MERGE_TRAIN_BATCH_MAX`
export function mergeTrainBatchMax(): number {
  const value = Number.parseInt(process.env[MERGE_TRAIN_BATCH_MAX_ENV] ?? ``, 10)
  return Number.isFinite(value) && value >= 1 ? Math.min(value, MERGE_TRAIN_LIMITS.batchMax) : MERGE_TRAIN_LIMITS.batchMax
}
/** A run's report must arrive within this (else the run is an error). */
export const PIPELINE_REPORT_TIMEOUT_MS = 30 * 60_000
/** A run's checks must settle within this (else the run is an error). */
export const RUN_TIMEOUT_MS = 6 * 60 * 60_000
/** #EXP-1711: a pipeline error (not a red check, not a conflict): wait. */
export const PIPELINE_ERROR_BACKOFF_MS = 10 * 60_000
/** The forge refused a fast-forward (the App off the bypass list): wait. */
export const REFUSED_BACKOFF_MS = 30 * 60_000

const LIVE_RUN_STATES = [`dispatching`, `testing`] as const
const PASSING = new Set([`success`, `neutral`, `skipped`])

type PrRow = typeof pullRequests.$inferSelect
type RunRow = typeof mergeTrains.$inferSelect
type RepoRow = Pick<
  typeof repositories.$inferSelect,
  `id` | `teamId` | `fullName` | `installationId` | `defaultBranch` | `defaultBranchOverride`
>

const backoff = new Map<string, number>()

function toMergePr(pr: PrRow): MergePr {
  return {
    state: pr.mergeState as PrMergeState,
    attempts: pr.mergeAttempts,
    queuedSince: pr.mergeQueuedSince?.toISOString() ?? null,
    holdReason: (pr.mergeHold as HoldReason | null) ?? null,
    mergeNow: pr.mergeNow,
  }
}

function forgeRepo(repo: RepoRow): ForgeRepo {
  return { id: repo.id, fullName: repo.fullName, installationId: repo.installationId }
}

// ── one PR edge ──────────────────────────────────────────────────────────

export interface StepContext {
  base: string
  now: Date
  files?: string[]
  failing?: Array<{ name: string; detailsUrl?: string | null }>
  /** Extra columns written with the step (merge_wait, the run id, …). */
  extra?: Partial<Pick<PrRow, `mergeWait` | `mergeTrainId` | `mergeHeadSha` | `mergePosition`>>
}

export interface StepOutcome {
  post: MergeOutcome | null
  /** The starter's Assistant the PR's route reached (merged: the caller's
   *  batch line; held by the limits: already posted). */
  assistant: string | null
  state: PrMergeState
}

/** Apply ONE event to a PR inside `tx` (row locked by the caller or here),
 *  persist what changed and post its `pr_merge` line. */
export async function stepPullRequestInTx(
  tx: Tx,
  prId: string,
  event: MergeEvent,
  ctx: StepContext
): Promise<StepOutcome | null> {
  const [pr] = await tx.select().from(pullRequests).where(eq(pullRequests.id, prId)).limit(1).for(`update`)
  if (!pr) return null
  const before = toMergePr(pr)
  const { pr: after, post } = mergeStep(before, event)
  const leftTesting = before.state === `testing` && after.state !== `testing`
  const set: Partial<PrRow> = {}
  if (after.state !== before.state) set.mergeState = after.state
  if (after.attempts !== before.attempts) set.mergeAttempts = after.attempts
  if (after.holdReason !== before.holdReason) set.mergeHold = after.holdReason
  if (after.queuedSince !== before.queuedSince) set.mergeQueuedSince = after.queuedSince ? new Date(after.queuedSince) : null
  if (after.mergeNow !== before.mergeNow) set.mergeNow = after.mergeNow
  if (after.state === `queued` && before.state !== `queued` && before.state !== `testing`) set.mergeReadyAt = ctx.now
  if (after.state !== `queued`) set.mergePosition = null
  if (leftTesting) set.mergeTrainId = null
  // A bounce is a train verdict, not a readiness wait: the pill reads the state.
  if (post === `conflict` || post === `failing`) set.mergeWait = null
  if (after.state === `held` || after.state === `merged`) set.mergeWait = null
  for (const [key, value] of Object.entries(ctx.extra ?? {})) {
    if ((pr as Record<string, unknown>)[key] !== value) (set as Record<string, unknown>)[key] = value
  }
  if (Object.keys(set).length > 0) {
    await generateTxId(tx)
    await tx.update(pullRequests).set(set).where(eq(pullRequests.id, pr.id))
  }
  let assistant: string | null = null
  if (post) {
    const text =
      post === `merged`
        ? mergedText([pr.number], ctx.base)
        : post === `conflict`
          ? conflictText(pr.number, ctx.base, ctx.files ?? [], after.attempts)
          : post === `failing`
            ? failingText(pr.number, ctx.failing ?? [], after.attempts)
            : heldText(pr.number, after.holdReason ?? `person`)
    const routed = await postPrMerge(tx, pr, post, text, after.holdReason)
    assistant = routed.assistant
    if (post === `held` && assistant && after.holdReason && after.holdReason !== `person`) {
      await postAssistantStuck(tx, assistant, pr, after.holdReason)
    }
  }
  return { post, assistant, state: after.state }
}

function stepPullRequest(prId: string, event: MergeEvent, ctx: StepContext): Promise<StepOutcome | null> {
  return db.transaction((tx) => stepPullRequestInTx(tx, prId, event, ctx))
}

// ── readiness ────────────────────────────────────────────────────────────

type ReadinessInputs = Awaited<ReturnType<typeof readinessInputs>>

async function readinessInputs(prs: PrRow[], repo: RepoRow, forge: Forge) {
  const roots = [...new Set(prs.flatMap((pr) => (pr.rootSessionId ? [pr.rootSessionId] : [])))]
  const tree =
    roots.length === 0
      ? []
      : await db
          .select({ id: sessions.id, parentId: sessions.parentId, status: sessions.status, busy: sessions.busy, rootId: sessions.rootId })
          .from(sessions)
          .where(inArray(sessions.rootId, roots))
  const modes = new Map<string, TeamMergeMode>()
  for (const teamId of new Set(prs.map((pr) => pr.teamId))) {
    const [team] = await db.select({ mode: teams.mergeMode }).from(teams).where(eq(teams.id, teamId)).limit(1)
    modes.set(teamId, (team?.mode as TeamMergeMode | undefined) ?? `auto`)
  }
  // #FEED-221: the base's required names, per base; unreadable = none known
  // (the rows' flags decide, and nothing is held for missing CI).
  const required = new Map<string, string[]>()
  for (const base of new Set(prs.map((pr) => pr.base))) {
    required.set(base, await forge.requiredChecks(forgeRepo(repo), base).catch(() => []))
  }
  // #EXP-1711: readiness reads the GATED checks (required when the base
  // names any), never an informational leg.
  const ids = prs.map((pr) => pr.id)
  const rows =
    ids.length === 0
      ? []
      : await db
          .select({
            pullRequestId: pullRequestChecks.pullRequestId,
            name: pullRequestChecks.name,
            status: pullRequestChecks.status,
            conclusion: pullRequestChecks.conclusion,
            required: pullRequestChecks.required,
          })
          .from(pullRequestChecks)
          .where(inArray(pullRequestChecks.pullRequestId, ids))
  const checks = new Map<string, typeof rows>()
  for (const row of rows) checks.set(row.pullRequestId, [...(checks.get(row.pullRequestId) ?? []), row])
  return { tree, modes, checks, required }
}

function waitOf(pr: PrRow, { tree, modes, checks, required }: ReadinessInputs): MergeWait | null {
  const rows = checks.get(pr.id) ?? []
  const names = required.get(pr.base) ?? []
  return mergeWait({
    draft: pr.state === `draft`,
    checksState: rows.length > 0 || names.length > 0 ? requiredChecksState(rows, names) : pr.checksState,
    subtreeIdle: subtreeIdle(
      pr.sessionId,
      tree.filter((s) => s.rootId === pr.rootSessionId)
    ),
    mode: modes.get(pr.teamId) ?? `auto`,
    approved: pr.mergeApprovedAt !== null,
  })
}

/** #FEED-221 (merge-train.json `noChecks`): a PR with no check runs at its
 *  head where the base expects some. → the hold it takes, else null. */
async function noChecksHold(
  repo: RepoRow,
  pr: PrRow,
  inputs: ReadinessInputs,
  forge: Forge,
  now: Date
): Promise<`no_checks` | `conflict` | null> {
  if ((inputs.checks.get(pr.id)?.length ?? 0) > 0 || pr.state !== `open` || pr.mergeNow) return null
  const expected = (inputs.required.get(pr.base)?.length ?? 0) > 0
  if (!expected) return null
  const live = await forge.readPullRequest(forgeRepo(repo), pr.number).catch(() => null)
  if (!live) return null
  const since = live.updatedAt ?? pr.createdAt
  const verdict = noChecksVerdict({ expected, mergeable: live.mergeable ?? null, ageMs: now.getTime() - since.getTime() })
  return verdict === `no_checks` || verdict === `conflict` ? verdict : null
}

async function readiness(repo: RepoRow, prs: PrRow[], forge: Forge, now: Date): Promise<void> {
  const inputs = await readinessInputs(prs, repo, forge)
  const at = now.toISOString()
  for (const pr of prs) {
    const base = pr.base
    const state = pr.mergeState as PrMergeState
    if (state === `conflict` || state === `failing`) {
      if (pr.mergeWait === null) {
        // A train bounce re-queues only once the head moved (a push the
        // webhook may have missed: a polling instance gets none).
        if (pr.mergeHeadSha) {
          const live = await forge.readPullRequest(forgeRepo(repo), pr.number).catch(() => null)
          if (live?.headSha && live.headSha !== pr.mergeHeadSha) {
            await stepPullRequest(pr.id, { type: `push` }, { base, now })
            pr.mergeState = `checks`
          }
        }
        // #EXP-1711: a conflict the train never tested (no head it ran: a
        // row from before the train, when the forge's `mergeable` set it) is no bounce:
        // readiness re-reads it, so the train composes it and either lands
        // it or bounces it with its files to the agent.
        const untested = state === `conflict` && !pr.mergeHeadSha
        if (pr.mergeState !== `checks` && !untested) {
          await stepPullRequest(pr.id, { type: `tick`, now: at }, { base, now })
          continue
        }
      }
    }
    // #FEED-221: CI showed up on a head held for having none.
    if (pr.mergeState === `held` && (inputs.checks.get(pr.id)?.length ?? 0) > 0) {
      const released = await stepPullRequest(pr.id, { type: `checks_appeared` }, { base, now })
      if (released) pr.mergeState = released.state
    }
    if (pr.mergeState !== `testing` && pr.mergeState !== `held`) {
      const hold = pr.mergeState === `draft` ? null : await noChecksHold(repo, pr, inputs, forge, now)
      if (hold) {
        await stepPullRequest(pr.id, { type: `forge_hold`, reason: hold }, { base, now })
        continue
      }
      const wait = waitOf(pr, inputs)
      // A queued PR stays queued through its opener's work: no `session` pill.
      const shown = pr.mergeState === `queued` && wait === `session` ? null : wait
      await stepPullRequest(pr.id, { type: `readiness`, wait, at }, { base, now, extra: { mergeWait: shown } })
    }
    await stepPullRequest(pr.id, { type: `tick`, now: at }, { base, now })
  }
}

// ── runs ─────────────────────────────────────────────────────────────────

async function loadPrs(ids: string[]): Promise<Map<string, PrRow>> {
  if (ids.length === 0) return new Map()
  const rows = await db.select().from(pullRequests).where(inArray(pullRequests.id, ids))
  return new Map(rows.map((row) => [row.id, row]))
}

async function finishRun(run: RunRow, state: string, extra: Partial<RunRow> = {}): Promise<void> {
  await db
    .update(mergeTrains)
    .set({ state, finishedAt: new Date(), ...extra })
    .where(eq(mergeTrains.id, run.id))
}

async function requeue(ids: string[], base: string, now: Date): Promise<void> {
  for (const id of ids) await stepPullRequest(id, { type: `train_requeued` }, { base, now })
}

/** #FEED-212: a landing the forge refused or never confirmed: its PRs back to
 *  the queue with the attempt counted (never closed, never merged), the rest
 *  of the run re-queued, the run an error. */
async function unland(run: RunRow, ids: string[], rest: string[], now: Date, error: string): Promise<void> {
  for (const id of ids) await stepPullRequest(id, { type: `train_unlanded` }, { base: run.base, now })
  await requeue(rest.filter((id) => !ids.includes(id)), run.base, now)
  await finishRun(run, `error`, { error })
}

/** Units whose members a push, a hold or a close pulled out of `runId`. */
function pulled(units: string[][], prs: Map<string, PrRow>, runId: string | null): boolean {
  return units.some((unit) =>
    unit.some((id) => {
      const pr = prs.get(id)
      if (!pr || pr.mergeState !== `testing` || !(LIVE_PR_STATES as readonly string[]).includes(pr.state)) return true
      return runId !== null && pr.mergeTrainId !== runId
    })
  )
}

/** The required checks at the train commit that make it red. */
function failingAt(checks: TrainCheck[], required: string[]): Array<{ name: string; detailsUrl: string | null }> {
  const latest = new Map(checks.map((c) => [c.name, c]))
  return required.flatMap((name) => {
    const c = latest.get(name)
    return c && c.status === `completed` && !PASSING.has(c.conclusion ?? ``) ? [{ name, detailsUrl: null }] : []
  })
}

async function pollRun(repo: RepoRow, run: RunRow, forge: Forge, now: Date): Promise<boolean> {
  const fr = forgeRepo(repo)
  const units = run.units
  const pendingIds = run.pending.flat(2)
  const prs = await loadPrs([...units.flat(), ...pendingIds])
  if (pulled(units, prs, run.id)) {
    // merge-train.json: a pushed or held unit is pulled from its run; the
    // worker recomposes without it.
    const still = [...units.flat(), ...pendingIds].filter((id) => prs.get(id)?.mergeState === `testing`)
    await requeue(still, run.base, now)
    await finishRun(run, `cancelled`, { error: `a unit left the run (push, hold or close)` })
    return false
  }
  const report = await forge.readPipeline(fr, run.id, run.runRef)
  if (!report.outcome) {
    if (report.finished || now.getTime() - run.createdAt.getTime() > PIPELINE_REPORT_TIMEOUT_MS) {
      await requeue([...units.flat(), ...pendingIds], run.base, now)
      await finishRun(run, `error`, { error: report.finished ? `the pipeline finished without a report` : `no report in time` })
      return false
    }
    return true
  }
  const outcome = report.outcome
  if (outcome.head && run.testedSha !== outcome.head) {
    await db.update(mergeTrains).set({ testedSha: outcome.head, state: `testing` }).where(eq(mergeTrains.id, run.id))
  }
  const dispatched = outcome.checks === `dispatched` && !!outcome.head
  const checks = dispatched ? ((await readCommitChecks(forge, fr, outcome.head!)) ?? []) : []
  // #FEED-221: unreadable is no verdict (an empty list would read as green).
  const required = dispatched ? await forge.requiredChecks(fr, run.base).catch(() => null) : []
  if (!required) return true
  const verdict = trainVerdict({ outcome, checks, required })
  if (verdict.state === `pending`) {
    if (now.getTime() - run.createdAt.getTime() <= RUN_TIMEOUT_MS) return true
    verdict.state = `error`
  }
  const byNumber = new Map([...prs.values()].map((pr) => [pr.number, pr.id]))
  const unitOf = (numbers: number[]): string[] | null => {
    const ids = numbers.map((n) => byNumber.get(n))
    if (ids.some((id) => !id)) return null
    return units.find((unit) => unit.length === ids.length && unit.every((id, i) => id === ids[i])) ?? null
  }
  const plan: PlanRun<string> = { units, attempt: run.attempt, bisect: run.bisectOf !== null, pending: run.pending }
  const decision = decideRun(plan, verdict, unitOf)
  if (!decision) return true
  const conflicts: Record<string, string[]> = {}
  for (const c of decision.conflicts) {
    for (const id of c.unit) {
      conflicts[String(prs.get(id)?.number ?? id)] = c.files
      // #EXP-1711: only an unlanded sibling of this batch is in the way:
      // back to the queue, no bounce, no post; it composes after it lands.
      if (c.against === `batch`) await requeue([id], run.base, now)
      else await stepPullRequest(id, { type: `train_conflict` }, { base: run.base, now, files: c.files })
    }
  }
  const failing = decision.failing.length > 0 ? failingAt(checks, required) : []
  for (const unit of decision.failing) {
    for (const id of unit) await stepPullRequest(id, { type: `train_failing` }, { base: run.base, now, failing })
  }
  await requeue(decision.requeue.flat(), run.base, now)
  if (decision.land.length > 0) {
    const landed = await land(repo, run, decision.land, outcome.head!, forge, now)
    if (!landed) return false
  }
  const state = verdict.state === `green` ? `landed` : verdict.state
  // #EXP-1711: a pipeline error (infra: a fetch, a resolver) keeps its
  // message and backs the repository off, so a broken compose never re-runs
  // every pass.
  if (state === `error`) {
    console.error(`[merge-train] ${repo.fullName}: pipeline error: ${outcome.error ?? `the run timed out`}`)
    backoff.set(repo.id, now.getTime() + PIPELINE_ERROR_BACKOFF_MS)
  }
  await finishRun(run, state, {
    conflicts: Object.keys(conflicts).length > 0 ? conflicts : null,
    ...(state === `error` ? { error: (outcome.error ?? `the run timed out`).slice(0, 2000) } : {}),
  })
  if (decision.next) {
    await startRun(repo, forge, decision.next, { base: run.base, bisectOf: decision.next.bisect ? run.id : null, now })
    return true
  }
  return false
}

/** Fast-forward the base to the tested commit, then close and apply every
 *  landed PR. False = not landed (moved: recompose; refused: back off). */
async function land(
  repo: RepoRow,
  run: RunRow,
  units: string[][],
  head: string,
  forge: Forge,
  now: Date
): Promise<boolean> {
  const fr = forgeRepo(repo)
  const ids = units.flat()
  const rest = [...run.units.flat(), ...run.pending.flat(2)]
  // A head pushed since the run composed it never lands from it.
  for (const id of ids) {
    const pr = (await loadPrs([id])).get(id)
    const live = pr ? await forge.readPullRequest(fr, pr.number).catch(() => null) : null
    if (!pr || !live || live.state !== `open` || live.headSha !== run.heads[id]) {
      if (pr) await stepPullRequest(id, { type: `push` }, { base: run.base, now })
      await requeue(rest.filter((other) => other !== id), run.base, now)
      await finishRun(run, `cancelled`, { error: `#${pr?.number ?? `?`} moved before landing` })
      return false
    }
  }
  if (!run.baseSha) return false
  // #EXP-1711: a last check: the base landed while this run tested. The
  // next pass moves the units onto the live base (`repairStranded`).
  if ((await landedBranches(repo, [run.base])).length > 0) {
    await requeue(rest, run.base, now)
    await finishRun(run, `cancelled`, { error: `${run.base} already landed: nothing lands onto it` })
    return false
  }
  // #FEED-212: a head equal to the base composed nothing (an empty squash):
  // fast-forwarding onto itself "succeeds" and lands nothing.
  if (head === run.baseSha) {
    console.error(`[merge-train] ${repo.fullName}: run ${run.id} composed nothing onto ${run.base}`)
    await unland(run, ids, rest, now, `the train head is ${run.base} itself: nothing to land`)
    return false
  }
  const ff = await forge.fastForward(fr, { branch: run.base, from: run.baseSha, to: head })
  if (!ff.ok) {
    if (ff.reason === `moved`) {
      await requeue(rest, run.base, now)
      await finishRun(run, `cancelled`, { error: `${run.base} moved: recompose` })
      return false
    }
    backoff.set(repo.id, now.getTime() + REFUSED_BACKOFF_MS)
    console.error(`[merge-train] ${repo.fullName}: fast-forward of ${run.base} refused: ${ff.message}`)
    await unland(run, ids, rest, now, ff.message)
    return false
  }
  // #FEED-212: reported only once the base holds the landed commit.
  if (!(await confirmLanding(forge, fr, { base: run.base, from: run.baseSha, landed: head }))) {
    console.error(`[merge-train] ${repo.fullName}: ${run.base} does not hold ${head} after the fast-forward`)
    await unland(run, ids, rest, now, `${run.base} does not hold ${head} after the fast-forward`)
    return false
  }
  await landPullRequests(repo, ids, run.base, head, forge, now)
  return true
}

/** The landing of PRs whose commits are on `base` as `sha`: pr_merged (the
 *  issues' merge automation via applyLandedInTx), the PR's merged line, the
 *  forge's Closed with a comment, then ONE Assistant line for the batch. */
async function landPullRequests(
  repo: RepoRow,
  ids: string[],
  base: string,
  sha: string | null,
  forge: Forge,
  now: Date,
  closeOnForge = true
): Promise<void> {
  const { applyPullRequestMerged } = await import(`@/lib/integrations/pr-sync`)
  const batch: Array<{ id: string; number: number; assistant: string | null; issues: string[] }> = []
  // Every PR is merged in Exponential BEFORE the forge closes any: a
  // stack's close (or its branch going) must find the rows above merged.
  for (const id of ids) {
    await applyPullRequestMerged({ pullRequestId: id, mergedAt: now, base, actorUserId: null, trainLanding: true })
    const result = await db.transaction(async (tx) => {
      const step = await stepPullRequestInTx(tx, id, { type: `train_landed` }, { base, now })
      const [pr] = await tx.select({ number: pullRequests.number }).from(pullRequests).where(eq(pullRequests.id, id)).limit(1)
      return { step, number: pr?.number ?? 0, issues: await closedIssueIdentifiers(tx, id) }
    })
    batch.push({ id, number: result.number, assistant: result.step?.assistant ?? null, issues: result.issues })
  }
  if (closeOnForge && sha) {
    for (const pr of batch) {
      await forge.closeAsLanded(forgeRepo(repo), { number: pr.number, sha, base }).catch((err) => {
        console.error(`[merge-train] closing ${repo.fullName}#${pr.number} as landed failed`, err)
      })
    }
  }
  await db.transaction((tx) => postAssistantBatch(tx, batch, base))
}

/** The required checks of a PR's head all green (a lone unit's tested_sha). */
async function provenHead(pr: PrRow, head: string): Promise<boolean> {
  const rows = await db
    .select({ required: pullRequestChecks.required, status: pullRequestChecks.status, conclusion: pullRequestChecks.conclusion, headSha: pullRequestChecks.headSha })
    .from(pullRequestChecks)
    .where(eq(pullRequestChecks.pullRequestId, pr.id))
  const required = rows.filter((r) => r.required && r.headSha === head)
  return required.length > 0 && required.every((r) => r.status === `completed` && PASSING.has(r.conclusion ?? ``))
}

async function startRun(
  repo: RepoRow,
  forge: Forge,
  plan: PlanRun<string>,
  opts: { base: string; bisectOf: string | null; now: Date; mergeNow?: boolean }
): Promise<void> {
  const fr = forgeRepo(repo)
  const { now, base } = opts
  const prs = await loadPrs([...plan.units.flat(), ...plan.pending.flat(2)])
  const units: string[][] = []
  const pipelineUnits: TrainPipelineUnit[] = []
  const heads: Record<string, string> = {}
  for (const unit of plan.units) {
    const members = unit.map((id) => prs.get(id))
    const ok = members.every(
      (pr) => pr && (pr.mergeState === `queued` || pr.mergeState === `testing`) && pr.state === `open`
    )
    const reads = ok ? await Promise.all(members.map((pr) => forge.readPullRequest(fr, pr!.number).catch(() => null))) : []
    if (!ok || reads.some((r) => !r || r.state !== `open` || r.draft || !r.headSha)) {
      // Not ready any more (a push, a hold, a close): back to readiness.
      await requeue(unit.filter((id) => prs.get(id)?.mergeState === `testing`), base, now)
      continue
    }
    units.push(unit)
    const pipelinePrs = members.map((pr, i) => {
      heads[pr!.id] = reads[i]!.headSha!
      return {
        number: pr!.number,
        head_sha: reads[i]!.headSha!,
        title: reads[i]!.title || pr!.title || `#${pr!.number}`,
        ...(reads[i]!.author ? { author: reads[i]!.author! } : {}),
      }
    })
    const lone = plan.units.length === 1 && members.length === 1
    const tested = lone && (await provenHead(members[0]!, pipelinePrs[0]!.head_sha)) ? pipelinePrs[0]!.head_sha : null
    pipelineUnits.push({ prs: pipelinePrs, ...(tested ? { tested_sha: tested } : {}) })
  }
  if (units.length === 0) {
    const next = plan.pending.length > 0 ? { units: plan.pending[0]!, attempt: 1, bisect: true, pending: plan.pending.slice(1) } : null
    if (next) await startRun(repo, forge, next, opts)
    return
  }
  const baseSha = await forge.readBranch(fr, base)
  if (!baseSha) {
    await requeue([...units.flat(), ...plan.pending.flat(2)], base, now)
    return
  }
  const untested = opts.mergeNow === true
  const [run] = await db
    .insert(mergeTrains)
    .values({
      teamId: repo.teamId,
      repositoryId: repo.id,
      base,
      baseSha,
      units,
      attempt: plan.attempt,
      bisectOf: opts.bisectOf,
      pending: plan.pending,
      heads,
      untested,
    })
    .returning()
  for (const id of units.flat()) {
    await stepPullRequest(id, { type: `train_start` }, { base, now, extra: { mergeTrainId: run!.id, mergeHeadSha: heads[id] } })
  }
  const dispatch = await forge.dispatchPipeline(fr, {
    train_id: run!.id,
    base,
    base_sha: baseSha,
    mode: untested ? `untested` : `test`,
    units: JSON.stringify(pipelineUnits),
  })
  if (dispatch.ok) {
    await db.update(mergeTrains).set({ state: `testing`, runRef: dispatch.runRef }).where(eq(mergeTrains.id, run!.id))
    return
  }
  if (dispatch.missing) {
    await landDirect(repo, forge, run!, units, base, now)
    return
  }
  console.error(`[merge-train] ${repo.fullName}: dispatch failed: ${dispatch.message}`)
  backoff.set(repo.id, now.getTime() + 5 * 60_000)
  await requeue([...units.flat(), ...plan.pending.flat(2)], base, now)
  await finishRun(run!, `error`, { error: dispatch.message })
}

/** No pipeline in the repo (self-hosted without CI, a repo not set up):
 *  serialized, untested, unresolved. Only a unit's bottom lands per pass
 *  (the forge retargets the PR above onto the base after it). */
async function landDirect(
  repo: RepoRow,
  forge: Forge,
  run: RunRow,
  units: string[][],
  base: string,
  now: Date
): Promise<void> {
  const fr = forgeRepo(repo)
  // #FEED-207: the direct path re-checks too: nothing ever merges onto a
  // landed branch (the next pass moves the units onto the live base).
  if ((await landedBranches(repo, [base])).length > 0) {
    await requeue(units.flat(), base, now)
    await finishRun(run, `cancelled`, { untested: true, error: `${base} already landed: nothing lands onto it` })
    return
  }
  const prs = await loadPrs(units.flat())
  const landed: string[] = []
  for (const unit of units) {
    const [bottom, ...above] = unit
    await requeue(above, base, now)
    const pr = prs.get(bottom!)!
    const read = await forge.readPullRequest(fr, pr.number).catch(() => null)
    if (!read || read.baseRef !== base) {
      await requeue([bottom!], base, now)
      continue
    }
    const merged = await forge.squashMerge(fr, {
      number: pr.number,
      title: read.title || pr.title || `#${pr.number}`,
      expectedHead: run.heads[bottom!] ?? null,
    })
    if (merged.ok && (await confirmLanding(forge, fr, { base, from: run.baseSha, landed: merged.sha }))) {
      await landPullRequests(repo, [bottom!], base, merged.sha, forge, now, false)
      landed.push(bottom!)
    } else if (merged.ok) {
      // #FEED-212: the forge said merged but the base does not hold it.
      console.error(`[merge-train] ${repo.fullName}: #${pr.number}'s merge ${merged.sha ?? `(no sha)`} is not on ${base}`)
      await stepPullRequest(bottom!, { type: `train_unlanded` }, { base, now })
    } else if (merged.conflict) {
      await stepPullRequest(bottom!, { type: `train_conflict` }, { base, now, files: [merged.message] })
    } else {
      await requeue([bottom!], base, now)
    }
  }
  await finishRun(run, landed.length > 0 ? `landed` : `conflict`, { untested: true, error: `no pipeline: landed directly` })
}

// ── one repository pass ──────────────────────────────────────────────────

function unitsOf(
  prs: PrRow[],
  eligible: (pr: PrRow) => boolean = (pr) => pr.mergeState === `queued`
): { units: PrRow[][]; train: TrainUnit[] } {
  const units = buildUnits(prs)
  const numberOf = new Map(prs.map((pr) => [pr.id, pr.number]))
  const train = units.map((unit) => ({
    members: unit.map((pr) => {
      // #EXP-1711: a tree-shaped stack's ready prefix follows parents.
      const parent = pr.basePullRequestId ? numberOf.get(pr.basePullRequestId) : undefined
      return {
        number: pr.number,
        ready: eligible(pr),
        readyAt: pr.mergeReadyAt?.toISOString() ?? null,
        ...(parent !== undefined ? { parent } : {}),
      }
    }),
    mergeNow: unit.some((pr) => pr.mergeNow),
  }))
  return { units, train }
}

/** #EXP-1711: which of `branches` are the head of a PR that already merged
 *  (never the repo's own branch: a promotion's head stays alive). */
async function landedBranches(repo: RepoRow, branches: string[]): Promise<Array<{ id: string; head: string }>> {
  const target = repo.defaultBranchOverride ?? repo.defaultBranch
  const names = branches.filter((name) => name !== target)
  if (names.length === 0) return []
  return db
    .select({ id: pullRequests.id, head: pullRequests.head })
    .from(pullRequests)
    .where(and(eq(pullRequests.repositoryId, repo.id), eq(pullRequests.state, `merged`), inArray(pullRequests.head, names)))
}

/** #EXP-1711 (merge-train.json CHILDREN): a live PR whose parent already
 *  landed and that still sits on the parent's branch (a move that failed, a
 *  stack from before the train). Moved onto the base here, and it rides no
 *  batch meanwhile: nothing ever lands onto a landed branch. → their ids. */
async function repairStranded(repo: RepoRow, prs: PrRow[]): Promise<Set<string>> {
  const live = new Set(prs.map((pr) => pr.id))
  const bottoms = prs.filter((pr) => !(pr.basePullRequestId && live.has(pr.basePullRequestId)))
  const landed = await landedBranches(repo, [...new Set(bottoms.map((pr) => pr.base))])
  if (landed.length === 0) return new Set()
  const { retargetChildrenOfMergedPr } = await import(`@/lib/integrations/pr-sync`)
  for (const { id, head } of new Map(landed.map((row) => [row.head, row])).values()) {
    await retargetChildrenOfMergedPr(id, { trainLanding: true }).catch((err) => {
      console.error(`[merge-train] ${repo.fullName}: moving the PRs on ${head} failed`, err)
    })
  }
  const heads = new Set(landed.map((row) => row.head))
  return new Set(bottoms.filter((pr) => heads.has(pr.base)).map((pr) => pr.id))
}

async function livePrs(repositoryId: string): Promise<PrRow[]> {
  return db
    .select()
    .from(pullRequests)
    .where(
      and(
        eq(pullRequests.repositoryId, repositoryId),
        inArray(pullRequests.state, [...LIVE_PR_STATES]),
        ne(pullRequests.mergeState, `merged`)
      )
    )
    .orderBy(asc(pullRequests.createdAt))
}

/** One pass over a repository (the caller holds its lock). */
export async function advanceRepository(repositoryId: string, now: Date = new Date()): Promise<void> {
  const [repo] = await db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      fullName: repositories.fullName,
      installationId: repositories.installationId,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  if (!repo) return
  const forge = await forgeFor(forgeRepo(repo))
  await readiness(repo, await livePrs(repo.id), forge, now)

  const [live] = await db
    .select()
    .from(mergeTrains)
    .where(and(eq(mergeTrains.repositoryId, repo.id), inArray(mergeTrains.state, [...LIVE_RUN_STATES])))
    .orderBy(asc(mergeTrains.createdAt))
    .limit(1)
  let busy = live ? await pollRun(repo, live, forge, now) : false

  const until = backoff.get(repo.id)
  if (!busy && (until === undefined || until <= now.getTime())) {
    backoff.delete(repo.id)
    const prs = await livePrs(repo.id)
    const stranded = await repairStranded(repo, prs)
    const all = unitsOf(prs)
    const rides = all.units.map((unit) => !stranded.has(unit[0]!.id))
    const units = all.units.filter((_, i) => rides[i])
    const train = all.train.filter((_, i) => rides[i])
    const queue = trainQueue(train)
    const first = queue[0]
    if (first) {
      const byNumber = new Map(prs.map((pr) => [pr.number, pr]))
      const bottom = byNumber.get(first.members[0]!.number)!
      const base = bottom.base
      const sameBase = train.filter((_, i) => units[i]![0]!.base === base)
      const batch = composeBatch(sameBase, mergeTrainBatchMax())
      if (batch.length > 0) {
        const ids = batch.map((unit) => unit.map((n) => byNumber.get(n)!.id))
        await startRun(repo, forge, { units: ids, attempt: 0, bisect: false, pending: [] }, {
          base,
          bisectOf: null,
          now,
          mergeNow: first.mergeNow === true,
        })
        busy = true
      }
    }
  }

  // Queue positions (the pill's `Queued · 2nd`), written only when changed.
  const prs = await livePrs(repo.id)
  // #FEED-221: the live run's units hold the first places, so a re-queued
  // batch never pushes the PRs behind it back.
  const { train } = unitsOf(prs, (pr) => pr.mergeState === `queued` || pr.mergeState === `testing`)
  const byNumber = new Map(prs.map((pr) => [pr.number, pr]))
  const queue = trainQueue(train).map((unit) => unit.members.map((m) => byNumber.get(m.number)!))
  const testing = (unit: PrRow[]) => unit.some((pr) => pr.mergeState === `testing`)
  const positions = queuePositions([...queue.filter(testing), ...queue.filter((unit) => !testing(unit))].map((unit) => unit.map((pr) => pr.id)))
  for (const pr of prs) {
    const position = pr.mergeState === `queued` ? (positions.get(pr.id) ?? null) : null
    if (pr.mergePosition !== position) {
      await db.update(pullRequests).set({ mergePosition: position }).where(eq(pullRequests.id, pr.id))
    }
  }
  // A PR closed in the queue keeps no place.
  await db
    .update(pullRequests)
    .set({ mergePosition: null })
    .where(
      and(
        eq(pullRequests.repositoryId, repo.id),
        notInArray(pullRequests.state, [...LIVE_PR_STATES]),
        isNotNull(pullRequests.mergePosition)
      )
    )
}

// ── serialization, edges, the tick ───────────────────────────────────────

const inFlight = new Map<string, { run: Promise<void>; dirty: boolean }>()

/** One pass per repository at a time: in process (a late edge marks the
 *  running pass dirty and it runs once more) and across instances (a
 *  session advisory lock; another instance's pass = skipped, the next edge
 *  or tick catches up). Never throws. */
export function runRepository(repositoryId: string, now?: Date): Promise<void> {
  const running = inFlight.get(repositoryId)
  if (running) {
    running.dirty = true
    return running.run
  }
  const entry = { dirty: false, run: Promise.resolve() }
  entry.run = (async () => {
    do {
      entry.dirty = false
      await locked(repositoryId, () => advanceRepository(repositoryId, now ?? new Date())).catch((err) =>
        console.error(`[merge-train] repository ${repositoryId} pass failed`, err)
      )
    } while (entry.dirty)
  })().finally(() => inFlight.delete(repositoryId))
  inFlight.set(repositoryId, entry)
  return entry.run
}

async function locked(repositoryId: string, fn: () => Promise<void>): Promise<void> {
  const client = await pool.connect()
  const key = `merge_train:${repositoryId}`
  let broken: Error | undefined
  try {
    const { rows } = await client.query<{ locked: boolean }>(`SELECT pg_try_advisory_lock(hashtext($1)) AS locked`, [key])
    if (!rows[0]?.locked) return
    try {
      await fn()
    } finally {
      try {
        await client.query(`SELECT pg_advisory_unlock(hashtext($1))`, [key])
      } catch (err) {
        // Back in the pool it would hold the lock forever: discard it.
        broken = err instanceof Error ? err : new Error(String(err))
      }
    }
  } finally {
    client.release(broken)
  }
}

const pending = new Set<Promise<void>>()

export type EdgeScope = { rootId: string } | { pullRequestId: string } | { repositoryId: string } | { repoFullName: string }

async function repositoriesOf(scope: EdgeScope): Promise<string[]> {
  if (`repositoryId` in scope) return [scope.repositoryId]
  if (`repoFullName` in scope) {
    const rows = await db.select({ id: repositories.id }).from(repositories).where(eq(repositories.fullName, scope.repoFullName))
    return rows.map((r) => r.id)
  }
  const where =
    `rootId` in scope ? eq(pullRequests.rootSessionId, scope.rootId) : eq(pullRequests.id, scope.pullRequestId)
  const rows = await db.selectDistinct({ id: pullRequests.repositoryId }).from(pullRequests).where(where)
  return rows.map((r) => r.id)
}

/** A lifecycle edge (fire and forget, after its write committed); the
 *  switch off = the display pass. */
export function mergeTrainEdge(scope: EdgeScope | null | undefined): void {
  if (!scope) return
  const edge: Promise<void> = repositoriesOf(scope)
    .then((ids) => Promise.all(ids.map((id) => runRepository(id))))
    .then(() => {})
    .catch((err) => console.error(`[merge-train] edge failed`, err))
    .finally(() => pending.delete(edge))
  pending.add(edge)
}

/** Tests: resolves once every fired edge finished. */
export async function drainMergeTrainEdges(): Promise<void> {
  while (pending.size > 0) await Promise.all([...pending])
}

/** The watchdog tick: every repository with a live PR in the train's hands
 *  or a live run. → how many repositories were passed. */
export async function runMergeTrainTick(now: Date = new Date()): Promise<number> {
  const rows = await db
    .selectDistinct({ id: pullRequests.repositoryId })
    .from(pullRequests)
    .where(and(inArray(pullRequests.state, [...LIVE_PR_STATES]), ne(pullRequests.mergeState, `merged`)))
  const runs = await db
    .selectDistinct({ id: mergeTrains.repositoryId })
    .from(mergeTrains)
    .where(inArray(mergeTrains.state, [...LIVE_RUN_STATES]))
  const ids = [...new Set([...rows, ...runs].map((r) => r.id))]
  for (const id of ids) await runRepository(id, now)
  return ids.length
}

// ── a person's and an agent's controls ───────────────────────────────────

async function personEvent(pullRequestId: string, event: MergeEvent): Promise<StepOutcome | null> {
  const [pr] = await db.select({ base: pullRequests.base }).from(pullRequests).where(eq(pullRequests.id, pullRequestId)).limit(1)
  if (!pr) return null
  const outcome = await stepPullRequest(pullRequestId, event, { base: pr.base, now: new Date() })
  mergeTrainEdge({ pullRequestId })
  return outcome
}

/** A new head (the forge's push): checks again; a testing unit leaves its run. */
export function onPullRequestPush(pullRequestId: string): Promise<StepOutcome | null> {
  return personEvent(pullRequestId, { type: `push` })
}
/** A person's Hold (never a Needs you row). */
export function holdPullRequest(pullRequestId: string): Promise<StepOutcome | null> {
  return personEvent(pullRequestId, { type: `hold` })
}
/** Resume auto-merge / Needs you Retry: checks again, bounces and clock reset. */
export function resumePullRequest(pullRequestId: string): Promise<StepOutcome | null> {
  return personEvent(pullRequestId, { type: `resume` })
}
/** An owner's Merge now: next, alone, untested (a stack lands whole). */
export async function mergeNowPullRequest(pullRequestId: string): Promise<StepOutcome | null> {
  const ids = await stackOf(pullRequestId)
  let outcome: StepOutcome | null = null
  for (const id of ids) {
    const step = await stepPullRequest(id, { type: `merge_now` }, { base: ``, now: new Date() })
    if (id === pullRequestId) outcome = step
  }
  mergeTrainEdge({ pullRequestId })
  return outcome
}
/** `approval` mode: a person's Needs approval (then it queues). */
export async function approvePullRequest(pullRequestId: string, now: Date = new Date()): Promise<void> {
  const ids = await stackOf(pullRequestId)
  await db
    .update(pullRequests)
    .set({ mergeApprovedAt: now })
    .where(and(inArray(pullRequests.id, ids), sql`${pullRequests.mergeApprovedAt} IS NULL`))
  mergeTrainEdge({ pullRequestId })
}

/** The live PRs of the stack a PR belongs to (itself when alone). */
export async function stackOf(pullRequestId: string): Promise<string[]> {
  const [pr] = await db.select({ repositoryId: pullRequests.repositoryId }).from(pullRequests).where(eq(pullRequests.id, pullRequestId)).limit(1)
  if (!pr) return [pullRequestId]
  const units = buildUnits(await livePrs(pr.repositoryId))
  return units.find((unit) => unit.some((member) => member.id === pullRequestId))?.map((m) => m.id) ?? [pullRequestId]
}

export interface QueueReport {
  state: PrMergeState
  wait: string | null
  position: number | null
  /** The other PRs of its testing run. */
  testingWith: number
  holdReason: HoldReason | null
  attempts: number
}

/** Where a PR stands (MCP `pr merge`). */
export async function queueReport(pullRequestId: string): Promise<QueueReport | null> {
  const [pr] = await db.select().from(pullRequests).where(eq(pullRequests.id, pullRequestId)).limit(1)
  if (!pr) return null
  let testingWith = 0
  if (pr.mergeState === `testing` && pr.mergeTrainId) {
    const [row] = await db
      .select({ count: sql<number>`count(*)::int` })
      .from(pullRequests)
      .where(and(eq(pullRequests.mergeTrainId, pr.mergeTrainId), ne(pullRequests.id, pr.id), eq(pullRequests.mergeState, `testing`)))
    testingWith = row?.count ?? 0
  }
  return {
    state: pr.mergeState as PrMergeState,
    wait: pr.mergeWait,
    position: pr.mergePosition,
    testingWith,
    holdReason: (pr.mergeHold as HoldReason | null) ?? null,
    attempts: pr.mergeAttempts,
  }
}

export function resetMergeTrainForTests(): void {
  backoff.clear()
  inFlight.clear()
}

/** Tests: the direct landing of one run (its base landing meanwhile is a
 *  race no single pass reproduces). */
export async function landDirectForTests(repositoryId: string, runId: string): Promise<void> {
  const [repo] = await db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      fullName: repositories.fullName,
      installationId: repositories.installationId,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  const [run] = await db.select().from(mergeTrains).where(eq(mergeTrains.id, runId)).limit(1)
  await landDirect(repo!, await forgeFor(forgeRepo(repo!)), run!, run!.units, run!.base, new Date())
}
