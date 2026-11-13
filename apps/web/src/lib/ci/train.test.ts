import { describe, expect, it, vi } from "vitest"
import type { TrainPipelineInputs } from "@exp/domain-contract/merge-train"
import type { BranchMove, EcodeHost, EcodeJob } from "@/lib/ecode/forge-ecode"

// #EXP-1617: the ecode merge train composes by device jobs and starts our
// `merge_train` run on the head (lib/ci/train.ts). The host is faked: its
// squash "merges" by naming the commit, and PR 3's head conflicts.

vi.mock(`@/db/connection`, () => ({ db: {} }))
const startRun = vi.fn(async () => ({ started: true as const, runId: `run-1` }))
vi.mock(`@/lib/ci/runs`, () => ({ startRun }))

const { compose, dispatchEcodeTrain, readEcodeTrain } = await import(`@/lib/ci/train`)

const BASE = `b`.repeat(40)
const repo = { id: `repo-1`, fullName: `team/app`, provider: `ecode` as const }

function host(opts: { pipeline?: string | null } = {}) {
  const jobs: EcodeJob[] = []
  const moves: BranchMove[][] = []
  const fake = {
    async job(_repo: unknown, job: EcodeJob) {
      jobs.push(job)
      if (job.op === `read_file`) return { ok: true, result: { text: opts.pipeline ?? null } }
      if (job.op === `squash`) {
        if (job.head === `head-3`) return { ok: false, reason: `dirty`, message: `conflict` }
        return { ok: true, result: { sha: `${job.base.slice(0, 6)}+${job.head}` } }
      }
      if (job.op === `mergeability`) {
        return { ok: true, result: { mergeability: job.base === BASE ? `clean` : `dirty`, conflicts: job.base === BASE ? [] : [`a.txt`] } }
      }
      throw new Error(`unexpected ${job.op}`)
    },
    async moveBranches(_repo: unknown, list: BranchMove[]) {
      moves.push(list)
      return true
    },
  } as unknown as EcodeHost
  return { fake, jobs, moves }
}

const inputs = (units: unknown, mode: `test` | `untested` = `test`): TrainPipelineInputs => ({
  train_id: `t1`,
  base: `main`,
  base_sha: BASE,
  mode,
  units: JSON.stringify(units),
})

const pr = (number: number) => ({ number, head_sha: `head-${number}`, title: `PR ${number}` })

describe(`the ecode merge train`, () => {
  it(`squashes each PR in order, bounces a conflicting unit, publishes the head and starts our run`, async () => {
    const { fake, jobs, moves } = host()
    const outcome = await compose(fake, repo, inputs([{ prs: [pr(1), pr(2)] }, { prs: [pr(3)] }]))
    expect(jobs.filter((j) => j.op === `squash`).map((j) => (j as { head: string }).head)).toEqual([`head-1`, `head-2`, `head-3`])
    const head = `${`${BASE.slice(0, 6)}+head-1`.slice(0, 6)}+head-2`
    expect(outcome).toEqual({
      train: `t1`,
      base_sha: BASE,
      head,
      checks: `dispatched`,
      units: [
        { prs: [1, 2], outcome: `composed`, files: [], resolved: [] },
        // It merges clean on the base alone: only the batch is in the way.
        { prs: [3], outcome: `conflict`, files: [`a.txt`], resolved: [], against: `batch` },
      ],
      error: null,
    })
    expect(moves).toEqual([[{ branch: `exp-train/t1`, expected: null, sha: head }]])
    expect(startRun).toHaveBeenCalledWith({ repositoryId: `repo-1`, sha: head, trigger: `merge_train`, ref: `exp-train/t1` })
  })

  it(`an untested batch composes without a run`, async () => {
    startRun.mockClear()
    const { fake } = host()
    const outcome = await compose(fake, repo, inputs([{ prs: [pr(1)] }], `untested`))
    expect(outcome.checks).toBe(`skipped`)
    expect(startRun).not.toHaveBeenCalled()
  })

  it(`is missing without a pipeline that has a merge_train trigger`, async () => {
    expect(await dispatchEcodeTrain(host({ pipeline: null }).fake, repo, inputs([]))).toMatchObject({ ok: false, missing: true })
    const pushOnly = JSON.stringify({ version: 1, on: { push: {} }, jobs: {} })
    expect(await dispatchEcodeTrain(host({ pipeline: pushOnly }).fake, repo, inputs([]))).toMatchObject({ ok: false, missing: true })
  })

  it(`reads the outcome off the train row once composed`, () => {
    expect(readEcodeTrain(null)).toEqual({ outcome: null, finished: false })
    expect(readEcodeTrain(`ecode:composing`)).toEqual({ outcome: null, finished: false })
    const outcome = { train: `t1`, base_sha: BASE, head: `h`, checks: `dispatched`, units: [], error: null }
    expect(readEcodeTrain(`ecode:${JSON.stringify(outcome)}`)).toEqual({ outcome, finished: true })
  })
})
