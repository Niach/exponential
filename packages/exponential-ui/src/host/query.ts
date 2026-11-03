// VAPP-189 (S1, docs/package-format.md "Records"): THE records query, one
// evaluator for every host. A binding's URL sugar (`records:<kind>?q`)
// parses INTO the structured `RecordsQuery`; `recordsValue` evaluates it
// over the rows of any `RecordStore` (record-store.ts). Pure, so the Rust
// core mirrors it line by line (`eui-core` `host/query.rs`, reached by the
// Swift and Compose SDKs through the FFI's `parse_records_query_json` /
// `parse_records_json` / `records_value_json`); fixtures/records-query.json
// is the spec.
//
// VAPP-190 (Q1): the JSON query form (`parseRecordsJson`, a package's
// `queries`, the `q=<json>` param): `group` (any field, `author`, date
// buckets), `aggregate`, `running`, `before`, the `in` / `between`
// operators and `or`. The date maths lives in `../dates.ts` (shared with
// the bind-time expressions).

import { evaluateExpression, expressionNames, parseExpression, parseTimestamp, type ExprNode } from "../cel"
import { DATE_BUCKETS, dateBucket, daysInMonth, weekdayOf, type DateBucket } from "../dates"
import { RECORD_TRANSITIONS_ROW_KEY, transitionsFrom, type RecordTransitions } from "./record-transitions"
import { canonicalJson } from "./signature"
import { parseSource, sourceWithParams, type ParsedSource } from "./sources"

/** The binding scheme: `records:<kind>[?query]`. */
export const RECORDS_SCHEME = `records`
/** The URL sugar's defaults and caps (catalog/host.json `package.records.query`). */
export const RECORDS_QUERY = { sortDefault: `createdAt`, orderDefault: `desc`, limitDefault: 100, limitMax: 500 } as const
/** Query keys with a meaning of their own; never a field filter. */
export const RECORDS_RESERVED_PARAMS: readonly string[] = [`app`, `before`, `by`, `expand`, `limit`, `mine`, `month`, `order`, `q`, `query`, `sort`, `sum`, `user`]
/** The keys every value row carries next to its data (a data key never shadows one). */
export const RECORDS_ROW_KEYS: readonly string[] = [`id`, `createdAt`, `updatedAt`, `createdBy`, `authorId`, `author`]
/** The filter operators: `eq` is the bare key, the others a `.<op>` suffix
 *  (`in` and `between` take comma-separated values). */
export const RECORDS_FILTER_OPS: readonly RecordsFilterOp[] = [`eq`, `gt`, `gte`, `lt`, `lte`, `in`, `between`]
/** The JSON form's spellings of the operators. */
export const RECORDS_OP_ALIASES: Readonly<Record<string, RecordsFilterOp>> = { "=": `eq`, "==": `eq`, ">": `gt`, ">=": `gte`, "<": `lt`, "<=": `lte` }
export const RECORDS_AGGREGATE_OPS: readonly RecordsAggregateOp[] = [`sum`, `avg`, `min`, `max`, `count`, `countDistinct`, `first`, `last`]
export const RECORDS_RUNNING_OPS: readonly RecordsRunningOp[] = [`sum`, `count`, `avg`, `min`, `max`]
/** Names an aggregate may not take (a group's and a total's own keys). */
export const RECORDS_AGGREGATE_RESERVED: readonly string[] = [`count`, `sum`, `running`, `rows`]
/** The month view's day and month names (en-US, the same ×4; query.test.ts pins them to the fixture, which stays out of the bundle). */
export const RECORDS_WEEKDAYS: readonly string[] = [`Sun`, `Mon`, `Tue`, `Wed`, `Thu`, `Fri`, `Sat`]
export const RECORDS_MONTHS: readonly string[] = [`January`, `February`, `March`, `April`, `May`, `June`, `July`, `August`, `September`, `October`, `November`, `December`]

export type RecordsFilterOp = `eq` | `gt` | `gte` | `lt` | `lte` | `in` | `between`
export type RecordsOrder = `asc` | `desc`
export type RecordsAggregateOp = `sum` | `avg` | `min` | `max` | `count` | `countDistinct` | `first` | `last`
export type RecordsRunningOp = `sum` | `count` | `avg` | `min` | `max`

/** One condition: `eq` / `in` compare as text, the ranges (`between`
 *  inclusive) as numbers when both sides parse, else as text. `in` and
 *  `between` carry a list (`between`: [low, high]). */
export interface RecordsFilter {
  field: string
  op: RecordsFilterOp
  value: string | string[]
}

export interface RecordsSortKey {
  field: string
  order: RecordsOrder
}

/** `by=<dateField>&month=YYYY-MM`: the rows of one month, every day listed. */
export interface RecordsMonthView {
  /** The date field the rows are bucketed by. */
  field: string
  /** `YYYY-MM`; null = the month of `now`. */
  key: string | null
}

/** One group-by key: a field (`author` = the author, by name), or a date
 *  field cut into `bucket`s. The group's column is named `field`. */
export interface RecordsGroupKey {
  field: string
  bucket: DateBucket | null
}

/** `name: [op, field?]`: a column of every group (and of `totals`). */
export interface RecordsAggregate {
  name: string
  op: RecordsAggregateOp
  /** null = `count` of the rows. */
  field: string | null
}

/** A cumulative value of `field` over the groups (or rows, or month days) in order. */
export interface RecordsRunning {
  field: string
  op: RecordsRunningOp
}

/** The rows whose `field` is before `date` are left out of the rows and
 *  groups, totalled into `before` and seed every running value. */
export interface RecordsBefore {
  field: string
  date: string
}

