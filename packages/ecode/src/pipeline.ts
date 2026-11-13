// eCode's pipeline file `.exponential/ci.json` (fixtures/pipeline.json is the
// spec): validation with JSON-pointer errors, the glob rule, and the planner
// that turns a pipeline plus a trigger into jobs (matrix expanded, ordered by
// `needs`, skipped by `paths` or for a fork). ecode-core ports this file.

import {
  isValidLabel,
  MATRIX_ENV_PREFIX,
  PIPELINE_DEFAULT_TIMEOUT_MIN,
  PIPELINE_MAX_JOBS,
  PIPELINE_MAX_MATRIX,
  PIPELINE_MAX_STEPS,
  PIPELINE_MAX_TIMEOUT_MIN,
  PIPELINE_NAME_MAX,
  PIPELINE_VERSION,
  triggerValues,
  type Skip,
  type Trigger,
} from "./contract.generated"
import { checkName } from "./checks"
import { byCodePoint } from "./forge"

// ── globs ───────────────────────────────────────────────────────────────

/** `**` = any run of path segments (`**\/` may match none), `*` = any run of
 *  characters but `/`, `?` = one character but `/`, the rest literal. */
export function globMatch(pattern: string, path: string): boolean {
  const p = Array.from(pattern)
  const s = Array.from(path)
  const go = (i: number, j: number): boolean => {
    if (i === p.length) return j === s.length
    if (p[i] === `*` && p[i + 1] === `*`) {
      let k = i
      while (p[k] === `*`) k += 1
      const slash = p[k] === `/`
      if (slash) k += 1
      for (let t = j; t <= s.length; t++) {
        if (slash && t !== j && s[t - 1] !== `/`) continue
        if (go(k, t)) return true
      }
      return false
    }
    if (p[i] === `*`) {
      for (let t = j; t <= s.length; t++) {
        if (t > j && s[t - 1] === `/`) break
        if (go(i + 1, t)) return true
      }
      return false
    }
    if (j === s.length) return false
    if (p[i] === `?`) return s[j] !== `/` && go(i + 1, j + 1)
    return p[i] === s[j] && go(i + 1, j + 1)
  }
  return go(0, 0)
}

export const isValidGlob = (glob: string): boolean => glob.length > 0 && !glob.startsWith(`/`)

// ── validation ──────────────────────────────────────────────────────────

export interface PipelineError {
  path: string
  code: string
}

type Json = unknown
type Obj = Record<string, Json>

const isObj = (v: Json): v is Obj => typeof v === `object` && v !== null && !Array.isArray(v)
const isInt = (v: Json): v is number => typeof v === `number` && Number.isInteger(v)
const ptr = (key: string) => `/${key.replaceAll(`~`, `~0`).replaceAll(`/`, `~1`)}`

const JOB_KEY = /^[a-z][a-z0-9_-]{0,63}$/
const ENV_NAME = /^[A-Z_][A-Z0-9_]*$/
const MATRIX_KEY = /^[a-z][a-z0-9_]*$/

const TOP_KEYS = [`$schema`, `version`, `name`, `on`, `env`, `secrets`, `jobs`, `required`]
const JOB_KEYS = [`name`, `runsOn`, `needs`, `timeoutMin`, `env`, `secrets`, `paths`, `matrix`, `steps`]
const STEP_KEYS = [`name`, `run`, `env`, `timeoutMin`, `continueOnError`, `workingDirectory`]
const TRIGGER_KEYS: Record<Trigger, string[]> = {
  push: [`branches`],
  pull_request: [`branches`],
  tag: [`tags`],
  merge_train: [],
  manual: [],
}

/** The matrix combinations in file order (first key outermost); [] = one plain job. */
export function matrixCombos(matrix: Json): Array<Array<[string, string]>> {
  if (!isObj(matrix)) return [[]]
  let combos: Array<Array<[string, string]>> = [[]]
  for (const [key, values] of Object.entries(matrix)) {
    if (!Array.isArray(values)) continue
    const strings = values.filter((v): v is string => typeof v === `string`)
    if (strings.length === 0) continue
    combos = combos.flatMap((combo) => strings.map((value) => [...combo, [key, value] as [string, string]]))
  }
  return combos
}

function jobDisplayName(key: string, job: Obj): string {
  return typeof job.name === `string` ? job.name : key
}

/** Every check name a job expands to. */
export function jobCheckNames(key: string, job: Json): string[] {
  if (!isObj(job)) return [key]
  return matrixCombos(job.matrix).map((combo) => checkName(jobDisplayName(key, job), combo.map(([, value]) => value)))
}

