// eCode's GitHub workflow reader (fixtures/workflow.json is the spec,
// #EXP-1617): a GitHub Actions workflow, already PARSED from YAML into a JSON
// tree, read into the SAME model as `.exponential/ci.json` (pipeline.ts), or
// every reason it cannot be, as JSON-pointer errors into the workflow. A
// subset on purpose: what is not listed is an error, never silently dropped.
// ecode-core ports this file; YAML text parsing stays with the host.

import { isValidLabel, MATRIX_ENV_PREFIX, PIPELINE_NAME_MAX, PIPELINE_VERSION } from "./contract.generated"
import { byCodePoint } from "./forge"
import type { PipelineError } from "./pipeline"

type Json = unknown
type Obj = Record<string, Json>

const isObj = (v: Json): v is Obj => typeof v === `object` && v !== null && !Array.isArray(v)
const isInt = (v: Json): v is number => typeof v === `number` && Number.isInteger(v)
const ptr = (key: string) => `/${key.replaceAll(`~`, `~0`).replaceAll(`/`, `~1`)}`

const JOB_KEY = /^[a-z][a-z0-9_-]{0,63}$/
const ENV_NAME = /^[A-Z_][A-Z0-9_]*$/
const MATRIX_KEY = /^[a-z][a-z0-9_]*$/
const EXPRESSION = /\$\{\{\s*([^}]*?)\s*\}\}/g

/** Keys read, keys accepted and ignored (they mean nothing to our runner). */
const TOP_KEYS = [`name`, `on`, `env`, `jobs`]
const TOP_IGNORED = [`permissions`, `concurrency`, `run-name`]
const JOB_KEYS = [`name`, `runs-on`, `needs`, `timeout-minutes`, `env`, `strategy`, `steps`]
const JOB_IGNORED = [`permissions`, `concurrency`]
const STRATEGY_KEYS = [`matrix`]
const STRATEGY_IGNORED = [`fail-fast`, `max-parallel`]
const STEP_KEYS = [`name`, `run`, `env`, `timeout-minutes`, `continue-on-error`, `working-directory`, `shell`, `uses`, `with`]
const STEP_IGNORED = [`id`]
const SHELLS = [`bash`, `sh`]
/** `uses:` our runner covers itself: the checkout (it checks out the job's
 *  commit) and the cache (a no-op until runner caches exist). */
const ACTIONS = [`actions/checkout`, `actions/cache`]
const EVENT_KEYS: Record<string, string[]> = {
  push: [`branches`, `tags`, `paths`],
  pull_request: [`branches`, `paths`, `types`],
  workflow_dispatch: [`inputs`],
  merge_group: [`types`],
}

export interface WorkflowRead {
  /** The `.exponential/ci.json` it reads as; null when there are errors. */
  file: Obj | null
  errors: PipelineError[]
}

const strings = (v: Json): string[] | null =>
  typeof v === `string` ? [v] : Array.isArray(v) && v.every((x) => typeof x === `string`) ? (v as string[]) : null

const scalar = (v: Json): string | null =>
  typeof v === `string` ? v : typeof v === `number` || typeof v === `boolean` ? String(v) : null

/** A GitHub `runs-on` label as ours: runner-image names become their OS. */
function label(raw: string): string | null {
  const l = raw.trim().toLowerCase()
  if (l === `self-hosted`) return ``
  if (l.startsWith(`ubuntu`) || l === `linux`) return `linux`
  if (l.startsWith(`macos`) || l === `macos` || l === `mac`) return `mac`
  if (l.startsWith(`windows`)) return `windows`
  return isValidLabel(l) ? l : null
}

