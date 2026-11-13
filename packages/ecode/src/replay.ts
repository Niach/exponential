// Replays fixtures/*.json against the TS reference: each `replay*` returns the
// fixture with every output recomputed, so a test compares it with the file
// and `scripts/bless.ts` can write it after a deliberate spec change (review
// the diff). Shared by this package's test and the server's
// (apps/web/src/lib/ecode/ecode-spec.test.ts).

import { supports, type Capability, type Provider } from "./contract.generated"
import { checkName, mergeGate, rollup, type CheckRow } from "./checks"
import { byCodePoint, forge, hydrate, type ForgeGiven, type ForgeState } from "./forge"
import { globMatch, planPipeline, validatePipeline, type TriggerInput } from "./pipeline"
import { credentialHelperKey, needsRefresh, parseRemote, redact } from "./remote"
import { emptyRunnerState, runConclusion, schedule, type RunnerState } from "./runner"
import { readWorkflow } from "./workflow"
import { readWorkflowText } from "./workflow-text"

type Json = Record<string, unknown>

interface Step {
  op: string
  args?: Json
  at?: number
  result?: unknown
  failure?: unknown
  events?: unknown[]
}

interface Case {
  name: string
  given?: unknown
  steps: Step[]
  expect?: unknown
}

function stepOut(step: Step, outcome: { events: unknown[]; result?: unknown; failure?: unknown }): Step {
  const out: Step = { op: step.op }
  if (step.at !== undefined) out.at = step.at
  if (step.args !== undefined) out.args = step.args
  if (outcome.failure !== undefined) out.failure = outcome.failure
  else out.result = outcome.result ?? {}
  out.events = outcome.events
  return out
}

const sortKeys = <T>(record: Record<string, T>): Record<string, T> =>
  Object.fromEntries(Object.entries(record).sort(([a], [b]) => byCodePoint(a, b)))

function forgeExpect(given: ForgeState, state: ForgeState): Json {
  const commits: Record<string, unknown> = {}
  for (const [id, commit] of Object.entries(state.commits)) if (!given.commits[id]) commits[id] = commit
  return {
    branches: sortKeys(state.branches),
    prs: state.prs.map((pr) => ({ ...pr })).sort((a, b) => a.number - b.number),
    commits: sortKeys(commits),
  }
}

export function replayForge(fixture: { capabilities: Json[]; cases: Case[] } & Json) {
  const capabilities = fixture.capabilities.map((row) => ({
    ...row,
    supported: supports(row.provider as Provider, row.capability as Capability),
  }))
  const cases = fixture.cases.map((entry) => {
    const start = hydrate((entry.given ?? {}) as ForgeGiven)
    let state = start
    const steps = entry.steps.map((step) => {
      const outcome = forge(state, step.op, step.args ?? {})
      state = outcome.state
      return stepOut(step, outcome)
    })
    return { name: entry.name, given: entry.given, steps, expect: forgeExpect(start, state) }
  })
  return { ...fixture, capabilities, cases }
}

export function replayRunner(fixture: { cases: Case[] } & Json) {
  const cases = fixture.cases.map((entry) => {
    let state: RunnerState = emptyRunnerState()
    const steps = entry.steps.map((step) => {
      const outcome = schedule(state, step.op, step.args ?? {}, step.at ?? 0)
      state = outcome.state
      return stepOut(step, outcome)
    })
    const jobs = Object.fromEntries(
      state.jobs.map((job) => [job.id, { status: job.status, conclusion: job.conclusion, runner: job.runner, flagged: job.flagged }])
    )
    return { name: entry.name, steps, expect: { jobs, run: runConclusion(state) } }
  })
  return { ...fixture, cases }
}

interface ChecksFixture extends Json {
  rollup: Array<{ name: string; checks: CheckRow[]; state?: string }>
  gate: Array<{ name: string; checks: CheckRow[]; required: string[]; result?: unknown }>
  checkName: Array<{ name: string; matrix: string[]; check?: string }>
}

export function replayChecks(fixture: ChecksFixture) {
  return {
    ...fixture,
    rollup: fixture.rollup.map((row) => ({ ...row, state: rollup(row.checks) })),
    gate: fixture.gate.map((row) => ({ ...row, result: mergeGate(row.checks, row.required) })),
    checkName: fixture.checkName.map((row) => ({ ...row, check: checkName(row.name, row.matrix) })),
  }
}

interface PipelineFixture extends Json {
  glob: Array<{ pattern: string; path: string; match?: boolean }>
  validate: Array<{ name: string; file: unknown; errors?: unknown }>
  plan: Array<{ name: string; file: unknown; trigger: TriggerInput; plan?: unknown }>
}

export function replayPipeline(fixture: PipelineFixture) {
  return {
    ...fixture,
    glob: fixture.glob.map((row) => ({ ...row, match: globMatch(row.pattern, row.path) })),
    validate: fixture.validate.map((row) => ({ ...row, errors: validatePipeline(row.file) })),
    plan: fixture.plan.map((row) => {
      const errors = validatePipeline(row.file)
      if (errors.length > 0) throw new Error(`plan case "${row.name}" has an invalid pipeline: ${JSON.stringify(errors)}`)
      return { ...row, plan: planPipeline(row.file, row.trigger) }
    }),
  }
}

interface RemoteFixture extends Json {
  parse: Array<{ url: string; remote?: unknown; helperKey?: unknown }>
  refresh: Array<{ expiresAt: number | null; now: number; refresh?: boolean }>
  redact: Array<{ text: string; secrets: string[]; out?: string }>
}

export function replayRemote(fixture: RemoteFixture) {
  return {
    ...fixture,
    parse: fixture.parse.map((row) => ({ ...row, remote: parseRemote(row.url), helperKey: credentialHelperKey(row.url) })),
    refresh: fixture.refresh.map((row) => ({ ...row, refresh: needsRefresh(row.expiresAt, row.now) })),
    redact: fixture.redact.map((row) => ({ ...row, out: redact(row.text, row.secrets) })),
  }
}

interface WorkflowFixture extends Json {
  cases: Array<{ name: string; workflow: unknown; file?: unknown; errors?: unknown }>
  text?: Array<{ name: string; yaml: string; file?: unknown; errors?: unknown }>
}


/** workflow.json: every case's {file, errors}; a file that reads must also
 *  be a VALID pipeline (pipeline.json `validate`). */
export function replayWorkflow(fixture: WorkflowFixture) {
  return {
    ...fixture,
    cases: fixture.cases.map((row) => {
      const read = readWorkflow(row.workflow)
      if (read.file) {
        const invalid = validatePipeline(read.file)
        if (invalid.length > 0) throw new Error(`workflow case "${row.name}" reads as an invalid pipeline: ${JSON.stringify(invalid)}`)
      }
      return { name: row.name, workflow: row.workflow, file: read.file, errors: read.errors }
    }),
    ...(fixture.text
      ? {
          text: fixture.text.map((row) => {
            const read = readWorkflowText(row.yaml)
            return { name: row.name, yaml: row.yaml, file: read.file, errors: read.errors }
          }),
        }
      : {}),
  }
}

export const REPLAYS = {
  forge: replayForge,
  runner: replayRunner,
  checks: replayChecks,
  pipeline: replayPipeline,
  remote: replayRemote,
  workflow: replayWorkflow,
} as const

export type FixtureName = keyof typeof REPLAYS
