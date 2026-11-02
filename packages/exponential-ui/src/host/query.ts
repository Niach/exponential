// VAPP-189 (S1, docs/package-format.md "Records"): THE records query, one
// evaluator for every host. A binding's URL sugar (`records:<kind>?q`, a
// host's own spelling like Exponential's `exp:records?kind=<kind>&q`)
// parses INTO the structured `RecordsQuery`; `recordsValue` evaluates it
// over the rows of any `RecordStore` (record-store.ts). Pure, so the Rust
// core mirrors it line by line (`eui-core` `host/query.rs`, reached by the
// Swift and Compose SDKs through the FFI's `parse_records_query_json` /
// `records_value_json`); fixtures/records-query.json is the spec.
//
// The parsed query is the canonical form's subset (`from`, `where`, `sort`,
// `limit`): a JSON query form (VAPP-190: group-by, aggregates, `in` /
// `between` / `or`) adds keys next to these, never changes them.

import type { ParsedSource } from "./sources"

/** The binding scheme: `records:<kind>[?query]`. */
export const RECORDS_SCHEME = `records`
/** The URL sugar's defaults and caps (catalog/host.json `package.records.query`). */
export const RECORDS_QUERY = { sortDefault: `createdAt`, orderDefault: `desc`, limitDefault: 100, limitMax: 500 } as const
/** Query keys with a meaning of their own; never a field filter. */
export const RECORDS_RESERVED_PARAMS: readonly string[] = [`app`, `by`, `kind`, `limit`, `mine`, `month`, `order`, `sort`, `sum`, `user`]
/** The keys every value row carries next to its data (a data key never shadows one). */
export const RECORDS_ROW_KEYS: readonly string[] = [`id`, `createdAt`, `updatedAt`, `createdBy`, `authorId`, `author`]
/** The filter operators: `eq` is the bare key, the others a `.<op>` suffix. */
export const RECORDS_FILTER_OPS: readonly RecordsFilterOp[] = [`eq`, `gt`, `gte`, `lt`, `lte`]
/** The month view's day and month names (en-US, the same ×4; query.test.ts pins them to the fixture, which stays out of the bundle). */
export const RECORDS_WEEKDAYS: readonly string[] = [`Sun`, `Mon`, `Tue`, `Wed`, `Thu`, `Fri`, `Sat`]
export const RECORDS_MONTHS: readonly string[] = [`January`, `February`, `March`, `April`, `May`, `June`, `July`, `August`, `September`, `October`, `November`, `December`]

export type RecordsFilterOp = `eq` | `gt` | `gte` | `lt` | `lte`
export type RecordsOrder = `asc` | `desc`

/** One condition: `eq` compares as text, the ranges as numbers when both
 *  sides parse, else as text. The URL sugar's values are strings. */
export interface RecordsFilter {
  field: string
  op: RecordsFilterOp
  value: string
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

export interface RecordsQuery {
  /** The kind; null = no kind (the value is null). */
  from: string | null
  /** Another app's slug (the HOST resolves whose rows); null = the hosting app. */
  app: string | null
  /** Only the viewer's rows (their author: `authorId ?? createdBy`). */
  mine: boolean
  /** Only this user's rows (by author). */
  user: string | null
  /** Every condition holds; by field, then operator. */
  where: RecordsFilter[]
  /** Sort keys in order; ties always fall back to the id. */
  sort: RecordsSortKey[]
  limit: number
  /** Σ of this field over the matches (before the limit). */
  sum: string | null
  month: RecordsMonthView | null
}

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
}

export interface RecordsContext {
  /** The viewer's user id (null = signed out: `mine` matches nothing). */
  me: string | null
  /** ISO 8601 (UTC): the month view's default month and `today`; default = the clock. */
  now?: string
  /** user id → display name (`author`; "" when unknown). */
  names?: Record<string, string>
}

const MONTH_KEY = /^\d{4}-(0[1-9]|1[0-2])$/
const SUFFIX_OPS: readonly string[] = [`gt`, `gte`, `lt`, `lte`]

const trimmed = (v: string | null | undefined): string | null => v?.trim() || null

/** Query params (as `parseSource` gives them) → the query. `kind` = the
 *  source's own name (the neutral `records:<kind>`), which wins over a
 *  `kind` param. Unknown keys are field filters. */
export function parseRecordsQuery(params: Record<string, string>, kind?: string | null): RecordsQuery {
  const limitRaw = Number(params.limit)
  const where: RecordsFilter[] = []
  for (const key of Object.keys(params).sort()) {
    if (RECORDS_RESERVED_PARAMS.includes(key) || !key) continue
    const dot = key.lastIndexOf(`.`)
    const suffix = dot >= 0 ? key.slice(dot + 1) : ``
    if (dot > 0 && SUFFIX_OPS.includes(suffix)) where.push({ field: key.slice(0, dot), op: suffix as RecordsFilterOp, value: params[key]! })
    else where.push({ field: key, op: `eq`, value: params[key]! })
  }
  where.sort((a, b) => (a.field < b.field ? -1 : a.field > b.field ? 1 : a.op < b.op ? -1 : a.op > b.op ? 1 : 0))
  const by = trimmed(params.by)
  return {
    from: trimmed(kind ?? params.kind),
    app: trimmed(params.app),
    mine: params.mine === `1` || params.mine === `true`,
    user: trimmed(params.user),
    where,
    sort: [{ field: trimmed(params.sort) ?? RECORDS_QUERY.sortDefault, order: params.order === `asc` ? `asc` : params.order === `desc` ? `desc` : RECORDS_QUERY.orderDefault }],
    limit: Number.isInteger(limitRaw) && limitRaw > 0 ? Math.min(limitRaw, RECORDS_QUERY.limitMax) : RECORDS_QUERY.limitDefault,
    sum: trimmed(params.sum),
    month: by ? { field: by, key: params.month && MONTH_KEY.test(params.month) ? params.month : null } : null,
  }
}

