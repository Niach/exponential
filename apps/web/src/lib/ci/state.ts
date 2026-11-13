// #EXP-1617: the bridge between `ci_jobs` rows and @exp/ecode's runner
// scheduler (fixtures/runner.json). Every state change of a run's jobs goes
// through `schedule` on the run's rows, so the server applies EXACTLY the
// spec's rules (the skip chain, timeouts, lost runners, the queued flag);
// this file only converts rows to the spec's state and back.

import type { CiJob } from "@exp/db-schema"
import { isFailingConclusion, type CheckConclusion, type Job as SpecJob, type Runner as SpecRunner, type RunnerState } from "@exp/ecode"

/** A job row as the scheduler sees it. */
export function specJob(row: CiJob): SpecJob {
  return {
    id: row.id,
    check: row.check,
    runsOn: row.runsOn,
    needs: row.needs,
    timeoutMin: row.timeoutMin,
    status: row.status,
    conclusion: (row.conclusion as SpecJob[`conclusion`]) ?? null,
    runner: row.runnerDeviceId,
    queuedAt: row.queuedAt.getTime(),
    startedAt: row.startedAt?.getTime() ?? null,
    completedAt: row.completedAt?.getTime() ?? null,
    flagged: row.queuedFlaggedAt !== null,
    reason: row.reason,
  }
}

/** The run's scheduler state: its jobs in queue order and the runners that
 *  run them. `lastSeen` = the runner's newest sign of life (its device
 *  heartbeat or a job beat). */
export function runState(
  rows: readonly CiJob[],
  runners: ReadonlyArray<{ id: string; labels: string[]; slots: number; lastSeen: number }>
): RunnerState {
  const jobs = [...rows].sort((a, b) => a.position - b.position).map(specJob)
  const spec: SpecRunner[] = runners.map((runner) => ({
    ...runner,
    running: jobs.filter((job) => job.status === `in_progress` && job.runner === runner.id).map((job) => job.id),
  }))
  return { jobs, runners: spec }
}

/** The columns of a row that moved, as an update set (empty = unchanged). */
export function jobDiff(row: CiJob, next: SpecJob): Partial<CiJob> {
  const set: Partial<CiJob> = {}
  if (next.status !== row.status) set.status = next.status
  if ((next.conclusion ?? null) !== (row.conclusion ?? null)) set.conclusion = next.conclusion
  if ((next.reason ?? null) !== (row.reason ?? null)) set.reason = next.reason
  if ((next.runner ?? null) !== (row.runnerDeviceId ?? null)) set.runnerDeviceId = next.runner
  const startedAt = next.startedAt === null ? null : new Date(next.startedAt)
  if ((startedAt?.getTime() ?? null) !== (row.startedAt?.getTime() ?? null)) set.startedAt = startedAt
  const completedAt = next.completedAt === null ? null : new Date(next.completedAt)
  if ((completedAt?.getTime() ?? null) !== (row.completedAt?.getTime() ?? null)) set.completedAt = completedAt
  if (next.flagged && row.queuedFlaggedAt === null) set.queuedFlaggedAt = new Date()
  return set
}

/** The run's rollup (checks.json): failure wins, then pending, then success. */
export function rollupOf(jobs: ReadonlyArray<Pick<SpecJob, `status` | `conclusion`>>): `pending` | `success` | `failure` {
  const failing = (job: Pick<SpecJob, `status` | `conclusion`>) =>
    job.status === `completed` && job.conclusion !== null && isFailingConclusion(job.conclusion as CheckConclusion)
  if (jobs.some(failing)) return `failure`
  if (jobs.some((job) => job.status !== `completed`)) return `pending`
  return `success`
}

/** The labels a runner device offers: its settings' labels plus its OS. */
export function runnerLabels(platform: string | null, labels: readonly string[] | undefined): string[] {
  const os = platform === `macos` || platform === `darwin` ? `mac` : platform === `linux` ? `linux` : platform === `windows` ? `windows` : null
  return [...new Set([...(os ? [os] : []), ...(labels ?? []).map((label) => label.toLowerCase())])]
}
