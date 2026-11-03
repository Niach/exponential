// VAPP-194 (eApps E1, spec docs/spec/apps.md §4.5): typed effects. One
// expression language at two levels:
//
// - EXPRESSIONS ARE PURE (bindings, `visible`, computed fields, query
//   params): `validatePackage` refuses a write or host function in any of
//   them, with the path and the reason (`functionEffect`, src/cel.ts).
// - ACTIONS (`actions: {<name>: {params?, steps}}`) are the only place a
//   package writes. A Button's `on.press` names one as the function call
//   `actions.<name>` (its args = the params). The steps run IN ORDER against
//   ONE snapshot (the params, the surface data at the press, the member, the
//   clock): `require`, `records.upsert`, `records.delete`, `set` and `call`
//   (a host function from `functions`). Running them is pure: `runAction`
//   returns the PLAN (the record writes, the data sets, the host calls) or
//   the one error that discards the whole action. The host then commits the
//   writes as ONE unit (`RecordStore.batch`, one Postgres transaction on the
//   Exponential server), applies the sets, and runs the calls.
//
// The Rust core mirrors this file (`host::actions`); fixtures/actions.json
// locks the checks, the runs and the batch (TS, Rust, Swift, Kotlin).

import { EXPRESSION_LIMITS, isOptional, optionalType, checkExpression, evaluateCounted, effectWords, functionEffect, parseExpression, typeDisplay, typeName } from "../cel"
import type { ExprNode, ExprType, ExpressionEnv } from "../cel"
import type { PackageIssue } from "./package"
import { literalType, rowOf, schemaType } from "./expressions"
import { validateRecord, validateRecordSchema } from "./records"
import type { RecordSchema } from "./records"
import type { RecordRow } from "./query"
import { recordPolicy, recordRefusal, type AppAccessLevel } from "./record-policy"

/** Caps: actions per package, steps per action, and the evaluation steps
 *  ONE run may spend over all of its expressions (each expression keeps
 *  its own 10 000). */
export const ACTION_LIMITS = { actions: 32, steps: 64, maxSteps: 100_000 } as const
/** The step forms, by their one key. */
export const ACTION_STEP_KINDS: readonly string[] = [`require`, `records.upsert`, `records.delete`, `set`, `call`]
/** `^[a-z][A-Za-z0-9_]{0,31}$`: an action's name (`actions.<name>`). */
export const ACTION_NAME_PATTERN = /^[a-z][A-Za-z0-9_]{0,31}$/
/** The function-call prefix that runs a package action. */
export const ACTION_CALL_PREFIX = `actions.`
/** The host function a host commits an action's record writes through. */
export const RECORDS_BATCH_FUNCTION = `records.batch`

export type ActionWrite = { op: `upsert`; kind: string; data: Record<string, unknown>; id?: string } | { op: `delete`; id: string }

export interface ActionPlan {
  writes: ActionWrite[]
  sets: { path: string; value: unknown }[]
  calls: { name: string; args: Record<string, unknown> }[]
}

export type ActionResult = { plan: ActionPlan } | { error: string }