export interface RecordsQuery {
  /** The kind; null = no kind (the value is null). */
  from: string | null
  /** Another app's slug (the HOST resolves whose rows); null = the hosting app. */
  app: string | null
  /** Only the viewer's rows (their author: `authorId ?? createdBy`). */
  mine: boolean
  /** Only this user's rows (by author). */
  user: string | null
  /** Every condition holds; by field, then operator (the URL sugar). */
  where: RecordsFilter[]
  /** VAPP-190: one of these alternatives holds too (each: every condition); [] = no alternatives. */
  or: RecordsFilter[][]
  /** Sort keys in order; ties always fall back to the id. */
  sort: RecordsSortKey[]
  limit: number
  /** Σ of this field over the matches (before the limit). */
  sum: string | null
  month: RecordsMonthView | null
  /** VAPP-190: the group-by keys ([] = no groups). */
  group: RecordsGroupKey[]
  /** VAPP-190: the aggregates, by name. */
  aggregate: RecordsAggregate[]
  /** VAPP-190: the running values, by field. */
  running: RecordsRunning[]
  before: RecordsBefore | null
  /** eApps R5 (VAPP-219): ref fields whose target is inlined into every
   *  row (`records.<kind>.refs`, one level); [] = none. */
  expand: string[]
}

/** eApps R5 (VAPP-219): what a ref field points to: a record of a kind of
 *  the same app, an issue of the team (by identifier, `EXP-12`) or a member
 *  of the team (by user id). */
export type RecordRefTarget = { kind: string } | `issue` | `member`

/** One stored record as a host hands it to the evaluator. */
export interface RecordRow {
  id: string
  kind: string
  data: Record<string, unknown>
  createdBy: string | null
  /** Whose record it is (#VAPP-187); unset = `createdBy`. */
  authorId?: string | null
  createdAt: string
  updatedAt: string
}

export interface RecordsDay {
  date: string
  day: number
  weekday: string
  count: number
  sum: number
  running?: Record<string, number | null>
}

export interface RecordsMonth {
  key: string
  label: string
  prev: string
  next: string
  /** `now`'s date (YYYY-MM-DD): the default for a new entry. */
  today: string
  days: RecordsDay[]
  /** The days as chart-ready arrays (Chart categories + series values). */
  labels: string[]
  counts: number[]
  sums: number[]
}

export interface RecordsValue {
  rows: Record<string, unknown>[]
  count: number
  sum?: number
  month?: RecordsMonth
  /** VAPP-190: one row per group: its columns, count, sum?, the aggregates, running?. */
  groups?: Record<string, unknown>[]
  /** VAPP-190: the aggregates over every match: count, sum?, the aggregates. */
  totals?: Record<string, unknown>
  /** VAPP-190: the same over the rows before `before.date`. */
  before?: Record<string, unknown>
}

export interface RecordsContext {
  /** The viewer's user id (null = signed out: `mine` matches nothing). */
  me: string | null
  /** ISO 8601 (UTC): the clock (computed fields); default = the wall clock. */
  now?: string
  /** `YYYY-MM-DD`: the host's calendar day in the team zone (team-zone.json):
   *  the month view's `today` and default month; default = `now`'s UTC day. */
  today?: string
  /** user id → display name (`author`; "" when unknown). */
  names?: Record<string, string>
  /** VAPP-214 (Q3): the queried kind's computed fields (`records.<kind>.computed`:
   *  name → CEL), evaluated on every row of the kind before the query. */
  computed?: Record<string, string>
  /** eApps R5 (VAPP-219): the queried kind's refs (`records.<kind>.refs`),
   *  what `expand` follows; a field without one is left as stored. */
  refs?: Record<string, RecordRefTarget>
  /** eApps R5: identifier → the issue summary an `issue` ref expands to
   *  (`{id, identifier, title, status}`); an identifier missing here reads null. */
  issues?: Record<string, Record<string, unknown>>
  /** eApps R6 (VAPP-222): the queried kind's transitions (`kindTransitions`):
   *  every row carries `transitions`, the next states the viewer may take. */
  transitions?: RecordTransitions | null
  /** eApps R6: the viewer's team role (`owner` takes every edge); unset = a member. */
  role?: string | null
}

/** A problem with a JSON query, a JSON pointer into it. */
export interface RecordsQueryIssue {
  path: string
  message: string
}

const MONTH_KEY = /^\d{4}-(0[1-9]|1[0-2])$/
const SUFFIX_OPS: readonly string[] = [`gt`, `gte`, `lt`, `lte`, `in`, `between`]
const LIST_OPS: readonly string[] = [`in`, `between`]

const trimmed = (v: string | null | undefined): string | null => v?.trim() || null
const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const byCodeUnits = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)

/** Query params (as `parseSource` gives them) → the query. `kind` = the
 *  source's own name (`records:<kind>`). Unknown keys are field filters. VAPP-190: `q=<json>` = the
 *  JSON form (`parseRecordsJson`; the other params are ignored). */
export function parseRecordsQuery(params: Record<string, string>, kind?: string | null): RecordsQuery {
  const from = trimmed(kind)
  if (params.q !== undefined && params.q.trim()) {
    let json: unknown = null
    try {
      json = JSON.parse(params.q)
    } catch {
      json = null
    }
    return parseRecordsJson(isObject(json) ? json : {}, from).query
  }
  const limitRaw = Number(params.limit)
  const where: RecordsFilter[] = []
  for (const key of Object.keys(params).sort()) {
    if (RECORDS_RESERVED_PARAMS.includes(key) || !key) continue
    const dot = key.lastIndexOf(`.`)
    const suffix = dot >= 0 ? key.slice(dot + 1) : ``
    const value = params[key]!
    if (dot > 0 && SUFFIX_OPS.includes(suffix)) where.push({ field: key.slice(0, dot), op: suffix as RecordsFilterOp, value: LIST_OPS.includes(suffix) ? value.split(`,`) : value })
    else where.push({ field: key, op: `eq`, value })
  }
  where.sort((a, b) => byCodeUnits(a.field, b.field) || byCodeUnits(a.op, b.op))
  const by = trimmed(params.by)
  const before = trimmed(params.before)
  return {
    from,
    app: trimmed(params.app),
    mine: params.mine === `1` || params.mine === `true`,
    user: trimmed(params.user),
    where,
    or: [],
    sort: [{ field: trimmed(params.sort) ?? RECORDS_QUERY.sortDefault, order: params.order === `asc` ? `asc` : params.order === `desc` ? `desc` : RECORDS_QUERY.orderDefault }],
    limit: Number.isInteger(limitRaw) && limitRaw > 0 ? Math.min(limitRaw, RECORDS_QUERY.limitMax) : RECORDS_QUERY.limitDefault,
    sum: trimmed(params.sum),
    month: by ? { field: by, key: params.month && MONTH_KEY.test(params.month) ? params.month : null } : null,
    group: [],
    aggregate: [],
    running: [],
    before: before ? { field: by ?? RECORDS_QUERY.sortDefault, date: before } : null,
    expand: [...new Set((params.expand ?? ``).split(`,`).map((f) => f.trim()).filter(Boolean))],
  }
}