/** Reads a GitHub workflow (parsed YAML) into a pipeline file. */
export function readWorkflow(workflow: Json): WorkflowRead {
  const errors: PipelineError[] = []
  const err = (path: string, code: string) => errors.push({ path, code })
  const unknown = (o: Obj, known: string[], ignored: string[], at: string) => {
    for (const k of Object.keys(o)) if (!known.includes(k) && !ignored.includes(k)) err(`${at}${ptr(k)}`, `unknown_field`)
  }

  if (!isObj(workflow)) return { file: null, errors: [{ path: ``, code: `type` }] }
  unknown(workflow, TOP_KEYS, TOP_IGNORED, ``)

  // Expressions: only `${{ matrix.<key> }}`, `${{ env.<NAME> }}` and
  // `${{ secrets.<NAME> }}`; in a `run`, they become shell variables.
  const runText = (text: string, at: string, matrixKeys: string[], secrets: Set<string>): string =>
    text.replace(EXPRESSION, (whole, expr: string) => {
      const [scope, name, ...rest] = expr.split(`.`)
      if (rest.length === 0 && name !== undefined) {
        if (scope === `matrix` && matrixKeys.includes(name)) return `\${${MATRIX_ENV_PREFIX}${name.toUpperCase()}}`
        if ((scope === `env` || scope === `secrets`) && ENV_NAME.test(name)) {
          if (scope === `secrets`) secrets.add(name)
          return `\${${name}}`
        }
      }
      err(at, `expression`)
      return whole
    })
  const noExpression = (text: string, at: string): boolean => {
    if (/\$\{\{/.test(text)) {
      err(at, `expression`)
      return false
    }
    return true
  }
  // An env map: literal values; `NAME: ${{ secrets.NAME }}` is our secret NAME.
  const envMap = (v: Json, at: string, secrets: Set<string>): Record<string, string> | undefined => {
    if (v === undefined) return undefined
    if (!isObj(v)) {
      err(at, `type`)
      return undefined
    }
    const env: Record<string, string> = {}
    for (const [k, raw] of Object.entries(v)) {
      const here = `${at}${ptr(k)}`
      if (!ENV_NAME.test(k)) err(here, `invalid_name`)
      const value = scalar(raw)
      if (value === null) {
        err(here, `type`)
        continue
      }
      const secret = value.match(/^\$\{\{\s*secrets\.([A-Z_][A-Z0-9_]*)\s*\}\}$/)
      if (secret) {
        if (secret[1] === k) secrets.add(k)
        else err(here, `expression`)
        continue
      }
      if (noExpression(value, here)) env[k] = value
    }
    return Object.keys(env).length > 0 ? Object.fromEntries(Object.entries(env).sort(([a], [b]) => byCodePoint(a, b))) : undefined
  }

  // ── name ──
  let name: string | undefined
  if (workflow.name !== undefined) {
    if (typeof workflow.name !== `string`) err(`/name`, `type`)
    else if (noExpression(workflow.name, `/name`)) name = workflow.name.slice(0, PIPELINE_NAME_MAX)
  }

  // ── on ──
  const on: Obj = {}
  let paths: string[] | undefined
  if (workflow.on === undefined) err(`/on`, `missing`)
  else {
    const events: Array<[string, Json]> =
      typeof workflow.on === `string`
        ? [[workflow.on, null]]
        : Array.isArray(workflow.on)
          ? workflow.on.map((e) => [String(e), null] as [string, Json])
          : isObj(workflow.on)
            ? Object.entries(workflow.on)
            : []
    if (events.length === 0) err(`/on`, `type`)
    for (const [event, body] of events) {
      const at = `/on${ptr(event)}`
      const known = EVENT_KEYS[event]
      if (!known) {
        err(at, `unsupported_event`)
        continue
      }
      if (body !== null && body !== undefined && !isObj(body)) {
        err(at, `type`)
        continue
      }
      const o = (body ?? {}) as Obj
      unknown(o, known, [], at)
      const filter = (key: string): string[] | undefined => {
        if (o[key] === undefined) return undefined
        const list = strings(o[key])
        if (!list) err(`${at}/${key}`, `type`)
        return list ?? undefined
      }
      if (o.paths !== undefined) {
        const p = filter(`paths`)
        if (p && paths && JSON.stringify(p) !== JSON.stringify(paths)) err(`${at}/paths`, `invalid_value`)
        if (p) paths = p
      }
      if (event === `push`) {
        const branches = filter(`branches`)
        const tags = filter(`tags`)
        // GitHub: a tags-only filter does not fire on branch pushes.
        if (branches !== undefined || tags === undefined) on.push = branches ? { branches } : {}
        if (tags !== undefined) on.tag = { tags }
      } else if (event === `pull_request`) {
        const branches = filter(`branches`)
        on.pull_request = branches ? { branches } : {}
      } else if (event === `workflow_dispatch`) on.manual = {}
      else if (event === `merge_group`) on.merge_train = {}
    }
  }

  // ── env ──
  const topSecrets = new Set<string>()
  const topEnv = envMap(workflow.env, `/env`, topSecrets)

  // ── jobs ──
  const jobs: Obj = {}
  if (!isObj(workflow.jobs)) err(`/jobs`, workflow.jobs === undefined ? `missing` : `type`)
  else {
    const ids = new Map<string, string>()
    for (const id of Object.keys(workflow.jobs)) {
      const key = id.toLowerCase()
      if (!JOB_KEY.test(key)) err(`/jobs${ptr(id)}`, `invalid_name`)
      else if ([...ids.values()].includes(key)) err(`/jobs${ptr(id)}`, `invalid_name`)
      ids.set(id, key)
    }
    for (const [id, raw] of Object.entries(workflow.jobs)) {
      const at = `/jobs${ptr(id)}`
      if (!isObj(raw)) {
        err(at, `type`)
        continue
      }
      unknown(raw, JOB_KEYS, JOB_IGNORED, at)
      const job: Obj = {}
      const secrets = new Set<string>()

      if (raw.name !== undefined) {
        if (typeof raw.name !== `string`) err(`${at}/name`, `type`)
        else if (noExpression(raw.name, `${at}/name`)) job.name = raw.name
      }

      const runsOn = strings(raw[`runs-on`])
      if (raw[`runs-on`] === undefined) err(`${at}/runs-on`, `missing`)
      else if (!runsOn) err(`${at}/runs-on`, `type`)
      else {
        const labels: string[] = []
        for (const l of runsOn) {
          if (!noExpression(l, `${at}/runs-on`)) continue
          const ours = label(l)
          if (ours === null) err(`${at}/runs-on`, `invalid_label`)
          else if (ours !== `` && !labels.includes(ours)) labels.push(ours)
        }
        job.runsOn = labels
      }

      if (raw.needs !== undefined) {
        const needs = strings(raw.needs)
        if (!needs) err(`${at}/needs`, `type`)
        else job.needs = needs.map((n) => ids.get(n) ?? n.toLowerCase())
      }

      if (raw[`timeout-minutes`] !== undefined) {
        if (!isInt(raw[`timeout-minutes`])) err(`${at}/timeout-minutes`, `type`)
        else job.timeoutMin = raw[`timeout-minutes`]
      }

      const env = envMap(raw.env, `${at}/env`, secrets)
      if (env) job.env = env

      let matrixKeys: string[] = []
      if (raw.strategy !== undefined) {
        if (!isObj(raw.strategy)) err(`${at}/strategy`, `type`)
        else {
          unknown(raw.strategy, STRATEGY_KEYS, STRATEGY_IGNORED, `${at}/strategy`)
          const m = raw.strategy.matrix
          if (m !== undefined) {
            if (!isObj(m)) err(`${at}/strategy/matrix`, `type`)
            else {
              const matrix: Record<string, string[]> = {}
              for (const [k, values] of Object.entries(m)) {
                const here = `${at}/strategy/matrix${ptr(k)}`
                if (k === `include` || k === `exclude`) {
                  err(here, `unknown_field`)
                  continue
                }
                if (!MATRIX_KEY.test(k)) err(here, `invalid_name`)
                const list = Array.isArray(values) ? values.map(scalar) : null
                if (!list || list.some((v) => v === null)) err(here, `type`)
                else matrix[k] = list as string[]
              }
              matrixKeys = Object.keys(matrix)
              if (matrixKeys.length > 0) job.matrix = matrix
            }
          }
        }
      }

      const steps: Obj[] = []
      if (!Array.isArray(raw.steps)) err(`${at}/steps`, raw.steps === undefined ? `missing` : `type`)
      else {
        raw.steps.forEach((s, i) => {
          const sat = `${at}/steps/${i}`
          if (!isObj(s)) {
            err(sat, `type`)
            return
          }
          unknown(s, STEP_KEYS, STEP_IGNORED, sat)
          if (s.uses !== undefined) {
            const action = typeof s.uses === `string` ? s.uses.split(`@`)[0]!.toLowerCase() : null
            if (!action || !ACTIONS.includes(action)) err(`${sat}/uses`, `unsupported_action`)
            if (s.run !== undefined) err(`${sat}/run`, `invalid_value`)
            return
          }
          if (s.with !== undefined) err(`${sat}/with`, `unknown_field`)
          if (typeof s.run !== `string`) {
            err(`${sat}/run`, s.run === undefined ? `missing` : `type`)
            return
          }
          if (s.shell !== undefined && !(typeof s.shell === `string` && SHELLS.includes(s.shell))) err(`${sat}/shell`, `invalid_value`)
          const run = runText(s.run, `${sat}/run`, matrixKeys, secrets)
          const step: Obj = {}
          if (s.name !== undefined) {
            if (typeof s.name !== `string`) err(`${sat}/name`, `type`)
            else if (noExpression(s.name, `${sat}/name`)) step.name = s.name
          }
          step.name ??= run.split(`\n`).find((line) => line.trim() !== ``)?.trim().slice(0, 64) ?? `run`
          step.run = run
          const senv = envMap(s.env, `${sat}/env`, secrets)
          if (senv) step.env = senv
          if (s[`timeout-minutes`] !== undefined) {
            if (!isInt(s[`timeout-minutes`])) err(`${sat}/timeout-minutes`, `type`)
            else step.timeoutMin = s[`timeout-minutes`]
          }
          if (s[`continue-on-error`] !== undefined) {
            if (typeof s[`continue-on-error`] !== `boolean`) err(`${sat}/continue-on-error`, `type`)
            else step.continueOnError = s[`continue-on-error`]
          }
          if (s[`working-directory`] !== undefined) {
            if (typeof s[`working-directory`] !== `string`) err(`${sat}/working-directory`, `type`)
            else if (noExpression(s[`working-directory`], `${sat}/working-directory`)) step.workingDirectory = s[`working-directory`]
          }
          steps.push(step)
        })
      }
      job.steps = steps
      if (paths) job.paths = paths
      if (secrets.size > 0) job.secrets = [...secrets].sort(byCodePoint)
      jobs[ids.get(id) ?? id.toLowerCase()] = job
    }
  }

  errors.sort((a, b) => byCodePoint(a.path, b.path) || byCodePoint(a.code, b.code))
  const unique = errors.filter((e, i) => i === 0 || e.path !== errors[i - 1]!.path || e.code !== errors[i - 1]!.code)
  if (unique.length > 0) return { file: null, errors: unique }
  const file: Obj = { version: PIPELINE_VERSION }
  if (name !== undefined) file.name = name
  file.on = on
  if (topEnv) file.env = topEnv
  if (topSecrets.size > 0) file.secrets = [...topSecrets].sort(byCodePoint)
  file.jobs = jobs
  return { file, errors: [] }
}
