import { describe, expect, it, vi } from "vitest"
import type { TrainChecksMode, TrainPipelineInputs, TrainPipelineOutcome, TrainUnitOutcome } from "@exp/domain-contract/merge-train"
import type { BranchMove, EcodeHost, EcodeJob } from "@/lib/ecode/forge-ecode"

// #EXP-1617: the ecode merge train composes with ONE device job (`compose`,
// the train engine's port: resolver rules included) and starts our
// `merge_train` run on the head when the engine dispatched
// (lib/ci/train.ts). The host is faked: it answers the compose job with the
// outcome the engine would.

vi.mock(`@/db/connection`, () => ({ db: {} }))
const startRun = vi.fn(async (_args: unknown) => ({ started: true as const, runId: `run-1` }))
vi.mock(`@/lib/ci/runs`, () => ({ startRun }))

const { compose, dispatchEcodeTrain, readEcodeTrain } = await import(`@/lib/ci/train`)

const BASE = `b`.repeat(40)
const HEAD = `h`.repeat(40)
const repo = { id: `repo-1`, fullName: `team/app`, provider: `ecode` as const }

type Answer = { ok: true; result: { outcome: TrainPipelineOutcome; dispatch: string[] } } | { ok: false; reason: string; message: string }

function host(opts: { pipeline?: string | null; answer?: (job: Extract<EcodeJob, { op: `compose` }>) => Answer } = {}) {
  const jobs: EcodeJob[] = []
  const moves: BranchMove[][] = []
  const fake = {
    async job(_repo: unknown, job: EcodeJob) {
      jobs.push(job)
      if (job.op === `read_file`) return { ok: true, result: { text: opts.pipeline ?? null } }
      if (job.op === `compose`) return opts.answer!(job)
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

const pr = (number: number) => ({ number, head_sha: `head-${number}`, title: `PR ${number}`, ...(number === 1 ? { author: `Ada <ada@example.com>` } : {}) })

const engine = (checks: TrainChecksMode, units: TrainUnitOutcome[], head: string | null = HEAD) => (): Answer => ({
  ok: true,
  result: {
    outcome: { train: `t1`, base_sha: BASE, head, checks, units, error: null },
    dispatch: checks === `dispatched` ? [`ci.json`] : [],
  },
})

const COMPOSED: TrainUnitOutcome[] = [
  { prs: [1, 2], outcome: `composed`, files: [], resolved: [`changelog`] },
  // It composes clean on the base alone: only the batch is in the way.
  { prs: [3], outcome: `conflict`, files: [`a.txt`], resolved: [], against: `batch` },
]

describe(`the ecode merge train`, () => {
  it(`composes with one engine job, publishes the head and starts our run when the engine dispatched`, async () => {
    startRun.mockClear()
    const { fake, jobs, moves } = host({ answer: engine(`dispatched`, COMPOSED) })
    const units = [{ prs: [pr(1), pr(2)] }, { prs: [pr(3)] }]
    const outcome = await compose(fake, repo, inputs(units), [`.exponential/ci.json`])
    // The units ride whole (the PR's author included) to the device's engine.
    expect(jobs).toEqual([{ op: `compose`, train_id: `t1`, base_sha: BASE, mode: `test`, units, workflows: [`.exponential/ci.json`] }])
    expect(outcome).toEqual({ train: `t1`, base_sha: BASE, head: HEAD, checks: `dispatched`, units: COMPOSED, error: null })
    expect(moves).toEqual([[{ branch: `exp-train/t1`, expected: null, sha: HEAD }]])
    expect(startRun).toHaveBeenCalledWith({ repositoryId: `repo-1`, sha: HEAD, trigger: `merge_train`, ref: `exp-train/t1` })
  })

  it(`keeps the engine's mode: a clean Merge now is skipped, a proven unit needs no run`, async () => {
    for (const checks of [`skipped`, `proven`] as const) {
      startRun.mockClear()
      const { fake, moves } = host({ answer: engine(checks, [COMPOSED[0]!]) })
      const outcome = await compose(fake, repo, inputs([{ prs: [pr(1)] }], checks === `skipped` ? `untested` : `test`))
      expect(outcome.checks).toBe(checks)
      expect(moves).toEqual([[{ branch: `exp-train/t1`, expected: null, sha: HEAD }]])
      expect(startRun).not.toHaveBeenCalled()
    }
  })

  it(`nothing composed moves no ref and starts no run`, async () => {
    startRun.mockClear()
    const { fake, moves } = host({ answer: engine(`none`, [COMPOSED[1]!]) })
    const outcome = await compose(fake, repo, inputs([{ prs: [pr(3)] }]))
    expect(outcome.units).toEqual([COMPOSED[1]])
    expect(moves).toEqual([])
    expect(startRun).not.toHaveBeenCalled()
  })

  it(`a failed job is the outcome's error`, async () => {
    const { fake, moves } = host({ answer: () => ({ ok: false, reason: `invalid`, message: `no device` }) })
    const outcome = await compose(fake, repo, inputs([{ prs: [pr(1)] }]))
    expect(outcome).toEqual({ train: `t1`, base_sha: BASE, head: null, checks: `none`, units: [], error: `no device` })
    expect(moves).toEqual([])
  })

  it(`dispatch hands the engine our pipeline when it fires on merge_train`, async () => {
    const quiet = vi.spyOn(console, `error`).mockImplementation(() => {})
    const fires = JSON.stringify({ version: 1, on: { merge_train: {} }, jobs: {} })
    const { fake, jobs } = host({ pipeline: fires, answer: engine(`none`, []) })
    expect(await dispatchEcodeTrain(fake, repo, inputs([{ prs: [pr(1)] }]))).toEqual({ ok: true, runRef: `ecode:composing` })
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(jobs.find((j) => j.op === `compose`)).toMatchObject({ workflows: [`.exponential/ci.json`] })
    quiet.mockRestore()
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
