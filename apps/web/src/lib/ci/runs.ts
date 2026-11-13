// #EXP-1617 eCode's own CI on the server: runs, dispatch, claims, results.
//
// - A run is planned from `.exponential/ci.json` at its commit (@exp/ecode
//   `validatePipeline` + `planPipeline`, fixtures/pipeline.json) into
//   `ci_jobs` rows. Fork PRs plan to the spec's `fork` skip: they never run.
// - Every state change of a run's jobs goes through @exp/ecode `schedule`
//   (fixtures/runner.json) under the run's advisory lock: enqueue (planned
//   skips), finish (the skip chain), cancel, tick (timeouts, lost runners,
//   the queued flag). `lib/ci/state.ts` converts rows ↔ the spec's state.
// - Dispatch = a `ci_job` device command (payload `{jobId}` only, never a
//   secret) plus the relay nudge to an enabled runner whose labels cover
//   the job; the runner CLAIMS the job by compare-and-swap and gets a
//   one-time job token (its sha256 is stored) for beats, logs, its secrets
//   (ONCE) and the finish.
// - Results write `commit_checks` + `pull_request_checks` (details_url = our
//   run page), so the Merge pill, the session wake-ups and the merge train
//   read them like any provider's; optionally GitHub commit statuses too.

import { createHash, randomBytes } from "node:crypto"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, gt, inArray, isNull, lt, or, sql } from "drizzle-orm"
import {
  PIPELINE_FILE,
  RUNNER_LOST_MS,
  planPipeline,
  schedule,
  validatePipeline,
  type PlannedJob,
  type RunnerState,
  type Trigger,
} from "@exp/ecode"
import { readWorkflowText } from "@exp/ecode/workflow-text"
import { db } from "@/db/connection"
import {
  ciJobs,
  ciRuns,
  commitChecks,
  devices,
  pullRequestChecks,
  pullRequests,
  repositories,
  teamMembers,
  teams,
  type CiJob,
  type CiJobSpec,
  type CiRun,
  type CiStepState,
  type DeviceRunnerSettings,
  type RepositoryCiConfig,
} from "@/db/schema"
import { forgeFor, type ForgeRepo } from "@/lib/ecode/forge"
import { repoInstallationId } from "@/lib/ecode/provider-config"
import { writePullRequestChecks, type CheckInput } from "@/lib/integrations/pr-checks"
import { writeCommitCheck } from "@/lib/merge-train/commit-checks"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { jobDiff, rollupOf, runState, runnerLabels } from "@/lib/ci/state"
import { withRepoRequired } from "@/lib/ci/required"

/** The device cap of a build that runs `ci_job` commands. */
export const CI_RUNNER_CAP = `ci-runner`
/** The device command kind that hands a runner one job. */
export const CI_JOB_COMMAND = `ci_job`
/** A dispatched job its runner has not claimed by then goes to another. */
export const CI_DISPATCH_STALE_MS = 90_000
/** A runner beats its job about this often (the daemon's job thread). */
export const CI_BEAT_MS = 15_000

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

export function hashJobToken(token: string): string {
  return createHash(`sha256`).update(token).digest(`hex`)
}

/** The run page (`ci-run`), the `details_url` of every check we write. */
export function ciRunUrl(teamSlug: string, runId: string, jobId?: string | null): string {
  return `${appBaseUrl()}/t/${teamSlug}/ci/${runId}${jobId ? `?job=${jobId}` : ``}`
}

type RepoRow = {
  id: string
  teamId: string
  fullName: string
  provider: `github` | `ecode`
  installationId: number | null
  ciConfig: RepositoryCiConfig
}

async function loadRepo(repositoryId: string): Promise<RepoRow | null> {
  const [repo] = await db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      fullName: repositories.fullName,
      provider: repositories.provider,
      installationId: repoInstallationId(),
      ciConfig: repositories.ciConfig,
    })
    .from(repositories)
    .where(eq(repositories.id, repositoryId))
    .limit(1)
  return (repo as RepoRow | undefined) ?? null
}

function forgeRepo(repo: RepoRow): ForgeRepo {
  return { id: repo.id, fullName: repo.fullName, provider: repo.provider, installationId: repo.installationId }
}

// ── planning ────────────────────────────────────────────────────────────

export interface StartRunInput {
  repositoryId: string
  sha: string
  trigger: Trigger
  /** push: the branch; tag: the tag; pull_request: the head branch. */
  ref?: string | null
  /** pull_request: the base branch. */
  base?: string | null
  pullRequestId?: string | null
  /** A pull request from outside the team: planned, never run. */
  fork?: boolean
  changedFiles?: string[] | null
  createdByUserId?: string | null
  rerunOf?: string | null
  /** A file already read (tests, a re-run): skips the forge read. */
  file?: unknown
}

export type StartRunResult =
  | { started: true; runId: string }
  | { started: false; reason: `no_pipeline` | `not_triggered` | `exists` | `disabled` | `no_runner` }

/** Our CI is opt-in on a GitHub repo (Actions is its CI) and on by default
 *  for an ecode repo: `ci_config.enabled`, unset = the provider's default. */
export function ciEnabled(repo: { provider: string; ciConfig: RepositoryCiConfig | null }): boolean {
  return repo.ciConfig?.enabled ?? repo.provider === `ecode`
}

