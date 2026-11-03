// VAPP-191 (eApps Q2): expressions are type-checked at package validation.
// Every `{"call": "expr", "args": {"expr": "<CEL>", "vars": {…}}}` anywhere
// in a template is parsed and checked (src/cel.ts) against the types of its
// vars: a literal's own type, a records binding's value (the kind's schema
// fields) or the template data's; anything else is open (`dyn`). An issue
// names the JSON pointer of the expression and the reason, so the editor
// agent can fix it. The Rust core's `host::expressions` mirrors it
// (`fixtures/expressions.json` `packages`).

import { checkExpression, expressionNames, isOptional, optionalType, parseExpression, requiredType, typeName } from "../cel"
import type { ExprType } from "../cel"
import { isBinding, isCall } from "../expr"
import { RECORDS_COMPUTED_MAX, RECORDS_COMPUTED_ROW_KEYS, RECORDS_ROW_KEYS, RECORDS_SCHEME, expandQuerySource, parseRecordsSource } from "./query"
import type { RecordsQuery } from "./query"
import { parseSource } from "./sources"

export interface ExpressionIssue {
  path: string
  message: string
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)
const escape = (key: string) => key.replace(/~/g, `~0`).replace(/\//g, `~1`)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)

/** A JSON Schema property → its expression type. */
export function schemaType(schema: unknown): ExprType {
  if (!isObject(schema)) return `dyn`
  const types = Array.isArray(schema.type) ? schema.type.filter((t) => t !== `null`) : [schema.type]
  const type = types.length === 1 ? types[0] : undefined
  if (type === `number` || type === `integer`) return `number`
  if (type === `boolean`) return `bool`
  if (type === `string`) return schema.format === `date` ? `date` : schema.format === `date-time` ? `timestamp` : `string`
  if (type === `array`) return [schemaType(schema.items)]
  if (type === `object`) return isObject(schema.properties) ? rowOf(schema.properties, schema.required) : `map`
  return `dyn`
}

/** VAPP-195 (E2): a schema's fields; one not in `required` may be missing
 *  (`T?`). */
export function rowOf(properties: Record<string, unknown>, required: unknown): ExprType {
  const names = Array.isArray(required) ? required : []
  const out: Record<string, ExprType> = {}
  for (const [k, v] of Object.entries(properties)) out[k] = names.includes(k) ? schemaType(v) : optionalType(schemaType(v))
  return out
}

/** A record row of a kind: its schema's fields (`T?` unless required) plus
 *  the row keys (always there). VAPP-214: plus its computed fields, each
 *  its expression's type (`dyn?` when it does not check). */
export function recordRowType(kind: unknown): ExprType {
  const schema = isObject(kind) && isObject(kind.schema) ? kind.schema : {}
  const props = isObject(schema.properties) ? schema.properties : {}
  const row: Record<string, ExprType> = { ...(rowOf(props, schema.required) as Record<string, ExprType>), id: `string`, createdAt: `timestamp`, updatedAt: `timestamp`, createdBy: `string`, authorId: `string`, author: `string` }
  const computed = isObject(kind) && isObject(kind.computed) ? kind.computed : {}
  const vars = computedVars(kind)
  for (const name of Object.keys(computed).sort()) {
    if (own(row, name)) continue
    const source = computed[name]
    const checked = typeof source === `string` ? checkExpression(source, vars) : null
    row[name] = checked && `type` in checked ? checked.type : optionalType(`dyn`)
  }
  return row
}

/** VAPP-214: the names a kind's computed field reads: its schema's fields
 *  (`T?` unless required) and the row keys but `author`. */
function computedVars(kind: unknown): Record<string, ExprType> {
  const schema = isObject(kind) && isObject(kind.schema) ? kind.schema : {}
  const props = isObject(schema.properties) ? schema.properties : {}
  const vars = { ...(rowOf(props, schema.required) as Record<string, ExprType>) }
  for (const key of RECORDS_COMPUTED_ROW_KEYS) vars[key] = key === `createdAt` || key === `updatedAt` ? `timestamp` : `string`
  return vars
}

/** VAPP-214 (Q3): a kind's `computed` ({name: CEL}), each issue at
 *  `<at>/computed/<name>`: at most `RECORDS_COMPUTED_MAX`; a name is not a
 *  schema field nor a row key; the expression is a string that reads no
 *  other computed field and type-checks against the kind's fields. */
export function validateRecordComputed(kind: unknown, at: string): ExpressionIssue[] {
  if (!isObject(kind) || kind.computed === undefined) return []
  const computed = kind.computed
  if (!isObject(computed)) return [{ path: `${at}/computed`, message: `computed is {name: expression}` }]
  const issues: ExpressionIssue[] = []
  const names = Object.keys(computed).sort()
  if (names.length > RECORDS_COMPUTED_MAX) issues.push({ path: `${at}/computed`, message: `at most ${RECORDS_COMPUTED_MAX} computed fields per kind` })
  const schema = isObject(kind.schema) ? kind.schema : {}
  const props = isObject(schema.properties) ? schema.properties : {}
  const vars = computedVars(kind)
  for (const name of names) {
    const fp = `${at}/computed/${escape(name)}`
    const source = computed[name]
    if (own(props, name)) issues.push({ path: fp, message: `${name} shadows a field of the schema` })
    else if (RECORDS_ROW_KEYS.includes(name)) issues.push({ path: fp, message: `${name} is a reserved record key` })
    else if (typeof source !== `string`) issues.push({ path: fp, message: `a computed field is an expression (a string)` })
    else {
      const parsed = parseExpression(source)
      const reads = `ast` in parsed ? expressionNames(parsed.ast).filter((n) => own(computed, n) && !own(props, n)) : []
      if (reads.length) issues.push({ path: fp, message: `a computed field reads no computed field: ${reads.join(`, `)}` })
      else {
        const checked = checkExpression(source, vars)
        if (`error` in checked) issues.push({ path: fp, message: checked.error })
      }
    }
  }
  return issues
}

/** eApps U3: a calendar day cell and a segment (calendar.ts). */
const DAY_CELL_TYPE: ExprType = { date: `date`, day: `number`, weekday: `string`, initial: `string`, inMonth: `bool`, weekend: `bool`, today: `bool`, selected: `bool`, total: `number`, dots: [`string`] }
const segmentType = (row: ExprType): ExprType => ({ id: `string`, col: `number`, span: `number`, lane: `number`, contL: `bool`, contR: `bool`, tone: optionalType(`string`), more: `number`, row: optionalType(row) })

/** A month view's value (`RecordsMonth`; eApps U3: + weeks, day, agenda). */
const monthType = (row: ExprType): ExprType => ({
  key: `string`,
  label: `string`,
  prev: `string`,
  next: `string`,
  today: `date`,
  days: [{ date: `date`, day: `number`, weekday: `string`, count: `number`, sum: `number`, running: optionalType(`map`) }],
  labels: [`string`],
  counts: [`number`],
  sums: [`number`],
  weeks: [{ key: `date`, days: [DAY_CELL_TYPE], segments: [segmentType(row)] }],
  day: `date`,
  agenda: { date: `date`, label: `string`, count: `number`, items: [{ id: `string`, tone: `string`, start: `date`, end: `date`, days: `number`, row }] },
})

/** eApps U3: the week view's value. */
const weekType = (row: ExprType): ExprType => ({ key: `date`, label: `string`, prev: `date`, next: `date`, today: `date`, days: [DAY_CELL_TYPE], segments: [segmentType(row)] })

/** eApps U3: the timeline's value. */
const timelineType = (row: ExprType): ExprType => ({
  scale: `string`,
  key: `date`,
  label: `string`,
  prev: `date`,
  next: `date`,
  today: `date`,
  columns: [{ key: `date`, label: `string`, end: `date`, today: `bool` }],
  bands: [{ label: `string`, col: `number`, span: `number` }],
  rows: [{ key: `string`, index: `number`, lanes: `number`, count: `number`, items: [{ id: `string`, rowIndex: `number`, col: `number`, span: `number`, lane: `number`, contL: `bool`, contR: `bool`, tone: `string`, row }] }],
  now: optionalType({ col: `number`, percent: `number` }),
})

/** The value a records binding lands (docs/package-format.md "Records").
 *  VAPP-204: what the binding's query leaves out may be missing (`T?`):
 *  `sum` without a sum, `month` without a month view, `groups` without a
 *  group, `totals` without an aggregate, `before` without a before (eApps
 *  U3: `week`, `timeline` without theirs); with no query known, all of them. */
export function recordsValueType(kind: unknown, query?: RecordsQuery | null): ExprType {
  const has = (present: boolean | undefined, t: ExprType) => (present ? t : optionalType(t))
  const row = recordRowType(kind) as Record<string, ExprType>
  return {
    rows: [query?.running.length && !query.group.length ? { ...row, running: `map` } : row],
    count: `number`,
    sum: has(query ? query.sum !== null : undefined, `number`),
    groups: has(query ? query.group.length > 0 : undefined, [`map`]),
    totals: has(query ? query.aggregate.length > 0 : undefined, `map`),
    before: has(query ? query.before !== null : undefined, `map`),
    month: has(query ? query.month !== null : undefined, monthType(row)),
    week: has(query ? query.week !== null : undefined, weekType(row)),
    timeline: has(query ? query.timeline !== null : undefined, timelineType(row)),
  }
}

/** A literal's type: scalars, lists (one item type, else dyn), rows. */
export function literalType(v: unknown): ExprType {
  if (Array.isArray(v)) {
    const items = v.map(literalType)
    if (items.length === 0) return [`dyn`]
    return [items.every((t) => JSON.stringify(t) === JSON.stringify(items[0])) ? items[0]! : `dyn`]
  }
  if (isObject(v)) {
    const out: Record<string, ExprType> = {}
    for (const [k, x] of Object.entries(v)) out[k] = literalType(x)
    return out
  }
  return typeName(v)
}

/** A walk's answer: the type, or the first field a closed row has not. */
type Walked = { type: ExprType } | { unknown: string }

/** The type at the rest of a pointer below a type. VAPP-204: an index may
 *  be past the end and a map's field missing (`T?`); `strict` = a row's
 *  unknown field is an error (a records row), else open (`dyn`). */
function walk(type: ExprType, segments: string[], strict: boolean): Walked {
  let t = type
  let opt = false
  for (const seg of segments) {
    if (isOptional(t)) {
      opt = true
      t = t[`?`]
    }
    if (Array.isArray(t)) {
      if (!/^\d+$/.test(seg)) return { type: `dyn` }
      t = t[0]!
      opt = true
    } else if (t === `map`) {
      t = `dyn`
      opt = true
    } else if (typeof t === `object`) {
      if (!own(t, seg)) return strict ? { unknown: seg } : { type: `dyn` }
      t = t[seg]!
    } else return { type: `dyn` }
  }
  return { type: opt ? optionalType(t) : t }
}

const unescape = (s: string) => s.replace(/~1/g, `/`).replace(/~0/g, `~`)

/** A type and whether its rows are closed (a records binding's, unless
 *  the kind allows additional properties). */
interface Scope {
  type: ExprType
  strict: boolean
}

/** The type of an absolute data path in a template: below a records
 *  binding its value, else the template data's literal, else dyn. */
function pathType(path: string, template: Record<string, unknown>, records: unknown, queries: unknown): Walked & { strict?: boolean } {
  if (!path.startsWith(`/`)) return { type: `dyn` }
  const bindings = Array.isArray(template.bindings) ? template.bindings : []
  for (const b of bindings) {
    if (!isObject(b) || typeof b.path !== `string` || typeof b.source !== `string`) continue
    if (path !== b.path && !path.startsWith(`${b.path}/`)) continue
    const parsed = parseSource(b.source)
    const kind = parsed?.scheme === RECORDS_SCHEME ? parsed.name : undefined
    if (!kind || !isObject(records) || !own(records, kind)) return { type: `dyn` }
    const rest = path.slice(b.path.length).split(`/`).filter((s, i) => i > 0 || s !== ``)
    const strict = closedKind(records[kind])
    const expanded = parseSource(expandQuerySource(b.source, isObject(queries) ? queries : undefined))
    const query = expanded ? parseRecordsSource(expanded) : null
    return { ...walk(recordsValueType(records[kind], query), rest.map(unescape), strict), strict }
  }
  let cur: unknown = template.data
  for (const seg of path === `/` ? [] : path.slice(1).split(`/`).map(unescape)) {
    if (Array.isArray(cur) && /^\d+$/.test(seg)) cur = cur[Number(seg)]
    else if (isObject(cur) && own(cur, seg)) cur = cur[seg]
    else return { type: `dyn` }
  }
  return { type: cur === undefined ? `dyn` : literalType(cur) }
}

/** A kind whose rows have only their schema's fields and the row keys. */
function closedKind(kind: unknown): boolean {
  const schema = isObject(kind) && isObject(kind.schema) ? kind.schema : {}
  return schema.additionalProperties !== true
}

/** A data path's type where it is read: absolute against the template,
 *  relative against the row scope it sits in (VAPP-204), else open. */
function bindingType(path: string, scope: Scope | null, template: Record<string, unknown>, records: unknown, queries: unknown): Walked & { strict?: boolean } {
  if (path.startsWith(`/`)) return pathType(path, template, records, queries)
  if (!scope) return { type: `dyn` }
  return { ...walk(scope.type, path.split(`/`).filter((s) => s !== ``).map(unescape), scope.strict), strict: scope.strict }
}

/** VAPP-204: the row each component of a template renders in: a
 *  `children: {componentId, path}` template gives its row component (and
 *  everything under it) the list's item type; first place wins, as in the
 *  reducer. Components outside every row are absent. */
function rowScopes(template: Record<string, unknown>, records: unknown, queries: unknown): Map<string, Scope | null> {
  const byId = new Map<string, Record<string, unknown>>()
  for (const c of Array.isArray(template.components) ? template.components : []) if (isObject(c) && typeof c.id === `string` && !byId.has(c.id)) byId.set(c.id, c)
  const scopes = new Map<string, Scope | null>()
  const place = (id: string, scope: Scope | null) => {
    const c = byId.get(id)
    if (!c || scopes.has(id)) return
    scopes.set(id, scope)
    if (Array.isArray(c.children)) for (const child of c.children) if (typeof child === `string`) place(child, scope)
    if (isObject(c.children) && typeof c.children.componentId === `string` && typeof c.children.path === `string`) {
      const list = bindingType(c.children.path, scope, template, records, queries)
      const t = `type` in list ? requiredType(list.type) : `dyn`
      place(c.children.componentId, { type: Array.isArray(t) ? t[0]! : `dyn`, strict: Array.isArray(t) && list.strict === true })
    }
    if (isObject(c.slots)) for (const child of Object.values(c.slots)) if (typeof child === `string`) place(child, scope)
  }
  place(`root`, null)
  return scopes
}

/** A var's type; one the checker cannot type may be missing (`dyn?`). A
 *  binding to a field a closed row has not is an issue at its path. */
function varType(v: unknown, at: string, scope: Scope | null, template: Record<string, unknown>, records: unknown, queries: unknown, issues: ExpressionIssue[]): ExprType {
  if (isBinding(v)) {
    const t = bindingType(v.path, scope, template, records, queries)
    if (`unknown` in t) {
      issues.push({ path: `${at}/path`, message: `unknown field ${t.unknown} in ${v.path}` })
      return optionalType(`dyn`)
    }
    return t.type === `dyn` ? optionalType(t.type) : t.type
  }
  if (isCall(v)) return optionalType(`dyn`)
  if (Array.isArray(v)) {
    const items = v.map((x, i) => varType(x, `${at}/${i}`, scope, template, records, queries, issues))
    if (items.length === 0) return [`dyn`]
    return [items.every((t) => JSON.stringify(t) === JSON.stringify(items[0])) ? items[0]! : `dyn`]
  }
  if (isObject(v)) {
    const out: Record<string, ExprType> = {}
    for (const [k, x] of Object.entries(v)) out[k] = varType(x, `${at}/${escape(k)}`, scope, template, records, queries, issues)
    return out
  }
  return typeName(v)
}

/** Every `expr` call in a package's templates, checked. */
export function validatePackageExpressions(pkg: Record<string, unknown>): ExpressionIssue[] {
  const issues: ExpressionIssue[] = []
  if (!isObject(pkg.templates)) return issues
  for (const [id, template] of Object.entries(pkg.templates)) {
    if (!isObject(template) || !Array.isArray(template.components)) continue
    let scope: Scope | null = null
    const visit = (value: unknown, at: string) => {
      if (Array.isArray(value)) return value.forEach((v, i) => visit(v, `${at}/${i}`))
      if (!isObject(value)) return
      if (value.call === `expr` && isObject(value.args)) {
        const args = value.args
        const here = `${at}/args`
        // VAPP-195: every expression binding shows a default when it fails.
        if (!Object.prototype.hasOwnProperty.call(args, `default`)) issues.push({ path: `${here}/default`, message: `an expression binding needs a default, shown when it fails` })
        if (typeof args.expr !== `string`) issues.push({ path: `${here}/expr`, message: `expr is a string` })
        else if (args.vars !== undefined && !isObject(args.vars)) issues.push({ path: `${here}/vars`, message: `vars is an object` })
        else {
          const types: Record<string, ExprType> = {}
          const unknown: ExpressionIssue[] = []
          for (const [name, v] of Object.entries((args.vars as Record<string, unknown> | undefined) ?? {})) types[name] = varType(v, `${here}/vars/${escape(name)}`, scope, template, pkg.records, pkg.queries, unknown)
          issues.push(...unknown)
          const checked = unknown.length ? null : checkExpression(args.expr, types)
          if (checked && `error` in checked) issues.push({ path: `${here}/expr`, message: checked.error })
        }
      }
      for (const [k, v] of Object.entries(value)) visit(v, `${at}/${escape(k)}`)
    }
    const scopes = rowScopes(template, pkg.records, pkg.queries)
    template.components.forEach((c, i) => {
      scope = isObject(c) && typeof c.id === `string` ? (scopes.get(c.id) ?? null) : null
      visit(c, `/templates/${escape(id)}/components/${i}`)
    })
  }
  return issues
}