/** `records:<kind>?…` → the query; null = not a records source. */
export function parseRecordsSource(source: Pick<ParsedSource, `scheme` | `name` | `params`>): RecordsQuery | null {
  return source.scheme === RECORDS_SCHEME ? parseRecordsQuery(source.params, source.name) : null
}

/** A JSON scalar as the query's text (a number as JS prints it); null = not a scalar. */
function scalarText(v: unknown): string | null {
  if (v === null) return ``
  if (typeof v === `string`) return v
  if (typeof v === `boolean`) return v ? `true` : `false`
  if (typeof v === `number`) return Number.isFinite(v) ? String(v) : null
  return null
}

/** `[field, op, value…]` → a condition (`in`: [field, "in", [values]];
 *  `between`: [field, "between", low, high]). */
function parseCondition(entry: unknown, at: string, issues: RecordsQueryIssue[]): RecordsFilter | null {
  if (!Array.isArray(entry) || entry.length < 3) {
    issues.push({ path: at, message: `a condition is [field, op, value]` })
    return null
  }
  const [fieldName, opName, ...rest] = entry as unknown[]
  if (typeof fieldName !== `string` || !fieldName.trim()) {
    issues.push({ path: `${at}/0`, message: `a condition's field is a non-empty string` })
    return null
  }
  const op = typeof opName === `string` ? (RECORDS_OP_ALIASES[opName] ?? (RECORDS_FILTER_OPS.includes(opName as RecordsFilterOp) ? (opName as RecordsFilterOp) : null)) : null
  if (!op) {
    issues.push({ path: `${at}/1`, message: `unknown operator ${String(opName)}` })
    return null
  }
  const field = fieldName.trim()
  if (op === `in`) {
    const list = rest.length === 1 && Array.isArray(rest[0]) ? (rest[0] as unknown[]).map(scalarText) : null
    if (!list || list.some((v) => v === null)) {
      issues.push({ path: `${at}/2`, message: `in takes one list of values` })
      return null
    }
    return { field, op, value: list as string[] }
  }
  const values = rest.map(scalarText)
  if (values.some((v) => v === null) || values.length !== (op === `between` ? 2 : 1)) {
    issues.push({ path: `${at}/2`, message: op === `between` ? `between takes a low and a high value` : `${op} takes one value` })
    return null
  }
  return { field, op, value: op === `between` ? (values as string[]) : values[0]! }
}

const QUERY_KEYS: readonly string[] = [`from`, `app`, `mine`, `user`, `where`, `sort`, `limit`, `sum`, `month`, `group`, `aggregate`, `running`, `before`, `expand`]

/** VAPP-190: the JSON form (a package's `queries`, the `q` param) → the
 *  query and every problem with it (a problem drops the part, the rest
 *  still runs). `kind` (a source's own) wins over `from`.
 *
 *  `{from, app?, mine?, user?, where?: [[field, op, value…] | {or: [[cond…]…]}],
 *  sort?: field | [field | [field, asc|desc]], limit?, sum?, month?: {field, key?},
 *  group?: [field | {bucket: field, by: day|week|month|quarter|year}],
 *  aggregate?: {name: [op, field?]}, running?: {field: op}, before?: date | {field, date}}` */