/** The snapshot an action runs against. */
export interface ActionContext {
  /** The surface data model at the press (the `data` var). */
  data?: unknown
  /** The member pressing (the `me` var; null = unknown). */
  me?: string | null
  /** The clock (epoch ms) and the team zone, as for expressions. */
  now?: number
  offset?: ExpressionEnv[`offset`]
  /** The run's evaluation budget (default `ACTION_LIMITS.maxSteps`). */
  maxSteps?: number
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)
const escape = (key: string) => key.replace(/~/g, `~0`).replace(/\//g, `~1`)
const POINTER = /^(\/([^/~]|~[01])*)*$/

/** The action a call name runs (`actions.log` → `log`), else null. */
export function actionCallName(call: string): string | null {
  return call.startsWith(ACTION_CALL_PREFIX) ? call.slice(ACTION_CALL_PREFIX.length) : null
}

/** A step's one key, or null. */
function stepKind(step: unknown): string | null {
  if (!isObject(step)) return null
  const keys = Object.keys(step).filter((k) => ACTION_STEP_KINDS.includes(k))
  return keys.length === 1 ? keys[0]! : null
}

/** The names a step expression reads, typed: `params` (the action's
 *  params schema), `data` (the surface data: open), `me` (a member id). */
export function actionVarTypes(action: Record<string, unknown>): Record<string, ExprType> {
  const schema = isObject(action.params) ? action.params : {}
  const params = rowOf(isObject(schema.properties) ? schema.properties : {}, schema.required)
  // VAPP-195: the surface data is untyped and the member may be unknown.
  return { params, data: optionalType(`dyn`), me: optionalType(`string`) }
}

/** May a value of type `t` land in a field of type `field`? */
function fits(field: ExprType, t: ExprType): boolean {
  // VAPP-195: a value that may be missing fits only a field that may be.
  if (isOptional(t)) return isOptional(field) && fits(field[`?`], t[`?`])
  if (isOptional(field)) return fits(field[`?`], t)
  if (field === `dyn` || t === `dyn` || t === `null`) return true
  const textual = (x: ExprType) => x === `string` || x === `date` || x === `timestamp`
  if (textual(field)) return textual(t)
  return typeDisplay(field) === typeDisplay(t)
}

// ---------------------------------------------------------------------------
// Validation (validatePackage)
// ---------------------------------------------------------------------------

/** Every problem with a package's `actions`, and every action call
 *  (`actions.<name>`) its templates make. */
export function validatePackageActions(pkg: Record<string, unknown>): PackageIssue[] {
  const issues: PackageIssue[] = []
  const actions = pkg.actions
  if (actions !== undefined) {
    if (!isObject(actions)) issues.push({ path: `/actions`, message: `actions is an object of named actions` })
    else {
      const names = Object.keys(actions).sort()
      if (names.length > ACTION_LIMITS.actions) issues.push({ path: `/actions`, message: `at most ${ACTION_LIMITS.actions} actions` })
      for (const name of names) issues.push(...validateAction(pkg, name, actions[name], `/actions/${escape(name)}`))
    }
  }
  return issues
}

function validateAction(pkg: Record<string, unknown>, name: string, action: unknown, at: string): PackageIssue[] {
  const issues: PackageIssue[] = []
  if (!ACTION_NAME_PATTERN.test(name)) issues.push({ path: at, message: `an action name is a letter, then letters, digits and underscores` })
  if (!isObject(action)) return [...issues, { path: at, message: `an action is {params?, steps}` }]
  if (action.params !== undefined) issues.push(...validateRecordSchema(action.params, `${at}/params`))
  if (!Array.isArray(action.steps) || action.steps.length === 0) return [...issues, { path: `${at}/steps`, message: `steps is a non-empty array` }]
  if (action.steps.length > ACTION_LIMITS.steps) issues.push({ path: `${at}/steps`, message: `at most ${ACTION_LIMITS.steps} steps` })
  const types = actionVarTypes(action)
  const records = isObject(pkg.records) ? pkg.records : {}
  const functions = Array.isArray(pkg.functions) ? (pkg.functions as unknown[]).filter((f): f is string => typeof f === `string`) : null
  action.steps.forEach((step, i) => issues.push(...validateStep(step, `${at}/steps/${i}`, types, records, functions)))
  return issues
}

/** An expression slot: a string = CEL (type-checked), anything else = a
 *  literal. The type, or null after an issue. */
function slot(value: unknown, at: string, types: Record<string, ExprType>, issues: PackageIssue[]): ExprType | null {
  if (typeof value !== `string`) return literalType(value)
  const checked = checkExpression(value, types)
  if (`error` in checked) {
    issues.push({ path: at, message: checked.error })
    return null
  }
  return checked.type
}

function validateStep(step: unknown, at: string, types: Record<string, ExprType>, records: Record<string, unknown>, functions: string[] | null): PackageIssue[] {
  const issues: PackageIssue[] = []
  const kind = stepKind(step)
  if (kind === null || !isObject(step)) return [{ path: at, message: `a step is one of ${ACTION_STEP_KINDS.join(`, `)}` }]
  const body = step[kind]
  const here = `${at}/${escape(kind)}`
  switch (kind) {
    case `require`: {
      if (typeof body !== `string`) issues.push({ path: here, message: `require is an expression` })
      else {
        const t = slot(body, here, types, issues)
        if (t !== null && t !== `bool` && t !== `dyn`) issues.push({ path: here, message: `require needs a bool, got ${typeDisplay(t)}` })
      }
      if (typeof step.message !== `string` || !step.message) issues.push({ path: `${at}/message`, message: `message is the text shown when the requirement fails` })
      return issues
    }
    case `records.upsert`: {
      if (!isObject(body)) return [{ path: here, message: `records.upsert is {kind, data, id?}` }]
      const kindName = typeof body.kind === `string` ? body.kind : ``
      const def = own(records, kindName) && isObject(records[kindName]) ? (records[kindName] as Record<string, unknown>) : null
      if (!def) issues.push({ path: `${here}/kind`, message: `records:${kindName} names no declared kind` })
      if (body.id !== undefined) {
        const t = slot(body.id, `${here}/id`, types, issues)
        if (t !== null && !fits(`string`, t)) issues.push({ path: `${here}/id`, message: `id is a string, got ${typeDisplay(t)}` })
      }
      if (!isObject(body.data)) return [...issues, { path: `${here}/data`, message: `data is an object of field expressions` }]
      const schema = def && isObject(def.schema) ? def.schema : null
      const props = schema && isObject(schema.properties) ? schema.properties : {}
      for (const field of Object.keys(body.data).sort()) {
        const fieldAt = `${here}/data/${escape(field)}`
        const t = slot(body.data[field], fieldAt, types, issues)
        if (!schema) continue
        if (!own(props, field)) {
          if (schema.additionalProperties !== true) issues.push({ path: fieldAt, message: `${field} is not in the schema of ${kindName}` })
          continue
        }
        const required = Array.isArray(schema.required) && schema.required.includes(field)
        const want = required ? schemaType(props[field]) : optionalType(schemaType(props[field]))
        if (t !== null && !fits(want, t)) issues.push({ path: fieldAt, message: `${field} is ${typeDisplay(want)}, got ${typeDisplay(t)}` })
      }
      // A create names every required field (a patch keeps the row's).
      if (schema && body.id === undefined && Array.isArray(schema.required))
        for (const field of schema.required) if (typeof field === `string` && !own(body.data, field)) issues.push({ path: `${here}/data`, message: `${field} is required by ${kindName}` })
      return issues
    }
    case `records.delete`: {
      if (!isObject(body) || body.id === undefined) return [{ path: here, message: `records.delete is {id}` }]
      const t = slot(body.id, `${here}/id`, types, issues)
      if (t !== null && !fits(`string`, t)) issues.push({ path: `${here}/id`, message: `id is a string, got ${typeDisplay(t)}` })
      return issues
    }
    case `set`: {
      if (!isObject(body) || typeof body.path !== `string` || !POINTER.test(body.path)) issues.push({ path: `${here}/path`, message: `path is a JSON pointer` })
      if (isObject(body)) slot(body.value ?? null, `${here}/value`, types, issues)
      return issues
    }
    case `call`: {
      if (typeof body !== `string` || !body) return [{ path: here, message: `call names a host function` }]
      const effect = functionEffect(body)
      if (effect === `write`) issues.push({ path: here, message: `${effectWords(body, effect)}: use a records.upsert, records.delete or set step` })
      else if (effect === `pure`) issues.push({ path: here, message: `${body} is pure: compute it in an expression` })
      else if (actionCallName(body) !== null) issues.push({ path: here, message: `an action never runs another action` })
      else if (functions !== null && !functions.some((p) => p === body || (p.endsWith(`*`) && body.startsWith(p.slice(0, -1)))))
        issues.push({ path: here, message: `${body} is not in the package's functions` })
      if (step.args !== undefined) {
        if (!isObject(step.args)) issues.push({ path: `${at}/args`, message: `args is an object of expressions` })
        else for (const k of Object.keys(step.args).sort()) slot(step.args[k], `${at}/args/${escape(k)}`, types, issues)
      }
      return issues
    }
  }
  return issues
}

/** VAPP-194: every function call in a template's PURE places (component
 *  props, a call's args, an event's context) is pure; an action's
 *  `functionCall` may name a host or write function, and an
 *  `actions.<name>` one names a declared action. */
export function validatePackageEffects(pkg: Record<string, unknown>): PackageIssue[] {
  const issues: PackageIssue[] = []
  if (!isObject(pkg.templates)) return issues
  const actions = isObject(pkg.actions) ? pkg.actions : {}
  const pure = (value: unknown, at: string) => {
    if (Array.isArray(value)) return value.forEach((v, i) => pure(v, `${at}/${i}`))
    if (!isObject(value)) return
    if (typeof value.call === `string`) {
      const effect = functionEffect(value.call)
      if (effect !== `pure`) issues.push({ path: `${at}/call`, message: `${effectWords(value.call, effect)}: a binding is pure, name an action in on.press` })
    }
    for (const [k, v] of Object.entries(value)) pure(v, `${at}/${escape(k)}`)
  }
  for (const [id, template] of Object.entries(pkg.templates)) {
    if (!isObject(template) || !Array.isArray(template.components)) continue
    template.components.forEach((c, i) => {
      if (!isObject(c)) return
      const at = `/templates/${escape(id)}/components/${i}`
      for (const [k, v] of Object.entries(c)) {
        if (k !== `on`) {
          pure(v, `${at}/${escape(k)}`)
          continue
        }
        if (!isObject(v)) continue
        for (const [event, handler] of Object.entries(v)) {
          const hat = `${at}/on/${escape(event)}`
          if (!isObject(handler)) continue
          for (const [hk, hv] of Object.entries(handler)) {
            if (hk !== `functionCall` || !isObject(hv)) {
              pure(hv, `${hat}/${escape(hk)}`)
              continue
            }
            const call = typeof hv.call === `string` ? hv.call : ``
            const action = actionCallName(call)
            if (action !== null && !own(actions, action)) issues.push({ path: `${hat}/functionCall/call`, message: `action ${action} names no package action` })
            for (const [fk, fv] of Object.entries(hv)) if (fk !== `call`) pure(fv, `${hat}/functionCall/${escape(fk)}`)
          }
        }
      }
    })
  }
  return issues
}

// ---------------------------------------------------------------------------
// Running (pure: the plan, or the one error)
// ---------------------------------------------------------------------------

class ActionFailure extends Error {}
const failAction = (message: string): never => {
  throw new ActionFailure(message)
}

const issueText = (issues: PackageIssue[]) => issues.map((i) => `${i.path || `/`} ${i.message}`).join(`; `)

/** Run a package action against one snapshot: its plan, or the error that
 *  discards it whole (a false `require` = its message; else `step <i>: …`). */
export function runAction(pkg: Record<string, unknown>, name: string, params: unknown, ctx: ActionContext = {}): ActionResult {
  const actions = isObject(pkg.actions) ? pkg.actions : {}
  const action = own(actions, name) && isObject(actions[name]) ? (actions[name] as Record<string, unknown>) : null
  if (!action || !Array.isArray(action.steps)) return { error: `no action ${name}` }
  const p = params === undefined || params === null ? {} : params
  if (!isObject(p)) return { error: `params is an object` }
  if (isObject(action.params)) {
    const issues = validateRecord(action.params as unknown as RecordSchema, p)
    if (issues.length) return { error: `params: ${issueText(issues)}` }
  }
  const vars: Record<string, unknown> = { params: p, data: ctx.data ?? null, me: ctx.me ?? null }
  const records = isObject(pkg.records) ? pkg.records : {}
  let budget = ctx.maxSteps ?? ACTION_LIMITS.maxSteps
  const plan: ActionPlan = { writes: [], sets: [], calls: [] }
  try {
    action.steps.forEach((step, i) => {
      const where = `step ${i}`
      const spend = (n: number) => {
        budget -= n
        if (budget < 0) failAction(`${where}: action step limit exceeded`)
      }
      spend(1)
      const value = (v: unknown): unknown => {
        if (typeof v !== `string`) return v
        const parsed = parseExpression(v)
        if (`error` in parsed) return failAction(`${where}: ${parsed.error}`)
        const cap = Math.min(EXPRESSION_LIMITS.maxSteps, budget)
        const { result, steps } = evaluateCounted(parsed.ast as ExprNode, vars, { now: ctx.now, offset: ctx.offset, maxSteps: cap })
        if (`error` in result) {
          if (result.error === `step limit exceeded` && cap === budget) failAction(`${where}: action step limit exceeded`)
          failAction(`${where}: ${result.error}`)
        }
        spend(steps)
        return (result as { value: unknown }).value
      }
      const kind = stepKind(step)
      if (kind === null || !isObject(step)) return failAction(`${where}: a step is one of ${ACTION_STEP_KINDS.join(`, `)}`)
      const body = step[kind]
      switch (kind) {
        case `require`: {
          const ok = value(body)
          if (typeof ok !== `boolean`) failAction(`${where}: require needs a bool, got ${typeName(ok)}`)
          if (!ok) failAction(typeof step.message === `string` && step.message ? step.message : `${where}: requirement failed`)
          return
        }
        case `records.upsert`: {
          if (!isObject(body) || !isObject(body.data)) return failAction(`${where}: records.upsert is {kind, data, id?}`)
          const kindName = typeof body.kind === `string` ? body.kind : ``
          const def = own(records, kindName) && isObject(records[kindName]) ? (records[kindName] as Record<string, unknown>) : null
          if (!def || !isObject(def.schema)) return failAction(`${where}: records: no kind ${kindName}`)
          let id: string | undefined
          if (body.id !== undefined) {
            const v = value(body.id)
            if (typeof v !== `string` || !v) failAction(`${where}: id is a string, got ${typeName(v)}`)
            id = v as string
          }
          const data: Record<string, unknown> = {}
          for (const field of Object.keys(body.data).sort()) data[field] = value(body.data[field])
          const schema = def.schema as unknown as RecordSchema
          // A patch is checked field by field (the store merges and checks the row).
          const issues = validateRecord(id === undefined ? schema : { ...schema, required: [] }, data)
          if (issues.length) failAction(`${where}: records: ${issueText(issues)}`)
          plan.writes.push(id === undefined ? { op: `upsert`, kind: kindName, data } : { op: `upsert`, kind: kindName, data, id })
          return
        }
        case `records.delete`: {
          if (!isObject(body)) return failAction(`${where}: records.delete is {id}`)
          const v = value(body.id)
          if (typeof v !== `string` || !v) failAction(`${where}: id is a string, got ${typeName(v)}`)
          plan.writes.push({ op: `delete`, id: v as string })
          return
        }
        case `set`: {
          if (!isObject(body) || typeof body.path !== `string` || !POINTER.test(body.path)) return failAction(`${where}: path is a JSON pointer`)
          plan.sets.push({ path: body.path, value: value(body.value ?? null) })
          return
        }
        case `call`: {
          if (typeof body !== `string` || functionEffect(body) !== `host` || actionCallName(body) !== null) return failAction(`${where}: call names a host function`)
          const args: Record<string, unknown> = {}
          if (isObject(step.args)) for (const k of Object.keys(step.args).sort()) args[k] = value(step.args[k])
          plan.calls.push({ name: body, args })
          return
        }
      }
    })
  } catch (e) {
    if (e instanceof ActionFailure) return { error: e.message }
    throw e
  }
  return { plan }
}

// ---------------------------------------------------------------------------
// The batch (one unit on a RecordStore)
// ---------------------------------------------------------------------------

export interface BatchOptions {
  /** The package's kinds (each write is checked against its schema). */
  records: Record<string, unknown> | undefined
  /** The writer (a create's `createdBy` and author). */
  me?: string | null
  /** eApps R3: the writer's team role; set = every write is judged by its
   *  kind's policy and `access` (record-policy.json), unset = not judged
   *  (the server judges). */
  role?: string | null
  /** The writer's app access (absent = write). */
  access?: AppAccessLevel
  /** The instant every row of the batch is stamped with. */
  now: string
  /** A new row's id (the `i`-th create of the batch). */
  newId: (i: number) => string
}

/** Apply a batch of writes to `rows`, all or nothing: the new rows and each
 *  write's id, or the first refusal (`write <i>: …`) with `rows` untouched.
 *  A patch merges onto its row and the merged row is checked. */
export function applyRecordBatch(rows: readonly RecordRow[], writes: readonly ActionWrite[], options: BatchOptions): { rows: RecordRow[]; ids: string[] } | { error: string } {
  let out = [...rows]
  const ids: string[] = []
  let creates = 0
  for (const [i, w] of writes.entries()) {
    const where = `write ${i}`
    const viewer = { id: options.me ?? null, role: options.role ?? null, access: options.access }
    const judge = (kind: string, op: `create` | `update` | `delete`, row?: RecordRow) => (options.role === undefined ? null : recordRefusal(recordPolicy(options.records?.[kind]), op, viewer, row))
    // A row the writer may not read is not there for them (no leak by refusal text).
    const find = (id: string) => out.find((r) => r.id === id && (options.role === undefined || recordRefusal(recordPolicy(options.records?.[r.kind]), `read`, viewer, r) === null))
    if (w.op === `delete`) {
      const row = find(w.id)
      if (!row) return { error: `${where}: record ${w.id} not found` }
      const refusal = judge(row.kind, `delete`, row)
      if (refusal) return { error: `${where}: ${refusal}` }
      out = out.filter((r) => r.id !== w.id)
      ids.push(w.id)
      continue
    }
    const def = options.records && own(options.records, w.kind) && isObject(options.records[w.kind]) ? (options.records[w.kind] as Record<string, unknown>) : null
    if (!def || !isObject(def.schema)) return { error: `${where}: records: no kind ${w.kind}` }
    const schema = def.schema as unknown as RecordSchema
    if (w.id !== undefined) {
      const base = find(w.id)
      if (!base) return { error: `${where}: record ${w.id} not found` }
      if (base.kind !== w.kind) return { error: `${where}: record ${w.id} is not of kind ${w.kind}` }
      const refusal = judge(w.kind, `update`, base)
      if (refusal) return { error: `${where}: ${refusal}` }
      const data = { ...base.data, ...w.data }
      const issues = validateRecord(schema, data)
      if (issues.length) return { error: `${where}: records: ${issueText(issues)}` }
      out = out.map((r) => (r.id === w.id ? { ...r, data, updatedAt: options.now } : r))
      ids.push(w.id)
      continue
    }
    const refusal = judge(w.kind, `create`)
    if (refusal) return { error: `${where}: ${refusal}` }
    const issues = validateRecord(schema, w.data)
    if (issues.length) return { error: `${where}: records: ${issueText(issues)}` }
    const id = options.newId(creates++)
    out.push({ id, kind: w.kind, data: { ...w.data }, createdBy: options.me ?? null, authorId: options.me ?? null, createdAt: options.now, updatedAt: options.now })
    ids.push(id)
  }
  return { rows: out, ids }
}