/** Whether any member of the team has a device that runs our CI. No runner =
 *  no run is planned: queued rows nobody can take only hold pending checks. */
async function teamHasRunner(teamId: string): Promise<boolean> {
  const [row] = await db
    .select({ id: devices.id })
    .from(devices)
    .innerJoin(teamMembers, and(eq(teamMembers.userId, devices.userId), eq(teamMembers.teamId, teamId)))
    .where(and(sql`(${devices.runner}->>'enabled')::boolean IS TRUE`, sql`${devices.caps} ? ${CI_RUNNER_CAP}`))
    .limit(1)
  return Boolean(row)
}

/** The planned job's spec as a runner gets it. */
function specOf(job: PlannedJob): CiJobSpec {
  return {
    env: job.env,
    secrets: job.secrets,
    steps: job.steps.map((step) => ({
      name: step.name,
      run: step.run,
      env: step.env,
      timeoutMin: step.timeoutMin,
      continueOnError: step.continueOnError,
      workingDirectory: step.workingDirectory,
    })),
  }
}

async function nextRunNumber(tx: Tx, repositoryId: string): Promise<number> {
  await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`ci_runs:` + repositoryId}))`)
  const [row] = await tx
    .select({ max: sql<number | null>`max(${ciRuns.number})` })
    .from(ciRuns)
    .where(eq(ciRuns.repositoryId, repositoryId))
  return (row?.max ?? 0) + 1
}

/** Plan and queue a run. Reads the pipeline file at `sha` through the forge;
 *  a file that does not validate is a FAILED run naming its errors (a
 *  `pipeline` check), never a silent no-op. */
/** One pipeline to plan: a parsed file, or why there is none to plan. */
interface PipelineSource {
  /** The run's name when the file has none (a workflow's file name). */
  fallbackName: string
  file?: unknown
  /** Reading or converting failed: the run fails with these lines. */
  failure?: string[]
}

const WORKFLOW_DIR = `.github/workflows`

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch (err) {
    return { __unparsable: err instanceof Error ? err.message : String(err) }
  }
}

/** What to plan at a commit: `.exponential/ci.json`, else (an ecode repo
 *  without one) every GitHub workflow in `.github/workflows` through the
 *  workflow reader (packages/ecode fixtures/workflow.json). A read that
 *  fails (the forge, no device for the job) is a source that FAILS (#EXP-1807):
 *  a push never plans silently. [] = no pipeline at all. */
async function pipelineSources(repo: RepoRow, sha: string): Promise<PipelineSource[]> {
  try {
    const forge = await forgeFor(forgeRepo(repo))
    const text = await forge.readFile(forgeRepo(repo), PIPELINE_FILE, sha)
    if (text !== null) return [{ fallbackName: `ci`, file: parseJson(text) }]
    if (repo.provider !== `ecode`) return []
    const names = ((await forge.listFiles(forgeRepo(repo), WORKFLOW_DIR, sha)) ?? [])
      .filter((name) => /\.ya?ml$/.test(name))
      .sort()
    const sources: PipelineSource[] = []
    for (const name of names) {
      const fallbackName = name.replace(/\.ya?ml$/, ``).slice(0, 64)
      const workflowText = await forge.readFile(forgeRepo(repo), `${WORKFLOW_DIR}/${name}`, sha)
      if (workflowText === null) continue
      const read = readWorkflowText(workflowText)
      sources.push(
        read.file
          ? { fallbackName, file: read.file }
          : { fallbackName, failure: read.errors.map((e) => `${WORKFLOW_DIR}/${name}${e.path || ``}: ${e.code}`) }
      )
    }
    return sources
  } catch (err) {
    return [{ fallbackName: `ci`, failure: [`Could not read the pipeline: ${err instanceof Error ? err.message : String(err)}`] }]
  }
}

/** Plan and queue a run per pipeline at the commit. A file that does not
 *  validate, a workflow the reader refuses or a read that fails is a FAILED
 *  run naming the cause (a `pipeline` check), never a silent no-op. */
export async function startRun(input: StartRunInput): Promise<StartRunResult> {
  const repo = await loadRepo(input.repositoryId)
  if (!repo) return { started: false, reason: `no_pipeline` }
  if (!ciEnabled(repo)) return { started: false, reason: `disabled` }
  if (!(await teamHasRunner(repo.teamId))) return { started: false, reason: `no_runner` }
  const sources: PipelineSource[] = input.file !== undefined ? [{ fallbackName: `ci`, file: input.file }] : await pipelineSources(repo, input.sha)
  if (sources.length === 0) return { started: false, reason: `no_pipeline` }
  const results: StartRunResult[] = []
  for (const source of sources) results.push(await startOneRun(repo, input, source))
  return results.find((r) => r.started) ?? results.find((r) => !r.started && r.reason === `exists`) ?? results[0]!
}

async function startOneRun(repo: RepoRow, input: StartRunInput, source: PipelineSource): Promise<StartRunResult> {
  const file = source.file
  const named = file && typeof file === `object` && typeof (file as { name?: unknown }).name === `string` ? (file as { name: string }).name : null
  const runName = (named ?? source.fallbackName).slice(0, 64)
  // One run per (commit, trigger, PR, pipeline) unless a re-run or a manual one.
  if (!input.rerunOf && input.trigger !== `manual`) {
    const [existing] = await db
      .select({ id: ciRuns.id })
      .from(ciRuns)
      .where(
        and(
          eq(ciRuns.repositoryId, repo.id),
          eq(ciRuns.sha, input.sha),
          eq(ciRuns.trigger, input.trigger),
          eq(ciRuns.name, runName),
          input.pullRequestId ? eq(ciRuns.pullRequestId, input.pullRequestId) : isNull(ciRuns.pullRequestId)
        )
      )
      .limit(1)
    if (existing) return { started: false, reason: `exists` }
  }

  const errorLines: string[] | null = source.failure
    ? source.failure
    : file && typeof file === `object` && `__unparsable` in (file as object)
      ? [`/: invalid_json`]
      : validatePipeline(file as never).map((e) => `${e.path || `/`}: ${e.code}`)
  const errors = errorLines ?? []
  const plan = errors.length === 0 ? planPipeline(file as never, {
    kind: input.trigger,
    branch: input.trigger === `push` ? input.ref ?? null : null,
    base: input.base ?? null,
    tag: input.trigger === `tag` ? input.ref ?? null : null,
    changedFiles: input.changedFiles ?? null,
    fork: input.fork === true,
  }) : null
  if (errors.length === 0 && !plan) return { started: false, reason: `not_triggered` }

  const now = new Date()
  const runId = await db.transaction(async (tx) => {
    const number = await nextRunNumber(tx, repo.id)
    const [run] = await tx
      .insert(ciRuns)
      .values({
        teamId: repo.teamId,
        repositoryId: repo.id,
        number,
        name: (plan?.name ?? runName).slice(0, 64),
        sha: input.sha,
        ref: input.ref ?? null,
        trigger: input.trigger,
        pullRequestId: input.pullRequestId ?? null,
        state: errors.length > 0 ? `failure` : `pending`,
        error: errors.length > 0 ? errors.join(`\n`).slice(0, 4000) : null,
        rerunOf: input.rerunOf ?? null,
        createdByUserId: input.createdByUserId ?? null,
        completedAt: errors.length > 0 ? now : null,
      })
      .returning({ id: ciRuns.id })
    if (!plan) return run.id

    // The planner's `needs` are check names; the scheduler's are job ids.
    const ids = plan.jobs.map(() => crypto.randomUUID())
    const idOfCheck = new Map(plan.jobs.map((job, i) => [job.check, ids[i]]))
    const rows = plan.jobs.map((job, i) => ({
      id: ids[i],
      teamId: repo.teamId,
      runId: run.id,
      key: job.job.slice(0, 64),
      check: job.check.slice(0, 255),
      position: i,
      runsOn: job.runsOn,
      needs: job.needs.flatMap((check) => (idOfCheck.has(check) ? [idOfCheck.get(check)!] : [])),
      matrix: job.matrix,
      timeoutMin: job.timeoutMin,
      steps: job.steps.map((step): CiStepState => ({ name: step.name, status: `queued`, conclusion: null, startedAt: null, completedAt: null })),
      queuedAt: now,
      spec: specOf(job),
    }))
    if (rows.length > 0) await tx.insert(ciJobs).values(rows)
    // Planned skips (fork, paths) complete at once, by the spec's enqueue.
    const outcome = schedule({ jobs: [], runners: [] }, `enqueue`, {
      jobs: plan.jobs.map((job, i) => ({
        id: ids[i],
        check: job.check,
        runsOn: job.runsOn,
        needs: rows[i].needs,
        timeoutMin: job.timeoutMin,
        ...(job.skip ? { skip: job.skip } : {}),
      })),
    }, now.getTime())
    for (const job of outcome.state.jobs) {
      if (job.status !== `completed`) continue
      await tx
        .update(ciJobs)
        .set({
          status: `completed`,
          conclusion: job.conclusion,
          reason: job.reason,
          completedAt: now,
          steps: rows.find((r) => r.id === job.id)!.steps.map((step) => ({ ...step, status: `completed`, conclusion: `skipped` })),
        })
        .where(eq(ciJobs.id, job.id))
    }
    const rollup = plan.jobs.length === 0 ? `success` : rollupOf(outcome.state.jobs)
    if (rollup !== `pending`) {
      await tx.update(ciRuns).set({ state: rollup, completedAt: now }).where(eq(ciRuns.id, run.id))
    }
    return run.id
  })

  if (input.trigger === `pull_request` && input.pullRequestId && !input.rerunOf) {
    await supersedeOlderRuns({ repositoryId: repo.id, pullRequestId: input.pullRequestId, name: runName, sha: input.sha, keep: runId })
  }
  await reportRun(runId, { all: true })
  await dispatchQueued({ teamId: repo.teamId })
  return { started: true, runId }
}

/** A push to a PR supersedes its older heads (like GitHub's `concurrency`
 *  with cancel-in-progress): their unfinished runs of the same pipeline are
 *  cancelled (a running job stops at its next log or beat), so a busy runner
 *  never works through stale heads; the dogfood queued three heads of one PR.
 *  Their check rows are DELETED, never reported `cancelled`: a cancelled row
 *  counts as failing and would paint the PR's checks red. Runs before the new
 *  run reports, so its rows are the PR's. */
async function supersedeOlderRuns(args: { repositoryId: string; pullRequestId: string; name: string; sha: string; keep: string }): Promise<void> {
  const older = await db
    .select({ id: ciRuns.id, sha: ciRuns.sha })
    .from(ciRuns)
    .where(
      and(
        eq(ciRuns.repositoryId, args.repositoryId),
        eq(ciRuns.pullRequestId, args.pullRequestId),
        eq(ciRuns.trigger, `pull_request`),
        eq(ciRuns.name, args.name),
        eq(ciRuns.state, `pending`),
        sql`${ciRuns.sha} <> ${args.sha}`,
        sql`${ciRuns.id} <> ${args.keep}`
      )
    )
  const now = new Date()
  for (const run of older) {
    await applyToRun(run.id, `cancel`, {}, now)
    const checks = (await db.select({ check: ciJobs.check }).from(ciJobs).where(eq(ciJobs.runId, run.id))).map((job) => job.check)
    if (checks.length === 0) continue
    await db.transaction(async (tx) => {
      await tx
        .delete(commitChecks)
        .where(and(eq(commitChecks.repositoryId, args.repositoryId), eq(commitChecks.sha, run.sha), inArray(commitChecks.name, checks)))
      await tx
        .delete(pullRequestChecks)
        .where(
          and(
            eq(pullRequestChecks.pullRequestId, args.pullRequestId),
            eq(pullRequestChecks.headSha, run.sha),
            inArray(pullRequestChecks.name, checks)
          )
        )
    })
  }
}

// ── results → check rows ────────────────────────────────────────────────

const PIPELINE_CHECK = `pipeline`

/** The check a run that could not plan reports as: `pipeline` for
 *  `.exponential/ci.json`, `pipeline (<name>)` for a workflow, so two broken
 *  workflows never share one row. */
export function pipelineCheckName(runName: string): string {
  return runName === `ci` ? PIPELINE_CHECK : `${PIPELINE_CHECK} (${runName})`
}

function checkInputOf(job: CiJob, detailsUrl: string): CheckInput {
  return {
    name: job.check,
    status: job.status,
    conclusion: job.conclusion,
    startedAt: job.startedAt ?? (job.status === `queued` ? job.queuedAt : null),
    completedAt: job.completedAt,
    detailsUrl,
  }
}

function githubState(job: Pick<CiJob, `status` | `conclusion`>): `pending` | `success` | `failure` | `error` {
  if (job.status !== `completed`) return `pending`
  if (job.conclusion === `success` || job.conclusion === `skipped` || job.conclusion === `neutral`) return `success`
  return job.conclusion === `startup_failure` ? `error` : `failure`
}

/** Write the run's jobs (`jobIds`, or every one) as check rows: per commit,
 *  on its PR, and as GitHub statuses when the repo asked for that. */
export async function reportRun(runId: string, which: { all: true } | { jobIds: readonly string[] }): Promise<void> {
  const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, runId)).limit(1)
  if (!run) return
  const [team] = await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, run.teamId)).limit(1)
  const repo = await loadRepo(run.repositoryId)
  if (!team || !repo) return
  const jobs = await db
    .select()
    .from(ciJobs)
    .where(`all` in which ? eq(ciJobs.runId, runId) : and(eq(ciJobs.runId, runId), inArray(ciJobs.id, [...which.jobIds])))
  const inputs: CheckInput[] = jobs.map((job) => checkInputOf(job, ciRunUrl(team.slug, run.id, job.id)))
  if (`all` in which && run.error) {
    inputs.push({
      name: pipelineCheckName(run.name),
      status: `completed`,
      conclusion: `failure`,
      startedAt: run.createdAt,
      completedAt: run.completedAt ?? run.createdAt,
      detailsUrl: ciRunUrl(team.slug, run.id),
    })
  }
  if (inputs.length === 0) return

  for (const check of inputs) {
    await writeCommitCheck({ repoFullName: repo.fullName, sha: run.sha, check }).catch((err) =>
      console.error(`[ci] commit check write failed`, err)
    )
  }
  if (run.pullRequestId) {
    const [pr] = await db
      .select({ id: pullRequests.id, base: pullRequests.base })
      .from(pullRequests)
      .where(eq(pullRequests.id, run.pullRequestId))
      .limit(1)
    if (pr) {
      const forge = await forgeFor(forgeRepo(repo)).catch(() => null)
      const forgeRequired = forge ? await forge.requiredChecks(forgeRepo(repo), pr.base).catch(() => null) : null
      const required = forgeRequired ? new Set(withRepoRequired(repo.ciConfig, forgeRequired)) : undefined
      await writePullRequestChecks({
        pullRequestId: pr.id,
        headSha: run.sha,
        checks: inputs,
        ...(required ? { required } : {}),
      }).catch((err) => console.error(`[ci] PR check write failed`, err))
    }
  }
  if (repo.ciConfig?.githubStatuses && repo.provider === `github`) {
    const forge = await forgeFor(forgeRepo(repo)).catch(() => null)
    for (const check of inputs) {
      await forge
        ?.reportStatus(forgeRepo(repo), run.sha, {
          context: check.name,
          state: githubState({ status: check.status, conclusion: check.conclusion as CiJob[`conclusion`] }),
          description: check.status === `completed` ? `${check.conclusion}` : check.status === `queued` ? `Queued` : `Running`,
          targetUrl: check.detailsUrl,
        })
        .catch((err) => console.error(`[ci] GitHub status failed`, err))
    }
  }
  // The merge train reads these rows: a completed one may advance it.
  if (inputs.some((check) => check.status === `completed`)) {
    const { mergeTrainEdge } = await import(`@/lib/merge-train/worker`)
    mergeTrainEdge({ repoFullName: repo.fullName })
  }
}

// ── the scheduler over rows ─────────────────────────────────────────────

interface RunnerRow {
  id: string
  userId: string
  deviceId: string
  platform: string | null
  caps: string[]
  runner: DeviceRunnerSettings | null
  lastSeenAt: Date
}

function runnerSpec(row: RunnerRow, lastBeat: number | null) {
  return {
    id: row.id,
    labels: runnerLabels(row.platform, row.runner?.labels),
    slots: Math.max(1, row.runner?.slots ?? 1),
    lastSeen: Math.max(row.lastSeenAt.getTime(), lastBeat ?? 0),
  }
}

/** Apply ONE scheduler op to a run under its lock; persist what moved.
 *  Returns the op's outcome and the ids of the jobs that changed. */
async function applyToRun(
  runId: string,
  op: string,
  args: Record<string, unknown>,
  now: Date
): Promise<{ changed: string[]; outcome: ReturnType<typeof schedule> | null; teamId: string | null }> {
  return db.transaction(async (tx) => {
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`ci_run:` + runId}))`)
    const [run] = await tx.select().from(ciRuns).where(eq(ciRuns.id, runId)).limit(1)
    if (!run) return { changed: [], outcome: null, teamId: null }
    const rows = await tx.select().from(ciJobs).where(eq(ciJobs.runId, runId)).orderBy(asc(ciJobs.position))
    const runnerIds = [...new Set(rows.flatMap((row) => (row.runnerDeviceId ? [row.runnerDeviceId] : [])))]
    const runnerRows: RunnerRow[] =
      runnerIds.length === 0
        ? []
        : ((await tx
            .select({
              id: devices.id,
              userId: devices.userId,
              deviceId: devices.deviceId,
              platform: devices.platform,
              caps: devices.caps,
              runner: devices.runner,
              lastSeenAt: devices.lastSeenAt,
            })
            .from(devices)
            .where(inArray(devices.id, runnerIds))) as RunnerRow[])
    const state: RunnerState = runState(
      rows,
      runnerRows.map((runner) => {
        const beats = rows.filter((row) => row.runnerDeviceId === runner.id && row.lastBeatAt).map((row) => row.lastBeatAt!.getTime())
        return runnerSpec(runner, beats.length > 0 ? Math.max(...beats) : null)
      })
    )
    // A job whose runner device row is gone: its runner is unseen forever.
    for (const row of rows) {
      if (row.status === `in_progress` && row.runnerDeviceId && !state.runners.some((r) => r.id === row.runnerDeviceId)) {
        state.runners.push({ id: row.runnerDeviceId, labels: [], slots: 1, lastSeen: 0, running: [row.id] })
      }
    }
    const outcome = schedule(state, op, args, now.getTime())
    if (outcome.failure) return { changed: [], outcome, teamId: run.teamId }
    const changed: string[] = []
    for (const next of outcome.state.jobs) {
      const row = rows.find((r) => r.id === next.id)
      if (!row) continue
      const set = jobDiff(row, next)
      if (Object.keys(set).length === 0) continue
      // A job that completes without its runner finishing it: close its steps.
      if (set.status === `completed`) {
        set.steps = row.steps.map((step) =>
          step.status === `completed`
            ? step
            : { ...step, status: `completed`, conclusion: step.status === `in_progress` ? next.conclusion : `skipped`, completedAt: step.status === `in_progress` ? now.getTime() : step.completedAt }
        )
      }
      await tx.update(ciJobs).set(set).where(eq(ciJobs.id, row.id))
      changed.push(row.id)
    }
    const rollup = rollupOf(outcome.state.jobs)
    if (rollup !== run.state || (rollup !== `pending` && !run.completedAt)) {
      await tx
        .update(ciRuns)
        .set({ state: rollup, completedAt: rollup === `pending` ? null : run.completedAt ?? now })
        .where(eq(ciRuns.id, runId))
    }
    return { changed, outcome, teamId: run.teamId }
  })
}