export function parseRecordsJson(json: unknown, kind?: string | null): { query: RecordsQuery; issues: RecordsQueryIssue[] } {
  const issues: RecordsQueryIssue[] = []
  const query = parseRecordsQuery({}, null)
  query.sort = [{ field: RECORDS_QUERY.sortDefault, order: RECORDS_QUERY.orderDefault }]
  if (!isObject(json)) {
    issues.push({ path: ``, message: `a query is an object` })
    query.from = trimmed(kind)
    return { query, issues }
  }
  for (const key of Object.keys(json).sort()) if (!QUERY_KEYS.includes(key) && !key.startsWith(`$`)) issues.push({ path: `/${key}`, message: `unknown key ${key}` })
  const str = (key: string): string | null => {
    const v = json[key]
    if (v === undefined || v === null) return null
    if (typeof v !== `string`) {
      issues.push({ path: `/${key}`, message: `${key} is a string` })
      return null
    }
    return trimmed(v)
  }
  query.from = trimmed(kind) ?? str(`from`)
  query.app = str(`app`)
  query.user = str(`user`)
  query.sum = str(`sum`)
  if (json.mine !== undefined) {
    if (typeof json.mine !== `boolean`) issues.push({ path: `/mine`, message: `mine is a boolean` })
    else query.mine = json.mine
  }
  if (json.where !== undefined) {
    if (!Array.isArray(json.where)) issues.push({ path: `/where`, message: `where is a list of conditions` })
    else
      json.where.forEach((entry, i) => {
        const at = `/where/${i}`
        if (isObject(entry)) {
          if (Object.keys(entry).length !== 1 || !Array.isArray(entry.or)) return void issues.push({ path: at, message: `an alternative is {or: [[condition…]…]}` })
          if (query.or.length) return void issues.push({ path: at, message: `one or per query` })
          ;(entry.or as unknown[]).forEach((alt, j) => {
            if (!Array.isArray(alt)) return void issues.push({ path: `${at}/or/${j}`, message: `an alternative is a list of conditions` })
            const conds: RecordsFilter[] = []
            alt.forEach((c, k) => {
              const parsed = parseCondition(c, `${at}/or/${j}/${k}`, issues)
              if (parsed) conds.push(parsed)
            })
            query.or.push(conds)
          })
          return
        }
        const parsed = parseCondition(entry, at, issues)
        if (parsed) query.where.push(parsed)
      })
  }
  if (json.sort !== undefined) {
    const list = typeof json.sort === `string` ? [json.sort] : Array.isArray(json.sort) ? json.sort : null
    if (!list) issues.push({ path: `/sort`, message: `sort is a field or a list of [field, order]` })
    else {
      const keys: RecordsSortKey[] = []
      list.forEach((entry, i) => {
        const [name, order] = typeof entry === `string` ? [entry, `asc`] : Array.isArray(entry) ? [entry[0], entry.length > 1 ? entry[1] : `asc`] : [null, null]
        if (typeof name !== `string` || !name.trim() || (order !== `asc` && order !== `desc`)) return void issues.push({ path: typeof json.sort === `string` ? `/sort` : `/sort/${i}`, message: `a sort key is field or [field, asc|desc]` })
        keys.push({ field: name.trim(), order })
      })
      if (keys.length) query.sort = keys
    }
  }
  if (json.limit !== undefined) {
    const n = typeof json.limit === `number` ? json.limit : typeof json.limit === `string` && json.limit.trim() ? Number(json.limit) : NaN
    if (!Number.isInteger(n) || n < 1) issues.push({ path: `/limit`, message: `limit is a positive integer` })
    else query.limit = Math.min(n, RECORDS_QUERY.limitMax)
  }
  if (json.month !== undefined && json.month !== null) {
    const m = json.month
    if (!isObject(m) || typeof m.field !== `string` || !m.field.trim() || (m.key !== undefined && m.key !== null && typeof m.key !== `string`)) issues.push({ path: `/month`, message: `month is {field, key?}` })
    else query.month = { field: m.field.trim(), key: typeof m.key === `string` && MONTH_KEY.test(m.key) ? m.key : null }
  }
  if (json.group !== undefined) {
    if (!Array.isArray(json.group)) issues.push({ path: `/group`, message: `group is a list of keys` })
    else
      json.group.forEach((entry, i) => {
        const at = `/group/${i}`
        let key: RecordsGroupKey | null = null
        if (typeof entry === `string` && entry.trim()) key = { field: entry.trim(), bucket: null }
        else if (isObject(entry) && typeof entry.bucket === `string` && entry.bucket.trim() && DATE_BUCKETS.includes(entry.by as DateBucket) && Object.keys(entry).length === 2) key = { field: entry.bucket.trim(), bucket: entry.by as DateBucket }
        if (!key) return void issues.push({ path: at, message: `a group key is a field or {bucket: field, by: ${DATE_BUCKETS.join(`|`)}}` })
        if (query.group.some((g) => g.field === key.field || (key.field === `author` && g.field === `authorId`) || (key.field === `authorId` && g.field === `author`)))
          return void issues.push({ path: at, message: `duplicate group ${key.field}` })
        query.group.push(key)
      })
  }
  if (json.aggregate !== undefined) {
    if (!isObject(json.aggregate)) issues.push({ path: `/aggregate`, message: `aggregate is {name: [op, field?]}` })
    else
      for (const name of Object.keys(json.aggregate).sort()) {
        const def = json.aggregate[name]
        const at = `/aggregate/${name}`
        if (!name.trim() || RECORDS_AGGREGATE_RESERVED.includes(name) || query.group.some((g) => g.field === name || (g.field === `author` && name === `authorId`))) {
          issues.push({ path: at, message: `the name ${name} is taken` })
          continue
        }
        const op = Array.isArray(def) && RECORDS_AGGREGATE_OPS.includes(def[0] as RecordsAggregateOp) ? (def[0] as RecordsAggregateOp) : null
        const f = Array.isArray(def) && def.length > 1 && typeof def[1] === `string` && def[1].trim() ? def[1].trim() : null
        if (!op || !Array.isArray(def) || def.length > 2 || (def.length > 1 && !f) || (op !== `count` && !f)) {
          issues.push({ path: at, message: `an aggregate is [${RECORDS_AGGREGATE_OPS.join(`|`)}, field] (count: the field is optional)` })
          continue
        }
        query.aggregate.push({ name, op, field: f })
      }
  }
  if (json.running !== undefined) {
    if (!isObject(json.running)) issues.push({ path: `/running`, message: `running is {field: op}` })
    else
      for (const name of Object.keys(json.running).sort()) {
        const op = json.running[name]
        if (!name.trim() || !RECORDS_RUNNING_OPS.includes(op as RecordsRunningOp)) {
          issues.push({ path: `/running/${name}`, message: `a running value is {field: ${RECORDS_RUNNING_OPS.join(`|`)}}` })
          continue
        }
        query.running.push({ field: name, op: op as RecordsRunningOp })
      }
  }
  if (json.before !== undefined && json.before !== null) {
    const b = json.before
    const implied = query.group.find((g) => g.bucket !== null)?.field ?? query.month?.field ?? RECORDS_QUERY.sortDefault
    if (typeof b === `string`) query.before = trimmed(b) ? { field: implied, date: b.trim() } : null
    else if (isObject(b) && typeof b.field === `string` && b.field.trim() && typeof b.date === `string`) query.before = b.date.trim() ? { field: b.field.trim(), date: b.date.trim() } : null
    else issues.push({ path: `/before`, message: `before is a date or {field, date}` })
  }
  // eApps R5 (VAPP-219): the ref fields to inline.
  if (json.expand !== undefined) {
    if (!Array.isArray(json.expand)) issues.push({ path: `/expand`, message: `expand is a list of ref fields` })
    else
      json.expand.forEach((entry, i) => {
        if (typeof entry !== `string` || !entry.trim()) return void issues.push({ path: `/expand/${i}`, message: `an expanded field is a non-empty string` })
        if (query.expand.includes(entry.trim())) return void issues.push({ path: `/expand/${i}`, message: `duplicate expand ${entry.trim()}` })
        query.expand.push(entry.trim())
      })
  }
  return { query, issues }
}

