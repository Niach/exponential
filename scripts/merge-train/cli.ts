// EXP-1708: the train pipeline's entrypoint, provider-neutral. Inputs come
// as env (the contract's dispatch inputs: TRAIN_ID, BASE_SHA, MODE, UNITS)
// plus TRAIN_DIR (a clone with every sha fetched) and OUTCOME (the file the
// outcome JSON goes to). Stdout: `ref_sha=` (what to push to
// exp-train/<train_id>, the head or a marker commit) and `dispatch=` (the CI
// workflows to start on it, comma-separated). The caller pushes, posts the
// `exp-train` result and starts the CI: the engine never holds a token.

import { writeFileSync } from "node:fs"
import type { TrainPipelineOutcome, TrainPipelineUnit } from "../../packages/domain-contract/src/merge-train"
import { compose } from "./compose"
import { Git } from "./git"

const env = (name: string): string => {
  const v = process.env[name]
  if (!v) throw new Error(`${name} is required`)
  return v
}

const trainId = env(`TRAIN_ID`)
if (!/^[A-Za-z0-9._-]+$/.test(trainId)) throw new Error(`TRAIN_ID: [A-Za-z0-9._-] only`)
const baseSha = env(`BASE_SHA`)
const dir = env(`TRAIN_DIR`)
const mode = (process.env.MODE || `test`) as `test` | `untested`
let outcome: TrainPipelineOutcome
let refSha: string
let dispatch: string[] = []
try {
  const units = JSON.parse(env(`UNITS`)) as TrainPipelineUnit[]
  const result = compose({ dir, trainId, baseSha, mode, units })
  outcome = result.outcome
  refSha = result.outcome.head!
  dispatch = result.dispatch
} catch (e) {
  const git = new Git(dir)
  const message = e instanceof Error ? e.message : String(e)
  console.error(`[merge-train] error: ${message}`)
  git.run(`reset`, `--quiet`, `--hard`)
  git.run(`checkout`, `--quiet`, `--detach`, baseSha)
  git.run(`commit`, `--quiet`, `--allow-empty`, `-m`, `exp-train ${trainId}: error`)
  refSha = git.run(`rev-parse`, `HEAD`).out.trim()
  outcome = { train: trainId, base_sha: baseSha, head: null, checks: `none`, units: [], error: message.slice(0, 2000) }
}
writeFileSync(env(`OUTCOME`), JSON.stringify(outcome))
console.log(`ref_sha=${refSha}`)
console.log(`dispatch=${dispatch.join(`,`)}`)