export function validatePipeline(file: Json): PipelineError[] {
  const errors: PipelineError[] = []
  const err = (path: string, code: string) => errors.push({ path, code })

  const unknownKeys = (obj: Obj, allowed: string[], at: string) => {
    for (const key of Object.keys(obj)) if (!allowed.includes(key)) err(`${at}${ptr(key)}`, `unknown_field`)
  }
  const text = (obj: Obj, key: string, at: string, required: boolean, max: number | null) => {
    const value = obj[key]
    if (value === undefined) {
      if (required) err(`${at}/${key}`, `missing`)
      return
    }
    if (typeof value !== `string`) return err(`${at}/${key}`, `type`)
    if (value.length === 0 || (max !== null && Array.from(value).length > max)) err(`${at}/${key}`, `invalid_value`)
  }
  const env = (value: Json, at: string) => {
    if (value === undefined) return
    if (!isObj(value)) return err(at, `type`)
    for (const [name, v] of Object.entries(value)) {
      if (!ENV_NAME.test(name)) err(`${at}${ptr(name)}`, `invalid_name`)
      else if (typeof v !== `string`) err(`${at}${ptr(name)}`, `type`)
    }
  }
  const names = (value: Json, at: string) => {
    if (value === undefined) return
    if (!Array.isArray(value)) return err(at, `type`)
    value.forEach((v, i) => {
      if (typeof v !== `string`) err(`${at}/${i}`, `type`)
      else if (!ENV_NAME.test(v)) err(`${at}/${i}`, `invalid_name`)
    })
  }
  const globs = (value: Json, at: string, nonEmpty: boolean) => {
    if (value === undefined) return
    if (!Array.isArray(value)) return err(at, `type`)
    if (nonEmpty && value.length === 0) return err(at, `invalid_value`)
    value.forEach((v, i) => {
      if (typeof v !== `string`) err(`${at}/${i}`, `type`)
      else if (!isValidGlob(v)) err(`${at}/${i}`, `invalid_glob`)
    })
  }
  const timeout = (value: Json, at: string) => {
    if (value === undefined) return
    if (!isInt(value)) return err(at, `type`)
    if (value < 1 || value > PIPELINE_MAX_TIMEOUT_MIN) err(at, `invalid_value`)
  }

  if (!isObj(file)) return [{ path: ``, code: `type` }]
  unknownKeys(file, TOP_KEYS, ``)

  if (file.version === undefined) err(`/version`, `missing`)
  else if (!isInt(file.version)) err(`/version`, `type`)
  else if (file.version !== PIPELINE_VERSION) err(`/version`, `invalid_value`)
  text(file, `name`, ``, false, PIPELINE_NAME_MAX)

  const on = file.on
  if (on === undefined) err(`/on`, `missing`)
  else if (!isObj(on)) err(`/on`, `type`)
  else if (Object.keys(on).length === 0) err(`/on`, `invalid_value`)
  else {
    for (const [kind, filter] of Object.entries(on)) {
      const at = `/on${ptr(kind)}`
      if (!(triggerValues as readonly string[]).includes(kind)) {
        err(at, `unknown_field`)
        continue
      }
      if (!isObj(filter)) {
        err(at, `type`)
        continue
      }
      const allowed = TRIGGER_KEYS[kind as Trigger]
      unknownKeys(filter, allowed, at)
      for (const key of allowed) globs(filter[key], `${at}/${key}`, false)
    }
  }

  env(file.env, `/env`)
  names(file.secrets, `/secrets`)

  const jobs = file.jobs
  let needsSound = true
  if (jobs === undefined) err(`/jobs`, `missing`)
  else if (!isObj(jobs)) err(`/jobs`, `type`)
  else if (Object.keys(jobs).length === 0) err(`/jobs`, `invalid_value`)
  else {
    let expanded = 0
    for (const [key, job] of Object.entries(jobs)) {
      const at = `/jobs${ptr(key)}`
      if (!JOB_KEY.test(key)) err(at, `invalid_name`)
      if (!isObj(job)) {
        err(at, `type`)
        needsSound = false
        expanded += 1
        continue
      }
      unknownKeys(job, JOB_KEYS, at)
      text(job, `name`, at, false, PIPELINE_NAME_MAX)

      if (job.runsOn === undefined) err(`${at}/runsOn`, `missing`)
      else if (!Array.isArray(job.runsOn)) err(`${at}/runsOn`, `type`)
      else if (job.runsOn.length === 0) err(`${at}/runsOn`, `invalid_value`)
      else
        job.runsOn.forEach((label, i) => {
          if (typeof label !== `string`) err(`${at}/runsOn/${i}`, `type`)
          else if (!isValidLabel(label)) err(`${at}/runsOn/${i}`, `invalid_label`)
        })

      if (job.needs !== undefined) {
        if (!Array.isArray(job.needs)) {
          err(`${at}/needs`, `type`)
          needsSound = false
        } else
          job.needs.forEach((need, i) => {
            if (typeof need !== `string`) {
              err(`${at}/needs/${i}`, `type`)
              needsSound = false
            } else if (!Object.hasOwn(jobs, need)) {
              err(`${at}/needs/${i}`, `unknown_need`)
              needsSound = false
            }
          })
      }

      timeout(job.timeoutMin, `${at}/timeoutMin`)
      env(job.env, `${at}/env`)
      names(job.secrets, `${at}/secrets`)
      globs(job.paths, `${at}/paths`, true)

      const matrix = job.matrix
      if (matrix !== undefined) {
        if (!isObj(matrix)) err(`${at}/matrix`, `type`)
        else if (Object.keys(matrix).length === 0) err(`${at}/matrix`, `invalid_value`)
        else {
          for (const [mkey, values] of Object.entries(matrix)) {
            const mat = `${at}/matrix${ptr(mkey)}`
            if (!MATRIX_KEY.test(mkey)) err(mat, `invalid_name`)
            if (!Array.isArray(values)) err(mat, `type`)
            else if (values.length === 0) err(mat, `invalid_value`)
            else values.forEach((v, i) => typeof v !== `string` && err(`${mat}/${i}`, `type`))
          }
          if (matrixCombos(matrix).length > PIPELINE_MAX_MATRIX) err(`${at}/matrix`, `limit`)
        }
      }
      expanded += matrixCombos(matrix).length

      const steps = job.steps
      if (steps === undefined) err(`${at}/steps`, `missing`)
      else if (!Array.isArray(steps)) err(`${at}/steps`, `type`)
      else if (steps.length === 0) err(`${at}/steps`, `invalid_value`)
      else {
        if (steps.length > PIPELINE_MAX_STEPS) err(`${at}/steps`, `limit`)
        steps.forEach((step, i) => {
          const sat = `${at}/steps/${i}`
          if (!isObj(step)) return err(sat, `type`)
          unknownKeys(step, STEP_KEYS, sat)
          text(step, `name`, sat, true, PIPELINE_NAME_MAX)
          text(step, `run`, sat, true, null)
          env(step.env, `${sat}/env`)
          timeout(step.timeoutMin, `${sat}/timeoutMin`)
          if (step.continueOnError !== undefined && typeof step.continueOnError !== `boolean`) err(`${sat}/continueOnError`, `type`)
          const dir = step.workingDirectory
          if (dir !== undefined) {
            if (typeof dir !== `string`) err(`${sat}/workingDirectory`, `type`)
            else if (dir.length === 0 || dir.startsWith(`/`) || dir.split(`/`).includes(`..`)) err(`${sat}/workingDirectory`, `invalid_value`)
          }
        })
      }
    }
    if (expanded > PIPELINE_MAX_JOBS) err(`/jobs`, `limit`)
    if (needsSound && hasCycle(jobs)) err(`/jobs`, `cycle`)
  }

  const required = file.required
  if (required !== undefined) {
    if (!Array.isArray(required)) err(`/required`, `type`)
    else {
      const known = isObj(jobs) ? Object.entries(jobs).flatMap(([key, job]) => jobCheckNames(key, job)) : []
      required.forEach((name, i) => {
        if (typeof name !== `string`) err(`/required/${i}`, `type`)
        else if (!known.includes(name)) err(`/required/${i}`, `unknown_required`)
      })
    }
  }

  return errors.sort((a, b) => byCodePoint(a.path, b.path) || byCodePoint(a.code, b.code))
}