/** `${name}` in every string of a JSON query → `params[name]` ("" when unset). */
export function fillQueryTemplates(value: unknown, params: Record<string, string>): unknown {
  if (typeof value === `string`) return value.replace(/\$\{([^}]*)\}/g, (_, name: string) => params[name.trim()] ?? ``)
  if (Array.isArray(value)) return value.map((v) => fillQueryTemplates(v, params))
  if (isObject(value)) return Object.fromEntries(Object.keys(value).map((k) => [k, fillQueryTemplates(value[k], params)]))
  return value
}

/** VAPP-190: a package binding `<scheme>:<name>?query=<id>&<params>` naming
 *  one of the package's `queries` → the same source with that query, its
 *  `${param}` templates filled from the other params, as `q=<canonical JSON>`.
 *  Anything else is returned as written. */
export function expandQuerySource(source: string, queries: Record<string, unknown> | undefined): string {
  const parsed = parseSource(source)
  const id = parsed?.params.query
  if (!parsed || id === undefined || !isObject(queries) || !Object.prototype.hasOwnProperty.call(queries, id)) return source
  const { query: _, ...params } = parsed.params
  const q = queries[id]
  const filled = fillQueryTemplates(q, params)
  const base = source.trim()
  const head = base.includes(`?`) ? base.slice(0, base.indexOf(`?`)) : base
  return sourceWithParams(head, { q: canonicalJson(filled) })
}

const num = (v: unknown): number | null => {
  if (typeof v === `number`) return Number.isFinite(v) ? v : null
  if (typeof v === `string` && v.trim() !== ``) {
    const n = Number(v)
    return Number.isFinite(n) ? n : null
  }
  return null
}
const text = (v: unknown): string => (v === null || v === undefined ? `` : typeof v === `string` ? v : typeof v === `object` ? canonicalJson(v) : String(v))

/** Numbers when both parse, else text (code units, no locale: the same ×4). */
function compare(a: unknown, b: unknown): number {
  const na = num(a)
  const nb = num(b)
  if (na !== null && nb !== null) return na < nb ? -1 : na > nb ? 1 : 0
  return byCodeUnits(text(a), text(b))
}

/** VAPP-202: a value read once for comparing: its number when it parses, its text. */
interface Key {
  n: number | null
  t: string
}
const keyOf = (v: unknown): Key => {
  const n = num(v)
  return { n, t: n === null ? text(v) : `` }
}
/** `compare` over keys (a number's text only when the other side is text). */
function compareKeys(a: Key, b: Key, av: unknown, bv: unknown): number {
  if (a.n !== null && b.n !== null) return a.n < b.n ? -1 : a.n > b.n ? 1 : 0
  return byCodeUnits(a.n === null ? a.t : text(av), b.n === null ? b.t : text(bv))
}

/** VAPP-202: a condition with its bounds parsed once per query, not per row. */
interface Cond {
  filter: RecordsFilter
  list: readonly string[]
  eq: string
  bounds: { key: Key; v: string }[]
}
function cond(filter: RecordsFilter): Cond {
  const list = Array.isArray(filter.value) ? filter.value : [filter.value]
  const bounds = filter.op === `between` ? (list.length === 2 ? list : []) : list.length === 1 ? list : [text(filter.value)]
  return { filter, list, eq: text(filter.value), bounds: bounds.map((v) => ({ key: keyOf(v), v })) }
}

function holds(c: Cond, value: unknown): boolean {
  const op = c.filter.op
  if (op === `eq`) return text(value) === c.eq
  if (op === `in`) return c.list.includes(text(value))
  const k = keyOf(value)
  if (op === `between`) return c.bounds.length === 2 && compareKeys(k, c.bounds[0]!.key, value, c.bounds[0]!.v) >= 0 && compareKeys(k, c.bounds[1]!.key, value, c.bounds[1]!.v) <= 0
  const b = c.bounds[0]!
  const r = compareKeys(k, b.key, value, b.v)
  return op === `gt` ? r > 0 : op === `gte` ? r >= 0 : op === `lt` ? r < 0 : r <= 0
}

/** VAPP-202: `rows` in the query's order (sort keys, ties by id), each row's
 *  keys read ONCE into a column (all numbers: the numbers; all text: the
 *  distinct texts ranked once; both: the generic comparison) instead of
 *  parsing and stringifying both sides of every comparison. */
function sorted(rows: RecordRow[], sort: readonly RecordsSortKey[]): RecordRow[] {
  const cols = sort.map((s) => {
    const values = rows.map((row) => field(row, s.field))
    const keys = values.map(keyOf)
    const sign = s.order === `asc` ? 1 : -1
    if (keys.every((k) => k.n !== null)) {
      const v = Float64Array.from(keys, (k) => k.n!)
      return (a: number, b: number) => sign * (v[a]! < v[b]! ? -1 : v[a]! > v[b]! ? 1 : 0)
    }
    if (keys.every((k) => k.n === null)) {
      const distinct = [...new Set(keys.map((k) => k.t))].sort(byCodeUnits)
      const rank = new Map(distinct.map((t, i) => [t, i]))
      const v = Uint32Array.from(keys, (k) => rank.get(k.t)!)
      return (a: number, b: number) => sign * (v[a]! - v[b]!)
    }
    return (a: number, b: number) => sign * compareKeys(keys[a]!, keys[b]!, values[a], values[b])
  })
  const idx = rows.map((_, i) => i)
  idx.sort((a, b) => {
    for (const col of cols) {
      const c = col(a, b)
      if (c !== 0) return c
    }
    return byCodeUnits(rows[a]!.id, rows[b]!.id)
  })
  return idx.map((i) => rows[i]!)
}

