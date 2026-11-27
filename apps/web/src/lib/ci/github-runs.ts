// #EXP-2172: GitHub Actions runs in the CI list. The App's `workflow_run` and
// `workflow_job` webhooks write the SAME `ci_runs` / `ci_jobs` rows our own
// CI writes, with `provider = github`, `external_id` (the run / job id) and
// `external_url` (its page), so the CI page, the run page and the badge read
// one list on every client. Nothing here plans, numbers, dispatches, ticks or
// reports: GitHub owns those runs (their checks reach the PR through the
// `check_run` events as before). Logs are read from GitHub on demand
// (`githubJobLog`), Re-run and Cancel call GitHub (`rerunGithubRun`,
// `cancelGithubRun`); all three need the installation's Actions permission
// (read for logs, write for the other two), which each installation accepts
// on github.com. Until then a GitHub row offers Open on GitHub only.

import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, sql } from "drizzle-orm"
import { checkConclusionFromGithub, checkStatusFromGithub } from "@exp/domain-contract/pr-checks"
import { db } from "@/db/connection"
import { ciJobs, ciRuns, repositories, type CiJob, type CiRun, type CiStepState } from "@/db/schema"
import { repoInstallationId } from "@/lib/ecode/provider-config"
import { githubApiHeaders, installationActionsAccess, resolveRepoInstallationToken } from "@/lib/integrations/github-app"
import { githubFetch } from "@/lib/integrations/github-client"
import { githubErrorMessage } from "@/lib/integrations/github-error"
import { resolveAppUserForGithubActor, type GithubActorRef } from "@/lib/integrations/github-identity"
import { pullRequestsForCheck } from "@/lib/integrations/pr-checks"
import { runTitle } from "@/lib/ci/runs"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"

export const GITHUB_CI = `github`

/** The `workflow_run` object of a webhook (or `GET …/actions/runs/{id}`). */
export interface GithubWorkflowRun {
  id?: number
  name?: string | null
  run_number?: number
  run_attempt?: number
  event?: string | null
  status?: string | null
  conclusion?: string | null
  head_branch?: string | null
  head_sha?: string | null
  html_url?: string | null
  /** The commit message's first line or the PR's title. */
  display_title?: string | null
  head_commit?: { message?: string | null } | null
  run_started_at?: string | null
  created_at?: string | null
  updated_at?: string | null
  pull_requests?: Array<{ number?: number }>
  actor?: GithubActorRef | null
  triggering_actor?: GithubActorRef | null
}

/** The `workflow_job` object of a webhook. */
export interface GithubWorkflowJob {
  id?: number
  run_id?: number
  run_attempt?: number
  name?: string | null
  workflow_name?: string | null
  head_branch?: string | null
  head_sha?: string | null
  status?: string | null
  conclusion?: string | null
  html_url?: string | null
  created_at?: string | null
  started_at?: string | null
  completed_at?: string | null
  labels?: string[]
  steps?: Array<{
    name?: string
    status?: string | null
    conclusion?: string | null
    started_at?: string | null
    completed_at?: string | null
  }>
}