/** `records:<kind>?…` → the query; null = not a records source. */
export function parseRecordsSource(source: Pick<ParsedSource, `scheme` | `name` | `params`>): RecordsQuery | null {
  return source.scheme === RECORDS_SCHEME ? parseRecordsQuery(source.params, source.name) : null
}

const num = (v: unknown): number | null => {
  if (typeof v === `number`) return Number.isFinite(v) ? v : null
  if (typeof v === `string` && v.trim() !== ``) {
    const n = Number(v)
    return Number.isFinite(n) ? n : null
  }
  return null
}
const text = (v: unknown): string => (v === null || v === undefined ? `` : typeof v === `string` ? v : typeof v === `object` ? JSON.stringify(v) : String(v))

/** Numbers when both parse, else text (code units, no locale: the same ×4). */
function compare(a: unknown, b: unknown): number {
  const na = num(a)
  const nb = num(b)
  if (na !== null && nb !== null) return na < nb ? -1 : na > nb ? 1 : 0
  const ta = text(a)
  const tb = text(b)
  return ta < tb ? -1 : ta > tb ? 1 : 0
}

function holds(filter: RecordsFilter, value: unknown): boolean {
  if (filter.op === `eq`) return text(value) === text(filter.value)
  const c = compare(value, filter.value)
  return filter.op === `gt` ? c > 0 : filter.op === `gte` ? c >= 0 : filter.op === `lt` ? c < 0 : c <= 0
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
  const count = new Date(Date.UTC(y, m, 0)).getUTCDate()
  const days: RecordsDay[] = []
  for (let day = 1; day <= count; day++) {
    days.push({ date: `${key}-${String(day).padStart(2, `0`)}`, day, weekday: RECORDS_WEEKDAYS[new Date(Date.UTC(y, m - 1, day)).getUTCDay()]!, count: 0, sum: 0 })
  }
  return days
}

/** Sums of hours like 0.1 + 0.2 stay readable (and identical ×4). */
function round(n: number): number {
  return Math.round(n * 1_000_000) / 1_000_000
}

/** The value a records binding emits for `query` over `rows` (every row of
 *  the app, any kind): null without a kind; else {rows, count, sum?,
 *  month?}: the matches sorted (ties by id) then cut at the limit, `count`
 *  and `sum` before the limit. */
export function recordsValue(rows: readonly RecordRow[], query: RecordsQuery, ctx: RecordsContext): RecordsValue | null {
  if (!query.from) return null
  const now = ctx.now ?? new Date().toISOString()
  const view = query.month
  const monthKey = view ? (view.key ?? now.slice(0, 7)) : null
  const matched = rows
    .filter((row) => {
      if (row.kind !== query.from) return false
      const author = recordAuthor(row)
      if (query.mine && (ctx.me === null || author !== ctx.me)) return false
      if (query.user !== null && author !== query.user) return false
      if (view && monthKey !== null && !text(row.data[view.field]).startsWith(monthKey)) return false
      return query.where.every((f) => holds(f, field(row, f.field)))
    })
    .sort((a, b) => {
      for (const key of query.sort) {
        const c = compare(field(a, key.field), field(b, key.field))
        if (c !== 0) return key.order === `asc` ? c : -c
      }
      return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
    })
  const out: RecordsValue = {
    rows: matched.slice(0, query.limit).map((row) => {
      const author = recordAuthor(row)
      return {
        ...row.data,
        id: row.id,
        createdAt: row.createdAt,
        updatedAt: row.updatedAt,
        createdBy: row.createdBy,
        authorId: author,
        author: (author && ctx.names?.[author]) || ``,
      }
    }),
    count: matched.length,
  }
  if (query.sum !== null) out.sum = round(matched.reduce((acc, row) => acc + (num(row.data[query.sum!]) ?? 0), 0))
  if (view && monthKey !== null) {
    const days = monthDays(monthKey)
    const byDate = new Map(days.map((d) => [d.date, d]))
    for (const row of matched) {
      const day = byDate.get(text(row.data[view.field]).slice(0, 10))
      if (!day) continue
      day.count += 1
      if (query.sum !== null) day.sum = round(day.sum + (num(row.data[query.sum]) ?? 0))
    }
    out.month = {
      key: monthKey,
      label: monthLabel(monthKey),
      prev: shiftMonth(monthKey, -1),
      next: shiftMonth(monthKey, 1),
      today: now.slice(0, 10),
      days,
      labels: days.map((d) => String(d.day)),
      counts: days.map((d) => d.count),
      sums: days.map((d) => d.sum),
    }
  }
  return out
}