/** A row's author: `authorId`, else `createdBy`. */
export function recordAuthor(row: Pick<RecordRow, `createdBy` | `authorId`>): string | null {
  return row.authorId ?? row.createdBy ?? null
}

/** A field as the query reads it: a row key (`authorId` = the author), else the data's. */
function field(row: RecordRow, name: string): unknown {
  return name === `createdAt` ? row.createdAt : name === `updatedAt` ? row.updatedAt : name === `id` ? row.id : name === `createdBy` ? row.createdBy : name === `authorId` ? recordAuthor(row) : row.data[name]
}

/** `YYYY-MM` ± `by` months. */
export function shiftMonth(key: string, by: number): string {
  const [y, m] = key.split(`-`).map(Number) as [number, number]
  const index = y * 12 + (m - 1) + by
  return `${Math.floor(index / 12)}-${String((((index % 12) + 12) % 12) + 1).padStart(2, `0`)}`
}

/** `October 2026`. */
export function monthLabel(key: string): string {
  const [y, m] = key.split(`-`).map(Number) as [number, number]
  return `${RECORDS_MONTHS[m - 1]} ${y}`
}

/** Every day of `key` (`YYYY-MM`), empty. */
export function monthDays(key: string): RecordsDay[] {
  const [y, m] = key.split(`-`).map(Number) as [number, number]
  const days: RecordsDay[] = []
  for (let day = 1; day <= daysInMonth(y, m); day++) {
    days.push({ date: `${key}-${String(day).padStart(2, `0`)}`, day, weekday: RECORDS_WEEKDAYS[weekdayOf(y, m, day)]!, count: 0, sum: 0 })
  }
  return days
}

/** Sums of hours like 0.1 + 0.2 stay readable (and identical ×4). */
function round(n: number): number {
  return Math.round(n * 1_000_000) / 1_000_000
}

const jsonOf = (v: unknown): unknown => (v === undefined ? null : v)

/** One aggregate over rows in the query's order. */
function aggregateOf(agg: RecordsAggregate, rows: readonly RecordRow[]): unknown {
  const values = agg.field === null ? [] : rows.map((r) => field(r, agg.field!))
  switch (agg.op) {
    case `count`:
      return agg.field === null ? rows.length : values.filter((v) => text(v) !== ``).length
    case `sum`:
      return round(values.reduce<number>((acc, v) => acc + (num(v) ?? 0), 0))
    case `avg`: {
      const nums = values.map(num).filter((n): n is number => n !== null)
      return nums.length ? round(nums.reduce((a, b) => a + b, 0) / nums.length) : null
    }
    case `min`:
    case `max`: {
      let best: unknown = undefined
      for (const v of values) {
        if (text(v) === ``) continue
        if (best === undefined || (agg.op === `min` ? compare(v, best) < 0 : compare(v, best) > 0)) best = v
      }
      return best === undefined ? null : (num(best) ?? text(best))
    }
    case `countDistinct`:
      return new Set(values.map(text).filter((t) => t !== ``)).size
    case `first`:
      return rows.length ? jsonOf(values[0]) : null
    case `last`:
      return rows.length ? jsonOf(values[values.length - 1]) : null
  }
}

/** count, sum?, then every aggregate: a group's, `totals`' and `before`'s columns. */
function summary(query: RecordsQuery, rows: readonly RecordRow[]): Record<string, unknown> {
  const out: Record<string, unknown> = { count: rows.length }
  if (query.sum !== null) out.sum = round(rows.reduce((acc, row) => acc + (num(row.data[query.sum!]) ?? 0), 0))
  for (const agg of query.aggregate) out[agg.name] = aggregateOf(agg, rows)
  return out
}

/** The running values: fed row by row, read after each step. */
class Running {
  private acc: { sum: number; count: number; nums: number; min: number | null; max: number | null }[]
  constructor(private readonly list: readonly RecordsRunning[]) {
    this.acc = list.map(() => ({ sum: 0, count: 0, nums: 0, min: null, max: null }))
  }
  add(row: RecordRow): void {
    this.list.forEach((r, i) => {
      const a = this.acc[i]!
      const n = num(field(row, r.field))
      a.count += 1
      if (n === null) return
      a.sum += n
      a.nums += 1
      a.min = a.min === null || n < a.min ? n : a.min
      a.max = a.max === null || n > a.max ? n : a.max
    })
  }
  read(): Record<string, number | null> {
    const out: Record<string, number | null> = {}
    this.list.forEach((r, i) => {
      const a = this.acc[i]!
      out[r.field] = r.op === `sum` ? round(a.sum) : r.op === `count` ? a.count : r.op === `avg` ? (a.nums ? round(a.sum / a.nums) : null) : r.op === `min` ? a.min : a.max
    })
    return out
  }
}

/** A group's part of the key: the author's id, a date's bucket key, any
 *  other field's text (VAPP-202: computed once per row). */
function groupPart(key: RecordsGroupKey, row: RecordRow): string {
  if (key.field === `author`) return recordAuthor(row) ?? ``
  const v = field(row, key.field)
  return key.bucket !== null ? (dateBucket(text(v), key.bucket) ?? ``) : text(v)
}

/** A group's columns (from its first row): `author` = the author's name (+
 *  `authorId`), a bucket = the date's bucket key, any other field its value. */