function parseDate(value: string | null | undefined): Date | null {
  if (!value) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

/** GitHub's triggering event → our trigger names (`ci_runs.trigger`,
 *  repo-ci.json copy.triggers). Unknown events keep their own name. */
export function triggerFromGithub(event: string | null | undefined): string {
  switch (event) {
    case `push`:
      return `push`
    case `pull_request`:
    case `pull_request_target`:
      return `pull_request`
    case `workflow_dispatch`:
    case `repository_dispatch`:
      return `manual`
    case `schedule`:
      return `schedule`
    case `release`:
      return `tag`
    case `merge_group`:
      return `merge_train`
    default:
      return (event ?? `push`).slice(0, 16)
  }
}

const STATUS_RANK: Record<string, number> = { queued: 0, in_progress: 1, completed: 2 }

/** The rollup of a GitHub run (`ci_runs.state`): pending until completed,
 *  then success for success / skipped / neutral, else failure. */
export function githubRunState(status: string | null | undefined, conclusion: string | null | undefined): `pending` | `success` | `failure` {
  if (status !== `completed`) return `pending`
  return conclusion === `success` || conclusion === `skipped` || conclusion === `neutral` ? `success` : `failure`
}

/** The steps of a GitHub job as our runner's `CiStepState`s (epoch ms). */
export function stepsFromGithub(steps: GithubWorkflowJob[`steps`]): CiStepState[] {
  return (steps ?? []).slice(0, 256).flatMap((step) => {
    if (!step.name) return []
    const status = checkStatusFromGithub(step.status)
    return [
      {
        name: step.name.slice(0, 255),
        status,
        conclusion: checkConclusionFromGithub(status, step.conclusion),
        startedAt: parseDate(step.started_at)?.getTime() ?? null,
        completedAt: status === `completed` ? (parseDate(step.completed_at)?.getTime() ?? null) : null,
      },
    ]
  })
}

/** The team repositories a GitHub repository is linked as (one per team). */
async function linkedRepos(fullName: string, installationId: number | null) {
  const rows = await db
    .select({ id: repositories.id, teamId: repositories.teamId, installationId: repoInstallationId() })
    .from(repositories)
    .where(and(eq(repositories.fullName, fullName), eq(repositories.provider, `github`)))
  return rows.filter((row) => installationId == null || row.installationId == null || row.installationId === installationId)
}

/** Upsert one workflow run into every team repository linked to it. A
 *  late delivery never moves a row back (an older attempt, or a lower
 *  status of the same attempt). Returns the run ids written. */
export async function ingestWorkflowRun(args: {
  fullName: string
  installationId: number | null
  run: GithubWorkflowRun
}): Promise<string[]> {
  const run = args.run
  if (typeof run.id !== `number` || !run.head_sha) return []
  const repos = await linkedRepos(args.fullName, args.installationId)
  if (repos.length === 0) return []
  const status = checkStatusFromGithub(run.status)
  const attempt = run.run_attempt ?? 1
  const access = args.installationId != null ? await installationActionsAccess(args.installationId) : null
  const createdBy = await resolveAppUserForGithubActor(run.triggering_actor ?? run.actor ?? null)
  const prs = await pullRequestsForCheck({
    repoFullName: args.fullName,
    numbers: (run.pull_requests ?? []).flatMap((pr) => (typeof pr.number === `number` ? [pr.number] : [])),
    headBranch: run.event === `pull_request` || run.event === `pull_request_target` ? (run.head_branch ?? null) : null,
  })
  // A queued run has not started (no duration yet); the list orders it by
  // its row's time.
  const startedAt = status === `queued` ? null : (parseDate(run.run_started_at) ?? parseDate(run.created_at))
  const completedAt = status === `completed` ? (parseDate(run.updated_at) ?? new Date()) : null
  const written: string[] = []
  for (const repo of repos) {
    const externalId = String(run.id)
    const values = {
      teamId: repo.teamId,
      repositoryId: repo.id,
      number: run.run_number ?? 0,
      name: (run.name || `workflow`).slice(0, 64),
      sha: run.head_sha.slice(0, 64),
      ref: run.head_branch ? run.head_branch.slice(0, 255) : null,
      trigger: triggerFromGithub(run.event),
      pullRequestId: prs.find((pr) => pr.repositoryId === repo.id)?.id ?? null,
      state: githubRunState(status, run.conclusion),
      createdByUserId: createdBy,
      startedAt,
      completedAt,
      provider: GITHUB_CI,
      externalId,
      externalUrl: run.html_url ?? null,
      externalAttempt: attempt,
      externalAccess: access,
      title: runTitle(run.display_title) ?? runTitle(run.head_commit?.message),
    }
    const id = await db.transaction(async (tx) => {
      await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`ci_runs:gh:${repo.id}:${externalId}`}))`)
      const [existing] = await tx
        .select({ id: ciRuns.id, attempt: ciRuns.externalAttempt, state: ciRuns.state, completedAt: ciRuns.completedAt })
        .from(ciRuns)
        .where(and(eq(ciRuns.repositoryId, repo.id), eq(ciRuns.externalId, externalId)))
        .limit(1)
      if (!existing) {
        const [row] = await tx.insert(ciRuns).values(values).returning({ id: ciRuns.id })
        return row.id
      }
      const storedAttempt = existing.attempt ?? 0
      const storedRank = existing.completedAt ? 2 : 0
      if (attempt < storedAttempt) return null
      if (attempt === storedAttempt && STATUS_RANK[status] < storedRank) return null
      // A new attempt keeps who started the first one unless GitHub names one.
      const { createdByUserId, ...rest } = values
      await tx
        .update(ciRuns)
        .set({ ...rest, ...(createdByUserId ? { createdByUserId } : {}) })
        .where(eq(ciRuns.id, existing.id))
      return existing.id
    })
    if (id) written.push(id)
  }
  return written
}

/** Read one workflow run from GitHub (a job delivered before its run). */
async function fetchWorkflowRun(fullName: string, runId: number): Promise<GithubWorkflowRun | null> {
  const token = await resolveRepoInstallationToken(fullName).catch(() => null)
  if (!token) return null
  const res = await githubFetch(`/repos/${fullName}/actions/runs/${runId}`, { headers: githubApiHeaders(token) }).catch(() => null)
  if (!res?.ok) return null
  return (await res.json()) as GithubWorkflowRun
}

/** Upsert one workflow job. Its run is read from GitHub first when no
 *  delivery of it arrived yet. A job is keyed by its NAME inside the run, so
 *  a re-run attempt replaces the attempt before it and a "re-run failed
 *  jobs" attempt keeps the jobs that passed. */
export async function ingestWorkflowJob(args: {
  fullName: string
  installationId: number | null
  job: GithubWorkflowJob
}): Promise<string[]> {
  const job = args.job
  if (typeof job.id !== `number` || typeof job.run_id !== `number` || !job.name) return []
  const repos = await linkedRepos(args.fullName, args.installationId)
  if (repos.length === 0) return []
  const externalRunId = String(job.run_id)
  let runs = await db
    .select({ id: ciRuns.id, teamId: ciRuns.teamId, attempt: ciRuns.externalAttempt })
    .from(ciRuns)
    .where(and(inArray(ciRuns.repositoryId, repos.map((repo) => repo.id)), eq(ciRuns.externalId, externalRunId)))
  if (runs.length < repos.length) {
    const run = (await fetchWorkflowRun(args.fullName, job.run_id)) ?? {
      id: job.run_id,
      name: job.workflow_name ?? null,
      run_attempt: job.run_attempt ?? 1,
      status: `in_progress`,
      head_branch: job.head_branch ?? null,
      head_sha: job.head_sha ?? null,
      created_at: job.created_at ?? null,
    }
    await ingestWorkflowRun({ fullName: args.fullName, installationId: args.installationId, run })
    runs = await db
      .select({ id: ciRuns.id, teamId: ciRuns.teamId, attempt: ciRuns.externalAttempt })
      .from(ciRuns)
      .where(and(inArray(ciRuns.repositoryId, repos.map((repo) => repo.id)), eq(ciRuns.externalId, externalRunId)))
  }
  const status = checkStatusFromGithub(job.status)
  const conclusion = checkConclusionFromGithub(status, job.conclusion) as CiJob[`conclusion`]
  const name = job.name.slice(0, 255)
  const written: string[] = []
  for (const run of runs) {
    const id = await db.transaction(async (tx) => {
      await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`ci_jobs:gh:${run.id}`}))`)
      const rows = await tx
        .select({ id: ciJobs.id, check: ciJobs.check, externalId: ciJobs.externalId, status: ciJobs.status, position: ciJobs.position })
        .from(ciJobs)
        .where(eq(ciJobs.runId, run.id))
        .orderBy(asc(ciJobs.position))
      const externalId = String(job.id)
      const same = rows.find((row) => row.externalId === externalId)
      if (same && STATUS_RANK[status] < STATUS_RANK[same.status]) return null
      // An older attempt's job never replaces the newer attempt's.
      if (!same && (job.run_attempt ?? 1) < (run.attempt ?? 0)) return null
      const values = {
        status,
        conclusion,
        steps: stepsFromGithub(job.steps),
        runsOn: (job.labels ?? []).slice(0, 16).map((label) => label.slice(0, 64)),
        startedAt: parseDate(job.started_at),
        completedAt: status === `completed` ? parseDate(job.completed_at) : null,
        externalId,
        externalUrl: job.html_url ?? null,
      }
      const target = same ?? rows.find((row) => row.check === name)
      if (target) {
        await tx.update(ciJobs).set(values).where(eq(ciJobs.id, target.id))
        return target.id
      }
      const [row] = await tx
        .insert(ciJobs)
        .values({
          ...values,
          teamId: run.teamId,
          runId: run.id,
          key: name.slice(0, 64),
          check: name,
          position: rows.length,
          queuedAt: parseDate(job.created_at) ?? new Date(),
          spec: { env: {}, secrets: [], steps: [] },
          provider: GITHUB_CI,
        })
        .returning({ id: ciJobs.id })
      return row.id
    })
    if (id) written.push(id)
  }
  return written
}