async function afterChange(runId: string, changed: readonly string[], teamId: string | null): Promise<void> {
  if (changed.length > 0) await reportRun(runId, { jobIds: changed })
  if (teamId) await dispatchQueued({ teamId })
}

// ── dispatch ────────────────────────────────────────────────────────────

/** Hand every ready queued job of the team (or of every team) to a runner:
 *  an online, enabled device of a team member with the `ci-runner` cap,
 *  whose labels cover the job's `runsOn` and with a free slot. The job is
 *  the scheduler's first claimable one for that runner (queue order). */
export async function dispatchQueued(scope: { teamId?: string } = {}, now: Date = new Date()): Promise<number> {
  const staleBefore = new Date(now.getTime() - CI_DISPATCH_STALE_MS)
  const queued = await db
    .select({ id: ciJobs.id, runId: ciJobs.runId, teamId: ciJobs.teamId })
    .from(ciJobs)
    .where(
      and(
        eq(ciJobs.status, `queued`),
        scope.teamId ? eq(ciJobs.teamId, scope.teamId) : undefined,
        or(isNull(ciJobs.dispatchedAt), lt(ciJobs.dispatchedAt, staleBefore))
      )
    )
    .orderBy(asc(ciJobs.queuedAt), asc(ciJobs.position))
    .limit(200)
  if (queued.length === 0) return 0

  const { queueDeviceCommand } = await import(`@/lib/trpc/devices`)
  let dispatched = 0
  for (const teamId of new Set(queued.map((job) => job.teamId))) {
    const runners = (await db
      .select({
        id: devices.id,
        userId: devices.userId,
        deviceId: devices.deviceId,
        platform: devices.platform,
        caps: devices.caps,
        runner: devices.runner,
        lastSeenAt: devices.lastSeenAt,
      })
      .from(devices)
      .innerJoin(teamMembers, and(eq(teamMembers.userId, devices.userId), eq(teamMembers.teamId, teamId)))
      .where(
        and(
          sql`(${devices.runner}->>'enabled')::boolean IS TRUE`,
          sql`${devices.caps} ? ${CI_RUNNER_CAP}`,
          gt(devices.lastSeenAt, new Date(now.getTime() - RUNNER_LOST_MS))
        )
      )) as RunnerRow[]
    if (runners.length === 0) continue
    // Busy = running, or dispatched and not stale yet (across every team).
    const busyRows = await db
      .select({ device: sql<string>`coalesce(${ciJobs.runnerDeviceId}, ${ciJobs.dispatchedDeviceId})`, count: sql<number>`count(*)::int` })
      .from(ciJobs)
      .where(
        and(
          inArray(sql`coalesce(${ciJobs.runnerDeviceId}, ${ciJobs.dispatchedDeviceId})`, runners.map((r) => r.id)),
          or(
            eq(ciJobs.status, `in_progress`),
            and(eq(ciJobs.status, `queued`), gt(ciJobs.dispatchedAt, staleBefore))
          )
        )
      )
      .groupBy(sql`coalesce(${ciJobs.runnerDeviceId}, ${ciJobs.dispatchedDeviceId})`)
    const busy = new Map(busyRows.map((row) => [row.device, row.count]))

    for (const runId of new Set(queued.filter((job) => job.teamId === teamId).map((job) => job.runId))) {
      const rows = await db.select().from(ciJobs).where(eq(ciJobs.runId, runId)).orderBy(asc(ciJobs.position))
      // Least loaded first, so parallel jobs spread over the runners.
      const order = [...runners].sort((a, b) => (busy.get(a.id) ?? 0) - (busy.get(b.id) ?? 0))
      const taken = new Set<string>()
      for (const runner of order) {
        const spec = runnerSpec(runner, null)
        while ((busy.get(runner.id) ?? 0) < spec.slots) {
          // Jobs dispatched to someone else (and not stale) are not claimable here.
          const claimable = rows.filter(
            (row) => !taken.has(row.id) && !(row.status === `queued` && row.dispatchedAt && row.dispatchedAt >= staleBefore)
          )
          const state = runState(
            rows.map((row) => (claimable.includes(row) || row.status !== `queued` ? row : { ...row, status: `in_progress` as const })),
            [{ ...spec, slots: Number.MAX_SAFE_INTEGER }]
          )
          const outcome = schedule(state, `claim`, { runner: runner.id }, now.getTime())
          const jobId = outcome.result?.job as string | null | undefined
          if (!jobId) break
          taken.add(jobId)
          const [moved] = await db
            .update(ciJobs)
            .set({ dispatchedDeviceId: runner.id, dispatchedAt: now })
            .where(
              and(
                eq(ciJobs.id, jobId),
                eq(ciJobs.status, `queued`),
                or(isNull(ciJobs.dispatchedAt), lt(ciJobs.dispatchedAt, staleBefore))
              )
            )
            .returning({ id: ciJobs.id })
          if (!moved) continue
          await queueDeviceCommand(db, {
            deviceRowId: runner.id,
            userId: runner.userId,
            deviceId: runner.deviceId,
            kind: CI_JOB_COMMAND,
            payload: { jobId },
            onDuplicate: `reuse`,
          })
          busy.set(runner.id, (busy.get(runner.id) ?? 0) + 1)
          dispatched++
        }
      }
    }
  }
  return dispatched
}