function groupColumns(key: RecordsGroupKey, row: RecordRow, names: Record<string, string> | undefined, cols: Record<string, unknown>): void {
  if (key.field === `author`) {
    const author = recordAuthor(row)
    cols.author = (author && names?.[author]) || ``
    cols.authorId = author
    return
  }
  const v = field(row, key.field)
  cols[key.field] = key.bucket !== null ? dateBucket(text(v), key.bucket) : jsonOf(v)
}

/** VAPP-214 (Q3): at most this many computed fields per kind. */
export const RECORDS_COMPUTED_MAX = 16
/** The row keys a computed field reads next to its kind's fields (`author`, a name, never). */
export const RECORDS_COMPUTED_ROW_KEYS: readonly string[] = [`id`, `createdAt`, `updatedAt`, `createdBy`, `authorId`]

/** VAPP-214 (Q3): a package's computed fields of `kind` (`records.<kind>.computed`,
 *  the string ones); undefined = none. What a host passes as `computed`. */
export function kindComputed(records: unknown, kind: string | null | undefined): Record<string, string> | undefined {
  const def = kind && isObject(records) && Object.prototype.hasOwnProperty.call(records, kind) ? records[kind] : undefined
  if (!isObject(def) || !isObject(def.computed)) return undefined
  const out: Record<string, string> = {}
  for (const [name, source] of Object.entries(def.computed)) if (typeof source === `string`) out[name] = source
  return Object.keys(out).length ? out : undefined
}

/** eApps R5 (VAPP-219): a package's refs of `kind` (`records.<kind>.refs`,
 *  the well-formed ones); undefined = none. What a host passes as `refs`. */
export function kindRefs(records: unknown, kind: string | null | undefined): Record<string, RecordRefTarget> | undefined {
  const def = kind && isObject(records) && Object.prototype.hasOwnProperty.call(records, kind) ? records[kind] : undefined
  if (!isObject(def) || !isObject(def.refs)) return undefined
  const out: Record<string, RecordRefTarget> = {}
  for (const [name, target] of Object.entries(def.refs)) {
    if (target === `issue` || target === `member`) out[name] = target
    else if (isObject(target) && typeof target.kind === `string` && target.kind) out[name] = { kind: target.kind }
  }
  return Object.keys(out).length ? out : undefined
}

/** A record as a value row: its data and the row keys (`author` = the name). */
function valueRow(row: RecordRow, names: Record<string, string> | undefined): Record<string, unknown> {
  const author = recordAuthor(row)
  return { ...row.data, id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt, createdBy: row.createdBy, authorId: author, author: (author && names?.[author]) || `` }
}

/** eApps R5 (VAPP-219): one ref's value inlined: a record of the target kind
 *  among `rows` (as a value row, its own refs not followed), an issue summary
 *  from `ctx.issues`, a member `{id, name}` from `ctx.names`; a dangling ref
 *  (deleted, unknown, not readable here) and an empty one read null. */
function expandedRef(target: RecordRefTarget, value: unknown, byId: () => Map<string, RecordRow>, ctx: RecordsContext): unknown {
  const id = typeof value === `string` ? value : typeof value === `number` ? String(value) : ``
  if (!id) return null
  if (target === `issue`) return ctx.issues && Object.prototype.hasOwnProperty.call(ctx.issues, id) ? ctx.issues[id] : null
  if (target === `member`) return ctx.names && Object.prototype.hasOwnProperty.call(ctx.names, id) ? { id, name: ctx.names[id] } : null
  const row = byId().get(id)
  return row && row.kind === target.kind ? valueRow(row, ctx.names) : null
}

/** VAPP-214 (Q3): `rows` with the computed fields of `kind` written into
 *  each of its rows' data (other kinds' rows as they are). A field reads
 *  the row's data and the row keys by name (a missing one = null); it never
 *  reads another computed field (the checker refuses it). A failing or
 *  unparsable expression reads null. Evaluated in UTC with the query's
 *  `now`, like the query's own buckets. Never stored. */
export function withComputed(rows: readonly RecordRow[], kind: string, computed: Record<string, string> | undefined, now?: string): readonly RecordRow[] {
  const names = computed ? Object.keys(computed).sort(byCodeUnits) : []
  if (!names.length) return rows
  const fields = names.map((name) => {
    const parsed = parseExpression(computed![name]!)
    return { name, ast: `ast` in parsed ? (parsed.ast as ExprNode) : null, reads: `ast` in parsed ? expressionNames(parsed.ast as ExprNode) : [] }
  })
  const env = { now: (now ? parseTimestamp(now) : null) ?? 0 }
  return rows.map((row) => {
    if (row.kind !== kind) return row
    const vars: Record<string, unknown> = { ...row.data, id: row.id, createdAt: row.createdAt, updatedAt: row.updatedAt, createdBy: row.createdBy, authorId: recordAuthor(row) }
    const data: Record<string, unknown> = { ...row.data }
    for (const f of fields) {
      for (const name of f.reads) if (!Object.prototype.hasOwnProperty.call(vars, name)) vars[name] = null
      const result = f.ast ? evaluateExpression(f.ast, vars, env) : null
      data[f.name] = result && `value` in result ? result.value : null
    }
    return { ...row, data }
  })
}

/** The value a records binding emits for `query` over `rows` (every row of
 *  the app, any kind): null without a kind; else {rows, count, sum?,
 *  month?, groups?, totals?, before?}: the matches sorted (ties by id)
 *  then cut at the limit, `count` and `sum` before the limit. */