function needsOf(job: Json): string[] {
  return isObj(job) && Array.isArray(job.needs) ? job.needs.filter((n): n is string => typeof n === `string`) : []
}

function hasCycle(jobs: Obj): boolean {
  const state = new Map<string, `visiting` | `done`>()
  const visit = (key: string): boolean => {
    if (state.get(key) === `done`) return false
    if (state.get(key) === `visiting`) return true
    state.set(key, `visiting`)
    for (const need of needsOf(jobs[key])) if (visit(need)) return true
    state.set(key, `done`)
    return false
  }
  return Object.keys(jobs).some(visit)
}

// ── the planner ─────────────────────────────────────────────────────────

export interface TriggerInput {
  kind: Trigger
  /** push: the pushed branch. */
  branch?: string | null
  /** pull_request: the PR's base branch. */
  base?: string | null
  /** tag: the tag name. */
  tag?: string | null
  /** Changed paths; absent = unknown (every job runs). */
  changedFiles?: string[] | null
  /** pull_request from outside the team. */
  fork?: boolean
}

export interface PlannedStep {
  name: string
  run: string
  env: Record<string, string>
  timeoutMin: number | null
  continueOnError: boolean
  workingDirectory: string | null
}

export interface PlannedJob {
  check: string
  job: string
  matrix: Record<string, string>
  runsOn: string[]
  needs: string[]
  timeoutMin: number
  env: Record<string, string>
  secrets: string[]
  skip: Skip | null
  steps: PlannedStep[]
}

