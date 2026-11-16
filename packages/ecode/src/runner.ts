// eCode's runner scheduling (fixtures/runner.json is the spec): jobs wait for
// their `needs`, a runner claims the one its labels cover that is expected to
// finish first, and the clock times jobs out, fails the jobs of a lost runner,
// flags a job queued too long (once) and times out one no runner claims. The server dispatches through it, and so does a server-free
// host. ecode-core ports this file.

import {
  QUEUED_FLAG_MINUTES,
  QUEUED_LIMIT_MINUTES,
  RUNNER_LOST_MS,
  isFailingConclusion,
  reasonCode,
  satisfiesNeeds,
  skipConclusion,
  type CheckConclusion,
  type CheckStatus,
  type ForgeReason,
  type Skip,
} from "./contract.generated"

export interface Job {
  id: string
  check: string
  runsOn: string[]
  needs: string[]
  timeoutMin: number
  /** How long it is expected to run (ms): the check's last duration; claims prefer the shortest. */
  expectMs?: number | null
  status: CheckStatus
  conclusion: CheckConclusion | null
  runner: string | null
  queuedAt: number
  startedAt: number | null
  completedAt: number | null
  flagged: boolean
  /** Why it completed without running (contract skip, `needs`, `runner_lost`, `unclaimed`). */
  reason: string | null
}

export interface Runner {
  id: string
  labels: string[]
  slots: number
  lastSeen: number
  running: string[]
}

export interface RunnerState {
  jobs: Job[]
  runners: Runner[]
}

export interface RunnerEventRow {
  kind: string
  [key: string]: unknown
}

export interface RunnerOutcome {
  state: RunnerState
  events: RunnerEventRow[]
  result?: Record<string, unknown>
  failure?: { code: string; reason: ForgeReason }
}

export const emptyRunnerState = (): RunnerState => ({ jobs: [], runners: [] })

const clone = (state: RunnerState): RunnerState => ({
  jobs: state.jobs.map((job) => ({ ...job, runsOn: [...job.runsOn], needs: [...job.needs] })),
  runners: state.runners.map((runner) => ({ ...runner, labels: [...runner.labels], running: [...runner.running] })),
})

class Refused extends Error {
  constructor(readonly reason: ForgeReason) {
    super(reason)
  }
}

function complete(state: RunnerState, job: Job, conclusion: CheckConclusion, at: number, events: RunnerEventRow[], reason?: string) {
  if (job.runner !== null) {
    const runner = state.runners.find((r) => r.id === job.runner)
    if (runner) runner.running = runner.running.filter((id) => id !== job.id)
  }
  job.status = `completed`
  job.conclusion = conclusion
  job.completedAt = at
  job.reason = reason ?? null
  events.push(reason ? { kind: `job_completed`, job: job.id, conclusion, reason } : { kind: `job_completed`, job: job.id, conclusion })
}

/** A completed need lets its dependents start: a satisfying conclusion that
 *  is not itself the skip of a blocked chain. */
function satisfied(dep: Job): boolean {
  return dep.status === `completed` && dep.conclusion !== null && satisfiesNeeds(dep.conclusion) && dep.reason !== `needs`
}

/** Queued jobs whose needs can no longer be met are skipped (to a fixpoint). */
function settle(state: RunnerState, at: number, events: RunnerEventRow[]) {
  for (let changed = true; changed; ) {
    changed = false
    for (const job of state.jobs) {
      if (job.status !== `queued`) continue
      const blocked = job.needs.some((need) => {
        const dep = state.jobs.find((j) => j.id === need)
        return dep !== undefined && dep.status === `completed` && !satisfied(dep)
      })
      if (blocked) {
        complete(state, job, `skipped`, at, events, `needs`)
        changed = true
      }
    }
  }
}

function ready(state: RunnerState, job: Job): boolean {
  return job.needs.every((need) => {
    const dep = state.jobs.find((j) => j.id === need)
    return dep !== undefined && satisfied(dep)
  })
}

/** When a ready job became claimable: its queuedAt or its last need's completion. */
function readyAt(state: RunnerState, job: Job): number {
  let at = job.queuedAt
  for (const need of job.needs) {
    const done = state.jobs.find((j) => j.id === need)?.completedAt
    if (done != null && done > at) at = done
  }
  return at
}

/** The job's expected run time: its expectMs, else its timeout. */
export function expectedMs(job: Pick<Job, `expectMs` | `timeoutMin`>): number {
  return job.expectMs ?? job.timeoutMin * 60_000
}

/** Claim order (#EXP-1862): a flagged job first, then the one expected to
 *  finish first; 0 = a tie (queue order decides). */
export function claimOrder(a: Pick<Job, `flagged` | `expectMs` | `timeoutMin`>, b: Pick<Job, `flagged` | `expectMs` | `timeoutMin`>): number {
  if (a.flagged !== b.flagged) return a.flagged ? -1 : 1
  return expectedMs(a) - expectedMs(b)
}

type Args = Record<string, unknown>