/** `installation` `new_permissions_accepted`: the installation's GitHub rows
 *  learn the new Actions permission at once (Re-run appears). */
export async function refreshInstallationAccess(installationId: number): Promise<void> {
  const access = await installationActionsAccess(installationId)
  const repoIds = db.select({ id: repositories.id }).from(repositories).where(eq(repoInstallationId(), installationId))
  await db
    .update(ciRuns)
    .set({ externalAccess: access })
    .where(and(eq(ciRuns.provider, GITHUB_CI), inArray(ciRuns.repositoryId, repoIds)))
}

// ── people's side (tRPC ci.log / ci.rerun / ci.cancel, MCP) ────────────

async function runRepo(run: Pick<CiRun, `repositoryId`>) {
  const [repo] = await db
    .select({ fullName: repositories.fullName, installationId: repoInstallationId() })
    .from(repositories)
    .where(eq(repositories.id, run.repositoryId))
    .limit(1)
  if (!repo) throw new TRPCError({ code: `NOT_FOUND`, message: `Repository not found` })
  const token = await resolveRepoInstallationToken(repo.fullName, { fallbackInstallationId: repo.installationId as number | null })
  if (!token) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The GitHub App has no access to ${repo.fullName}` })
  return { fullName: repo.fullName, token }
}

export const ACTIONS_WRITE_MESSAGE = (fullName: string) =>
  `The GitHub App cannot re-run or cancel workflows on ${fullName}: it needs the Actions: write permission (accept it on github.com, in the App installation's settings).`

/** Re-run a GitHub run (all jobs, or only the failed ones): GitHub keeps the
 *  run id and starts a new attempt, so the row stays the same and the
 *  webhooks move it. */
export async function rerunGithubRun(run: CiRun, failedOnly: boolean): Promise<string> {
  if (!run.externalId) throw new TRPCError({ code: `BAD_REQUEST`, message: `Not a GitHub run` })
  const { fullName, token } = await runRepo(run)
  const path = `/repos/${fullName}/actions/runs/${run.externalId}/${failedOnly ? `rerun-failed-jobs` : `rerun`}`
  const res = await githubFetch(path, { method: `POST`, headers: githubApiHeaders(token) })
  if (res.status === 403 || res.status === 404) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: ACTIONS_WRITE_MESSAGE(fullName) })
  if (!res.ok) throw new TRPCError({ code: `BAD_GATEWAY`, message: `GitHub refused the re-run (${res.status}): ${await githubErrorMessage(res)}` })
  await db.update(ciRuns).set({ state: `pending`, completedAt: null }).where(eq(ciRuns.id, run.id))
  return run.id
}