// ── the runner's side ───────────────────────────────────────────────────

export interface ClaimedJob {
  token: string
  job: {
    id: string
    check: string
    sha: string
    gitRef: string | null
    timeoutMin: number
    env: Record<string, string>
    secrets: string[]
    steps: CiJobSpec[`steps`]
  }
  runId: string
  repositoryId: string
}

/** A runner claims a job (by compare-and-swap): it must be queued, ready
 *  (its needs satisfied), within the runner's labels and slots. */
export async function claimJob(args: { userId: string; deviceId: string; jobId: string; now?: Date }): Promise<ClaimedJob | null> {
  const now = args.now ?? new Date()
  const [device] = (await db
    .select({
      id: devices.id,
      userId: devices.userId,
      deviceId: devices.deviceId,
      platform: devices.platform,
      caps: devices.caps,
      runner: devices.runner,
      lastSeenAt: devices.lastSeenAt,
    })
    .from(devices)
    .where(and(eq(devices.userId, args.userId), eq(devices.deviceId, args.deviceId)))
    .limit(1)) as RunnerRow[]
  if (!device) throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
  if (!device.runner?.enabled) throw new TRPCError({ code: `FORBIDDEN`, message: `This device is not a CI runner` })
  const [job] = await db.select().from(ciJobs).where(eq(ciJobs.id, args.jobId)).limit(1)
  if (!job) return null
  const [member] = await db
    .select({ id: teamMembers.id })
    .from(teamMembers)
    .where(and(eq(teamMembers.teamId, job.teamId), eq(teamMembers.userId, args.userId)))
    .limit(1)
  if (!member) return null

  const token = `expci_${randomBytes(32).toString(`base64url`)}`
  const claimed = await db.transaction(async (tx) => {
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`ci_run:` + job.runId}))`)
    const rows = await tx.select().from(ciJobs).where(eq(ciJobs.runId, job.runId)).orderBy(asc(ciJobs.position))
    const [{ running }] = await tx
      .select({ running: sql<number>`count(*)::int` })
      .from(ciJobs)
      .where(and(eq(ciJobs.runnerDeviceId, device.id), eq(ciJobs.status, `in_progress`)))
    const spec = runnerSpec(device, null)
    if (running >= spec.slots) return null
    // Only THIS job is claimable: every other queued one is hidden.
    const state = runState(
      rows.map((row) => (row.id === job.id || row.status !== `queued` ? row : { ...row, status: `in_progress` as const })),
      [{ ...spec, slots: Number.MAX_SAFE_INTEGER }]
    )
    const outcome = schedule(state, `claim`, { runner: device.id }, now.getTime())
    if (outcome.result?.job !== job.id) return null
    const [moved] = await tx
      .update(ciJobs)
      .set({
        status: `in_progress`,
        runnerDeviceId: device.id,
        startedAt: now,
        lastBeatAt: now,
        jobTokenHash: hashJobToken(token),
        logKey: `ci-logs/${job.teamId}/${job.id}`,
      })
      .where(and(eq(ciJobs.id, job.id), eq(ciJobs.status, `queued`)))
      .returning()
    if (!moved) return null
    await tx
      .update(ciRuns)
      .set({ startedAt: sql`coalesce(${ciRuns.startedAt}, ${now})` })
      .where(eq(ciRuns.id, job.runId))
    return moved
  })
  if (!claimed) return null
  const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, claimed.runId)).limit(1)
  await reportRun(claimed.runId, { jobIds: [claimed.id] })
  const cap = device.runner?.timeoutMin && device.runner.timeoutMin > 0 ? device.runner.timeoutMin : null
  return {
    token,
    runId: claimed.runId,
    repositoryId: run.repositoryId,
    job: {
      id: claimed.id,
      check: claimed.check,
      sha: run.sha,
      gitRef: run.ref ? (run.trigger === `tag` ? `refs/tags/${run.ref}` : `refs/heads/${run.ref}`) : null,
      timeoutMin: cap ? Math.min(cap, claimed.timeoutMin) : claimed.timeoutMin,
      env: claimed.spec.env,
      secrets: claimed.spec.secrets,
      steps: claimed.spec.steps,
    },
  }
}

/** The job behind a runner's token, while it runs. */
export async function jobForToken(jobId: string, token: string): Promise<CiJob | null> {
  const [job] = await db
    .select()
    .from(ciJobs)
    .where(and(eq(ciJobs.id, jobId), eq(ciJobs.jobTokenHash, hashJobToken(token))))
    .limit(1)
  return job ?? null
}

function cleanSteps(steps: unknown): CiStepState[] | null {
  if (!Array.isArray(steps)) return null
  return steps.slice(0, 256).map((step) => {
    const s = (step ?? {}) as Record<string, unknown>
    const status = s.status === `in_progress` || s.status === `completed` ? s.status : `queued`
    return {
      name: String(s.name ?? ``).slice(0, 255),
      status,
      conclusion: typeof s.conclusion === `string` ? s.conclusion.slice(0, 32) : null,
      startedAt: typeof s.startedAt === `number` ? s.startedAt : null,
      completedAt: typeof s.completedAt === `number` ? s.completedAt : null,
    }
  })
}

/** A runner's beat: `cancel` = stop (cancelled, timed out, lost, re-run). */
export async function beatJob(args: { jobId: string; token: string; steps?: unknown; now?: Date }): Promise<{ cancel: boolean }> {
  const job = await jobForToken(args.jobId, args.token)
  if (!job || job.status !== `in_progress`) return { cancel: true }
  const steps = cleanSteps(args.steps)
  await db
    .update(ciJobs)
    .set({ lastBeatAt: args.now ?? new Date(), ...(steps ? { steps } : {}) })
    .where(and(eq(ciJobs.id, job.id), eq(ciJobs.status, `in_progress`)))
  return { cancel: false }
}

/** The runner reports the job's end. A job the server already concluded
 *  (timed out, cancelled, lost) keeps that conclusion. */
export async function finishJob(args: {
  jobId: string
  token: string
  conclusion: string
  steps?: unknown
  now?: Date
}): Promise<{ ok: boolean }> {
  const now = args.now ?? new Date()
  const job = await jobForToken(args.jobId, args.token)
  if (!job) return { ok: false }
  const steps = cleanSteps(args.steps)
  if (steps) {
    await db.update(ciJobs).set({ steps }).where(and(eq(ciJobs.id, job.id), eq(ciJobs.status, `in_progress`)))
  }
  const { changed, outcome, teamId } = await applyToRun(
    job.runId,
    `finish`,
    { runner: job.runnerDeviceId, job: job.id, conclusion: args.conclusion },
    now
  )
  await afterChange(job.runId, changed, teamId)
  return { ok: !!outcome && !outcome.failure }
}

// ── people's side ───────────────────────────────────────────────────────

/** Cancel a run's queued and running jobs (runners see it on their beat). */
export async function cancelRun(runId: string, now: Date = new Date()): Promise<void> {
  const { changed, teamId } = await applyToRun(runId, `cancel`, {}, now)
  await afterChange(runId, changed, teamId)
}

/** A new run of the same commit; `failedOnly` keeps the jobs that passed
 *  (completed `success`, reason `reused`, their logs) and runs the rest. */
export async function rerunRun(args: { runId: string; failedOnly: boolean; userId: string }): Promise<string> {
  const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, args.runId)).limit(1)
  if (!run) throw new TRPCError({ code: `NOT_FOUND`, message: `Run not found` })
  const old = await db.select().from(ciJobs).where(eq(ciJobs.runId, run.id)).orderBy(asc(ciJobs.position))
  const now = new Date()
  const runId = await db.transaction(async (tx) => {
    const number = await nextRunNumber(tx, run.repositoryId)
    const [next] = await tx
      .insert(ciRuns)
      .values({
        teamId: run.teamId,
        repositoryId: run.repositoryId,
        number,
        name: run.name,
        sha: run.sha,
        ref: run.ref,
        trigger: run.trigger,
        pullRequestId: run.pullRequestId,
        state: `pending`,
        rerunOf: run.id,
        createdByUserId: args.userId,
      })
      .returning({ id: ciRuns.id })
    const ids = new Map(old.map((job) => [job.id, crypto.randomUUID()]))
    const keep = (job: CiJob) => args.failedOnly && job.status === `completed` && job.conclusion === `success`
    if (old.length > 0) {
      await tx.insert(ciJobs).values(
        old.map((job) => ({
          id: ids.get(job.id)!,
          teamId: job.teamId,
          runId: next.id,
          key: job.key,
          check: job.check,
          position: job.position,
          runsOn: job.runsOn,
          needs: job.needs.map((need) => ids.get(need) ?? need),
          matrix: job.matrix,
          timeoutMin: job.timeoutMin,
          spec: job.spec,
          queuedAt: now,
          ...(keep(job)
            ? {
                status: `completed` as const,
                conclusion: `success` as const,
                reason: `reused`,
                steps: job.steps,
                logKey: job.logKey,
                logBytes: 0,
                logChunks: job.logChunks,
                startedAt: job.startedAt,
                completedAt: job.completedAt,
                runnerDeviceId: job.runnerDeviceId,
              }
            : {
                steps: job.steps.map((step): CiStepState => ({ name: step.name, status: `queued`, conclusion: null, startedAt: null, completedAt: null })),
              }),
        }))
      )
    }
    if (old.length === 0 || old.every(keep)) {
      await tx.update(ciRuns).set({ state: `success`, completedAt: now }).where(eq(ciRuns.id, next.id))
    }
    return next.id
  })
  // A run that never planned (a broken file) is planned again from the file.
  if (old.length === 0 && run.error) {
    await db.delete(ciRuns).where(eq(ciRuns.id, runId))
    const result = await startRun({
      repositoryId: run.repositoryId,
      sha: run.sha,
      trigger: run.trigger as Trigger,
      ref: run.ref,
      pullRequestId: run.pullRequestId,
      createdByUserId: args.userId,
      rerunOf: run.id,
    })
    if (!result.started) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The pipeline file is gone` })
    return result.runId
  }
  await reportRun(runId, { all: true })
  await dispatchQueued({ teamId: run.teamId })
  return runId
}