export function recordsValue(rows: readonly RecordRow[], query: RecordsQuery, ctx: RecordsContext): RecordsValue | null {
  if (!query.from) return null
  const now = ctx.now ?? new Date().toISOString()
  const all = rows
  rows = withComputed(rows, query.from, ctx.computed, now)
  // eApps R5: the expanded fields that are refs of the kind (another name stays as stored).
  const expand = query.expand.flatMap((name) => (ctx.refs && Object.prototype.hasOwnProperty.call(ctx.refs, name) ? [{ name, target: ctx.refs[name]! }] : []))
  let index: Map<string, RecordRow> | null = null
  const byId = () => (index ??= new Map(all.map((r) => [r.id, r])))
  const view = query.month
  const today = ctx.today && /^\d{4}-\d{2}-\d{2}$/.test(ctx.today) ? ctx.today : now.slice(0, 10)
  const monthKey = view ? (view.key ?? today.slice(0, 7)) : null
  const where = query.where.map(cond)
  const or = query.or.map((alt) => alt.map(cond))
  const beforeDate = query.before ? keyOf(query.before.date) : null
  const before: RecordRow[] = []
  const matched = sorted(
    rows.filter((row) => {
      if (row.kind !== query.from) return false
      const author = recordAuthor(row)
      if (query.mine && (ctx.me === null || author !== ctx.me)) return false
      if (query.user !== null && author !== query.user) return false
      if (!where.every((c) => holds(c, field(row, c.filter.field)))) return false
      if (or.length && !or.some((alt) => alt.every((c) => holds(c, field(row, c.filter.field))))) return false
      if (query.before) {
        const t = text(field(row, query.before.field))
        if (t !== `` && compareKeys(keyOf(t), beforeDate!, t, query.before.date) < 0) {
          before.push(row)
          return false
        }
      }
      if (view && monthKey !== null && !text(row.data[view.field]).startsWith(monthKey)) return false
      return true
    }),
    query.sort,
  )
  // The before rows in the query's order too (first/last read it).
  const beforeRows = sorted(before, query.sort)
  const seeded = () => {
    const r = new Running(query.running)
    for (const row of beforeRows) r.add(row)
    return r
  }
  const rowRunning = query.running.length && !query.group.length ? seeded() : null
  const out: RecordsValue = {
    rows: matched.slice(0, query.limit).map((row) => {
      const value = valueRow(row, ctx.names)
      for (const e of expand) value[e.name] = expandedRef(e.target, row.data[e.name], byId, ctx)
      if (ctx.transitions) value[RECORD_TRANSITIONS_ROW_KEY] = transitionsFrom(ctx.transitions, row.data, { id: ctx.me, role: ctx.role ?? null }, row)
      if (rowRunning) {
        rowRunning.add(row)
        value.running = rowRunning.read()
      }
      return value
    }),
    count: matched.length,
  }
  if (query.sum !== null) out.sum = round(matched.reduce((acc, row) => acc + (num(row.data[query.sum!]) ?? 0), 0))
  if (view && monthKey !== null) {
    const days = monthDays(monthKey)
    const byDate = new Map(days.map((d) => [d.date, d]))
    const perDay = new Map<string, RecordRow[]>()
    for (const row of matched) {
      const date = text(row.data[view.field]).slice(0, 10)
      const day = byDate.get(date)
      if (!day) continue
      day.count += 1
      if (query.sum !== null) day.sum = round(day.sum + (num(row.data[query.sum]) ?? 0))
      const list = perDay.get(date)
      if (list) list.push(row)
      else perDay.set(date, [row])
    }
    if (query.running.length) {
      const r = seeded()
      for (const day of days) {
        for (const row of perDay.get(day.date) ?? []) r.add(row)
        day.running = r.read()
      }
    }
    out.month = {
      key: monthKey,
      label: monthLabel(monthKey),
      prev: shiftMonth(monthKey, -1),
      next: shiftMonth(monthKey, 1),
      today,
      days,
      labels: days.map((d) => String(d.day)),
      counts: days.map((d) => d.count),
      sums: days.map((d) => d.sum),
    }
  }
  if (query.group.length) {
    const groups = new Map<string, { parts: string[]; rows: RecordRow[]; out: Record<string, unknown> }>()
    // One part: the part is the id; several: joined (JSON only when a part
    // could hold the separator).
    const one = query.group.length === 1
    for (const row of matched) {
      const parts = query.group.map((key) => groupPart(key, row))
      const id = one ? parts[0]! : parts.some((p) => p.includes(`\u0000`)) ? JSON.stringify(parts) : parts.join(`\u0000`)
      const g = groups.get(id)
      if (g) g.rows.push(row)
      else {
        const cols: Record<string, unknown> = {}
        for (const key of query.group) groupColumns(key, row, ctx.names, cols)
        groups.set(id, { parts, rows: [row], out: cols })
      }
    }
    const list = [...groups.values()]
    for (const g of list) Object.assign(g.out, summary(query, g.rows))
    const columns = new Set([...query.group.flatMap((g) => (g.field === `author` ? [`author`, `authorId`] : [g.field])), `count`, ...(query.sum !== null ? [`sum`] : []), ...query.aggregate.map((a) => a.name)])
    const sortKeys = query.sort.filter((k) => columns.has(k.field))
    list.sort((a, b) => {
      for (const key of sortKeys) {
        const c = compare(a.out[key.field], b.out[key.field])
        if (c !== 0) return key.order === `asc` ? c : -c
      }
      for (const key of query.group) {
        const c = compare(a.out[key.field], b.out[key.field])
        if (c !== 0) return c
      }
      for (let i = 0; i < a.parts.length; i++) {
        const c = byCodeUnits(a.parts[i]!, b.parts[i]!)
        if (c !== 0) return c
      }
      return 0
    })
    if (query.running.length) {
      const r = seeded()
      for (const g of list) {
        for (const row of g.rows) r.add(row)
        g.out.running = r.read()
      }
    }
    out.groups = list.map((g) => g.out)
  }
  if (query.aggregate.length) out.totals = summary(query, matched)
  if (query.before) out.before = summary(query, beforeRows)
  return out
}
