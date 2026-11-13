// #EXP-1617: the merge train's pipeline on an eCode repository (merge-train.json
// `pipeline`, the provider-neutral contract GitHub runs as train.yml). The
// batch is COMPOSED by device jobs (E3's `squash`: each PR of each unit, in
// order, squashed onto the train tip; a unit that does not squash clean is a
// `conflict` and the tip stays before it), the composed head is published as
// `exp-train/<train_id>` (the server's ref CAS), and our CI plans its
// `merge_train` run there. The outcome is the contract's
// `TrainPipelineOutcome`; the worker reads our check rows at its head
// (`commit_checks`) and gates on the repo's required checks, exactly like a
// GitHub train. Differences from train.yml, on purpose for now: no resolver
// rules (`generated`), and the squash commit's author is the job's.
//
// Composing takes minutes of device jobs, so dispatch answers at once and
// composes in the background; the outcome is kept on the train row
// (`merge_trains.run_ref` = `ecode:` + its JSON, server-only, display-only
// otherwise) where `readPipeline` finds it, also after a restart.

import { and, eq } from "drizzle-orm"
import { PIPELINE_FILE } from "@exp/ecode"
import type { TrainPipelineInputs, TrainPipelineOutcome, TrainPipelineUnit, TrainUnitOutcome } from "@exp/domain-contract/merge-train"
import { db } from "@/db/connection"
import { mergeTrains } from "@/db/schema"
import type { ForgePipelineResult, ForgeRepo } from "@/lib/ecode/forge"
import type { EcodeHost } from "@/lib/ecode/forge-ecode"
import { TRAIN_REF_PREFIX } from "@/lib/merge-train/commit-checks"

const PENDING = `ecode:composing`
const PREFIX = `ecode:`
/** How long the background compose waits for the worker to record the dispatch. */
const RECORD_WAIT_MS = 30_000

type Dispatch = { ok: true; runRef: string | null } | { ok: false; missing: boolean; message: string }

function pipelineFires(text: string | null): boolean {
  if (text === null) return false
  try {
    const file = JSON.parse(text) as { on?: Record<string, unknown> }
    return !!file.on && typeof file.on === `object` && `merge_train` in file.on
  } catch {
    return false
  }
}

/** `dispatchPipeline` of the ecode forge. `missing` = no pipeline with a
 *  `merge_train` trigger at the base (the train lands directly, untested). */
export async function dispatchEcodeTrain(host: EcodeHost, repo: ForgeRepo, inputs: TrainPipelineInputs): Promise<Dispatch> {
  let text: string | null
  try {
    const read = await host.job(repo, { op: `read_file`, sha: inputs.base_sha, path: PIPELINE_FILE })
    if (!read.ok) return { ok: false, missing: false, message: read.message }
    text = read.result.text
  } catch (err) {
    return { ok: false, missing: false, message: err instanceof Error ? err.message : String(err) }
  }
  if (inputs.mode === `test` && !pipelineFires(text)) {
    return { ok: false, missing: true, message: `no ${PIPELINE_FILE} with an on.merge_train trigger at ${inputs.base}` }
  }
  void compose(host, repo, inputs)
    .then((outcome) => record(inputs.train_id, outcome))
    .catch((err: unknown) => console.error(`[ci] train ${inputs.train_id} compose failed`, err))
  return { ok: true, runRef: PENDING }
}

/** Compose the batch and start our run on it. Never throws: a failure is the outcome's `error`. */
export async function compose(host: EcodeHost, repo: ForgeRepo, inputs: TrainPipelineInputs): Promise<TrainPipelineOutcome> {
  const outcome: TrainPipelineOutcome = {
    train: inputs.train_id,
    base_sha: inputs.base_sha,
    head: null,
    checks: `none`,
    units: [],
    error: null,
  }
  try {
    const units = JSON.parse(inputs.units) as TrainPipelineUnit[]
    let tip = inputs.base_sha
    for (const unit of units) {
      const numbers = unit.prs.map((pr) => pr.number)
      let unitTip = tip
      let conflict: TrainUnitOutcome | null = null
      for (const pr of unit.prs) {
        const squash = await host.job(repo, { op: `squash`, base: unitTip, head: pr.head_sha, message: `${pr.title} (#${pr.number})` })
        if (squash.ok) {
          unitTip = squash.result.sha
          continue
        }
        if (squash.reason !== `dirty`) throw new Error(squash.message)
        const onTip = await host.job(repo, { op: `mergeability`, base: tip, head: pr.head_sha })
        const onBase = await host.job(repo, { op: `mergeability`, base: inputs.base_sha, head: pr.head_sha })
        conflict = {
          prs: numbers,
          outcome: `conflict`,
          files: onTip.ok ? onTip.result.conflicts : [],
          resolved: [],
          ...(onBase.ok && onBase.result.mergeability === `clean` && tip !== inputs.base_sha ? { against: `batch` as const } : {}),
        }
        break
      }
      if (conflict) {
        outcome.units.push(conflict)
        continue
      }
      tip = unitTip
      outcome.units.push({ prs: numbers, outcome: `composed`, files: [], resolved: [] })
    }
    if (!outcome.units.some((unit) => unit.outcome === `composed`)) return outcome
    const branch = `${TRAIN_REF_PREFIX}${inputs.train_id}`
    if (!(await host.moveBranches(repo, [{ branch, expected: null, sha: tip }]))) {
      throw new Error(`the train ref ${branch} already exists`)
    }
    outcome.head = tip
    if (inputs.mode === `untested`) {
      outcome.checks = `skipped`
      return outcome
    }
    const { startRun } = await import(`@/lib/ci/runs`)
    const run = await startRun({ repositoryId: repo.id, sha: tip, trigger: `merge_train`, ref: branch })
    outcome.checks = run.started || run.reason === `exists` ? `dispatched` : `none`
    return outcome
  } catch (err) {
    outcome.error = err instanceof Error ? err.message : String(err)
    return outcome
  }
}

async function record(trainId: string, outcome: TrainPipelineOutcome): Promise<void> {
  const deadline = Date.now() + RECORD_WAIT_MS
  // The worker records the dispatch (`run_ref` = PENDING) right after we
  // answered; never overwrite anything else.
  while (Date.now() < deadline) {
    const moved = await db
      .update(mergeTrains)
      .set({ runRef: `${PREFIX}${JSON.stringify(outcome)}` })
      .where(and(eq(mergeTrains.id, trainId), eq(mergeTrains.runRef, PENDING)))
      .returning({ id: mergeTrains.id })
    if (moved.length > 0) return
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
  console.error(`[ci] train ${trainId}: its row never recorded the dispatch; the outcome is dropped`)
}

/** `readPipeline` of the ecode forge: the outcome on the row, once composed. */
export function readEcodeTrain(runRef: string | null): ForgePipelineResult {
  if (!runRef || runRef === PENDING || !runRef.startsWith(PREFIX)) return { outcome: null, finished: false }
  try {
    return { outcome: JSON.parse(runRef.slice(PREFIX.length)) as TrainPipelineOutcome, finished: true }
  } catch {
    return { outcome: null, finished: true }
  }
}