// ── the clock (the session watchdog, every minute) ──────────────────────

/** fixtures/runner.json `tick` over every open run: timeouts, lost runners,
 *  the queued-too-long flag; then dispatch whatever is ready or stale. */
export async function ciTick(now: Date = new Date()): Promise<number> {
  const open = await db
    .selectDistinct({ runId: ciJobs.runId })
    .from(ciJobs)
    .where(sql`${ciJobs.status} <> 'completed'`)
    .limit(500)
  let moved = 0
  for (const { runId } of open) {
    const { changed } = await applyToRun(runId, `tick`, {}, now)
    moved += changed.length
    if (changed.length > 0) await reportRun(runId, { jobIds: changed })
  }
  await dispatchQueued({}, now)
  return moved
}

// ── reads ───────────────────────────────────────────────────────────────

export async function loadRunForMember(runId: string): Promise<CiRun> {
  const [run] = await db.select().from(ciRuns).where(eq(ciRuns.id, runId)).limit(1)
  if (!run) throw new TRPCError({ code: `NOT_FOUND`, message: `Run not found` })
  return run
}

export async function recentRuns(repositoryId: string, limit = 50): Promise<CiRun[]> {
  return db.select().from(ciRuns).where(eq(ciRuns.repositoryId, repositoryId)).orderBy(desc(ciRuns.createdAt)).limit(limit)
}