export async function cancelGithubRun(run: CiRun): Promise<void> {
  if (!run.externalId) throw new TRPCError({ code: `BAD_REQUEST`, message: `Not a GitHub run` })
  const { fullName, token } = await runRepo(run)
  const res = await githubFetch(`/repos/${fullName}/actions/runs/${run.externalId}/cancel`, { method: `POST`, headers: githubApiHeaders(token) })
  if (res.status === 403 || res.status === 404) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: ACTIONS_WRITE_MESSAGE(fullName) })
  if (!res.ok && res.status !== 409) {
    throw new TRPCError({ code: `BAD_GATEWAY`, message: `GitHub refused the cancel (${res.status}): ${await githubErrorMessage(res)}` })
  }
}

/** GitHub log lines start with an ISO timestamp; our log view reads the
 *  runner markers (`##[group]`, `##[error]`) at the start of a line. */
export function githubLogText(raw: string): string {
  return raw
    .replace(/^\uFEFF/, ``)
    .split(/\r?\n/)
    .map((line) => line.replace(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z ?/, ``))
    .join(`\n`)
}

/** Per job, while it is warm: a finished job's log never changes. */
const logCache = new TtlPromiseCache<string>({ ttlMs: 10 * 60_000, maxEntries: 50 })

/** The same page as `readLog` (`ci.log`): GitHub serves a job's log only
 *  once the job finished, so a running GitHub job reads empty and not
 *  complete (the client keeps tailing, no GitHub call is spent), a finished
 *  one is read once and paged from the cache. */
export async function githubJobLog(
  job: CiJob,
  run: CiRun,
  offset: number,
  pageMax: number
): Promise<{ text: string; nextOffset: number; complete: boolean }> {
  if (job.status !== `completed`) return { text: ``, nextOffset: offset, complete: false }
  if (!job.externalId) return { text: ``, nextOffset: offset, complete: true }
  const text = await logCache.get(job.id, async () => {
    const { fullName, token } = await runRepo(run)
    const res = await githubFetch(`/repos/${fullName}/actions/jobs/${job.externalId}/logs`, { headers: githubApiHeaders(token) })
    if (res.status === 404 || res.status === 410) return ``
    if (res.status === 403) throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The GitHub App cannot read workflow logs on ${fullName}: it needs the Actions permission.` })
    if (!res.ok) throw new TRPCError({ code: `BAD_GATEWAY`, message: `GitHub returned ${res.status} reading the log` })
    return githubLogText(await res.text())
  })
  const slice = text.slice(offset, offset + pageMax)
  const nextOffset = offset + slice.length
  return { text: slice, nextOffset, complete: nextOffset >= text.length }
}