function apply(state: RunnerState, op: string, args: Args, at: number, events: RunnerEventRow[]): Record<string, unknown> {
  switch (op) {
    case `enqueue`: {
      for (const spec of (args.jobs as Array<Record<string, unknown>>) ?? []) {
        const id = String(spec.id)
        if (state.jobs.some((job) => job.id === id)) throw new Refused(`exists`)
        const job: Job = {
          id,
          check: String(spec.check ?? id),
          runsOn: (spec.runsOn as string[]) ?? [],
          needs: (spec.needs as string[]) ?? [],
          timeoutMin: Number(spec.timeoutMin ?? 60),
          expectMs: spec.expectMs == null ? null : Number(spec.expectMs),
          status: `queued`,
          conclusion: null,
          runner: null,
          queuedAt: at,
          startedAt: null,
          completedAt: null,
          flagged: false,
          reason: null,
        }
        state.jobs.push(job)
        events.push({ kind: `job_queued`, job: id })
        if (typeof spec.skip === `string`) complete(state, job, skipConclusion(spec.skip as Skip), at, events, spec.skip)
      }
      return {}
    }
    case `beat`: {
      const id = String(args.runner)
      const runner = state.runners.find((r) => r.id === id)
      const labels = (args.labels as string[]) ?? []
      const slots = Number(args.slots ?? 1)
      if (runner) Object.assign(runner, { labels, slots, lastSeen: at })
      else state.runners.push({ id, labels, slots, lastSeen: at, running: [] })
      return {}
    }
    case `claim`: {
      const runner = state.runners.find((r) => r.id === args.runner)
      if (!runner) throw new Refused(`runner`)
      runner.lastSeen = at
      if (runner.running.length >= runner.slots) return { job: null }
      let job: Job | undefined
      for (const j of state.jobs) {
        if (j.status !== `queued` || !ready(state, j) || !j.runsOn.every((label) => runner.labels.includes(label))) continue
        if (!job || claimOrder(j, job) < 0) job = j
      }
      if (!job) return { job: null }
      job.status = `in_progress`
      job.runner = runner.id
      job.startedAt = at
      runner.running.push(job.id)
      events.push({ kind: `job_started`, job: job.id, runner: runner.id })
      return { job: job.id }
    }
    case `finish`: {
      const job = state.jobs.find((j) => j.id === args.job)
      if (!job) throw new Refused(`job`)
      if (job.status !== `in_progress` || job.runner !== args.runner) throw new Refused(`not_running`)
      complete(state, job, args.conclusion as CheckConclusion, at, events)
      return {}
    }
    case `cancel`: {
      const ids = args.jobs as string[] | undefined
      for (const job of state.jobs) {
        if (ids && !ids.includes(job.id)) continue
        if (job.status === `in_progress`) events.push({ kind: `kill`, job: job.id, runner: job.runner })
        if (job.status !== `completed`) complete(state, job, `cancelled`, at, events)
      }
      return {}
    }
    case `tick`: {
      for (const job of state.jobs) {
        if (job.status === `in_progress` && job.startedAt !== null && at - job.startedAt >= job.timeoutMin * 60_000) {
          events.push({ kind: `kill`, job: job.id, runner: job.runner })
          complete(state, job, `timed_out`, at, events)
        }
      }
      for (const runner of state.runners) {
        if (at - runner.lastSeen < RUNNER_LOST_MS) continue
        for (const id of [...runner.running]) {
          const job = state.jobs.find((j) => j.id === id)!
          complete(state, job, `failure`, at, events, `runner_lost`)
        }
      }
      for (const job of state.jobs) {
        if (job.status === `queued` && !job.flagged && at - job.queuedAt >= QUEUED_FLAG_MINUTES * 60_000) {
          job.flagged = true
          events.push({ kind: `queued_too_long`, job: job.id })
        }
      }
      for (const job of state.jobs) {
        if (job.status === `queued` && ready(state, job) && at - readyAt(state, job) >= QUEUED_LIMIT_MINUTES * 60_000) {
          complete(state, job, `timed_out`, at, events, `unclaimed`)
        }
      }
      return {}
    }
    default:
      throw new Error(`unknown runner op ${op}`)
  }
}

/** One op at time `at` (unix ms). A refusal leaves the state unchanged. */
export function schedule(state: RunnerState, op: string, args: Args, at: number): RunnerOutcome {
  const next = clone(state)
  const events: RunnerEventRow[] = []
  try {
    const result = apply(next, op, args, at, events)
    settle(next, at, events)
    return { state: next, events, result }
  } catch (error) {
    if (!(error instanceof Refused)) throw error
    return { state, events: [], failure: { code: reasonCode(error.reason), reason: error.reason } }
  }
}

/** The run's rollup over its jobs: failure wins, then pending, then success. */
export function runConclusion(state: RunnerState): `pending` | `success` | `failure` {
  if (state.jobs.some((job) => job.status === `completed` && job.conclusion !== null && isFailingConclusion(job.conclusion))) return `failure`
  if (state.jobs.some((job) => job.status !== `completed`)) return `pending`
  return `success`
}
