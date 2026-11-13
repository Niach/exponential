// #EXP-1617: the merge train's pipeline on an eCode repository (merge-train.json
// `pipeline`, the provider-neutral contract GitHub runs as train.yml). The
// batch is COMPOSED by ONE device job, `compose`: the same train engine as
// GitHub's (scripts/merge-train/compose.ts, ported to `ecode::compose`), so
// each PR of each unit is squashed onto the train tip with its author, the
// generated-file resolver rules of the base's `.exponential/merge-train.json`
// apply (prepend-union, renumber, regenerate, json-union, take-later), and
// the engine picks the checks mode (dispatched, proven, skipped, none). The
// composed head is published as `exp-train/<train_id>` (the server's ref
// CAS) and, when the engine dispatched, our CI plans its `merge_train` run
// there. The outcome is the contract's `TrainPipelineOutcome`; the worker
// reads our check rows at its head (`commit_checks`) and gates on the repo's
// required checks, exactly like a GitHub train.
//
// Composing takes minutes, so dispatch answers at once and composes in the
// background; the outcome is kept on the train row (`merge_trains.run_ref` =
// `ecode:` + its JSON, server-only, display-only otherwise) where
// `readPipeline` finds it, also after a restart.

import { and, eq } from "drizzle-orm"
import { PIPELINE_FILE } from "@exp/ecode"
import type { TrainPipelineInputs, TrainPipelineOutcome, TrainPipelineUnit } from "@exp/domain-contract/merge-train"
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
  void compose(host, repo, inputs, pipelineFires(text) ? [PIPELINE_FILE] : [])
    .then((outcome) => record(inputs.train_id, outcome))
    .catch((err: unknown) => console.error(`[ci] train ${inputs.train_id} compose failed`, err))
  return { ok: true, runRef: PENDING }
}

/** Compose the batch (one device job) and start our run on it. Never
 *  throws: a failure is the outcome's `error`. `workflows` = our pipeline
 *  when it fires on `merge_train`: the engine's CI when the repo's
 *  `.exponential/merge-train.json` names none. */
export async function compose(host: EcodeHost, repo: ForgeRepo, inputs: TrainPipelineInputs, workflows: string[] = []): Promise<TrainPipelineOutcome> {
  let outcome: TrainPipelineOutcome = {
    train: inputs.train_id,
    base_sha: inputs.base_sha,
    head: null,
    checks: `none`,
    units: [],
    error: null,
  }
  try {
    const units = JSON.parse(inputs.units) as TrainPipelineUnit[]
    const job = await host.job(repo, {
      op: `compose`,
      train_id: inputs.train_id,
      base_sha: inputs.base_sha,
      mode: inputs.mode,
      units,
      ...(workflows.length > 0 ? { workflows } : {}),
    })
    if (!job.ok) throw new Error(job.message)
    outcome = job.result.outcome
    if (outcome.error || !outcome.head || !outcome.units.some((unit) => unit.outcome === `composed`)) return outcome
    const branch = `${TRAIN_REF_PREFIX}${inputs.train_id}`
    if (!(await host.moveBranches(repo, [{ branch, expected: null, sha: outcome.head }]))) {
      throw new Error(`the train ref ${branch} already exists`)
    }
    // The engine's mode stands (skipped / proven / none); dispatched = our run.
    if (outcome.checks !== `dispatched`) return outcome
    const { startRun } = await import(`@/lib/ci/runs`)
    const run = await startRun({ repositoryId: repo.id, sha: outcome.head, trigger: `merge_train`, ref: branch })
    if (!run.started && run.reason !== `exists`) outcome.checks = `none`
    return outcome
  } catch (err) {
    return { ...outcome, error: err instanceof Error ? err.message : String(err) }
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