export interface Plan {
  name: string
  jobs: PlannedJob[]
}

const strings = (v: Json): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === `string`) : [])
const stringMap = (v: Json): Record<string, string> =>
  isObj(v) ? Object.fromEntries(Object.entries(v).filter((e): e is [string, string] => typeof e[1] === `string`)) : {}
const sortedMap = (m: Record<string, string>): Record<string, string> =>
  Object.fromEntries(Object.entries(m).sort(([a], [b]) => byCodePoint(a, b)))

function filterMatches(filter: Obj, key: string, value: string | null | undefined): boolean {
  if (filter[key] === undefined) return true
  if (typeof value !== `string`) return false
  return strings(filter[key]).some((glob) => globMatch(glob, value))
}

/** The jobs a VALID pipeline runs for a trigger, null = not triggered. */
export function planPipeline(file: Json, trigger: TriggerInput): Plan | null {
  if (!isObj(file) || !isObj(file.on) || !isObj(file.jobs)) return null
  const filter = file.on[trigger.kind]
  if (!isObj(filter)) return null
  if (trigger.kind === `push` && !filterMatches(filter, `branches`, trigger.branch)) return null
  if (trigger.kind === `pull_request` && !filterMatches(filter, `branches`, trigger.base)) return null
  if (trigger.kind === `tag` && !filterMatches(filter, `tags`, trigger.tag)) return null

  const jobs = file.jobs
  const order: string[] = []
  const keys = Object.keys(jobs)
  while (order.length < keys.length) {
    const next = keys.find((key) => !order.includes(key) && needsOf(jobs[key]).every((need) => order.includes(need)))
    if (next === undefined) return null
    order.push(next)
  }

  const topEnv = stringMap(file.env)
  const topSecrets = strings(file.secrets)
  const planned: PlannedJob[] = []
  for (const key of order) {
    const job = jobs[key] as Obj
    let skip: Skip | null = null
    if (trigger.kind === `pull_request` && trigger.fork === true) skip = `fork`
    else if (Array.isArray(trigger.changedFiles) && job.paths !== undefined) {
      const globsOf = strings(job.paths)
      if (!trigger.changedFiles.some((path) => globsOf.some((glob) => globMatch(glob, path)))) skip = `paths`
    }
    const needs = needsOf(job).flatMap((need) => jobCheckNames(need, jobs[need]))
    for (const combo of matrixCombos(job.matrix)) {
      const matrixEnv = Object.fromEntries(combo.map(([k, v]) => [`${MATRIX_ENV_PREFIX}${k.toUpperCase()}`, v]))
      const env = { ...topEnv, ...stringMap(job.env), ...matrixEnv }
      const steps = (Array.isArray(job.steps) ? job.steps : []).filter(isObj).map((step) => ({
        name: String(step.name),
        run: String(step.run),
        env: sortedMap({ ...env, ...stringMap(step.env) }),
        timeoutMin: isInt(step.timeoutMin) ? step.timeoutMin : null,
        continueOnError: step.continueOnError === true,
        workingDirectory: typeof step.workingDirectory === `string` ? step.workingDirectory : null,
      }))
      planned.push({
        check: checkName(jobDisplayName(key, job), combo.map(([, v]) => v)),
        job: key,
        matrix: Object.fromEntries(combo),
        runsOn: strings(job.runsOn),
        needs,
        timeoutMin: isInt(job.timeoutMin) ? job.timeoutMin : PIPELINE_DEFAULT_TIMEOUT_MIN,
        env: sortedMap(env),
        secrets: [...new Set([...topSecrets, ...strings(job.secrets)])].sort(byCodePoint),
        skip,
        steps,
      })
    }
  }
  return { name: typeof file.name === `string` ? file.name : `ci`, jobs: planned }
}
