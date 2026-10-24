// VAPP-122: app RECORDS, the pure reference every app host mirrors
// (fixtures/app-records.json is the spec; desktop `domain::app_records`,
// iOS `AppRecords.swift`, Android `AppRecords.kt` replay its cases). The
// `exp:records?…` binding's query grammar and the value it emits over the
// synced `app_records` rows.

import fixture from "../fixtures/app-records.json" with { type: "json" }
import { APPS_COPY } from "./apps"

export const APP_RECORDS_QUERY = fixture.query
export const APP_RECORD_ROW_KEYS: readonly string[] = fixture.rowKeys
const WEEKDAYS: readonly string[] = fixture.weekdays
const MONTHS: readonly string[] = fixture.months

export type RecordsFilterOp = `eq` | `gt` | `gte` | `lt` | `lte`

export interface RecordsFilter {
  field: string
  op: RecordsFilterOp
  value: string
}

export interface RecordsQuery {
  kind: string | null
  /** Another app's slug; null = the hosting app. */
  app: string | null
  mine: boolean
  user: string | null
  sort: string
  order: `asc` | `desc`
  limit: number
  /** By field, then operator (the fixture's order). */
  filters: RecordsFilter[]
  sum: string | null
  by: string | null
  month: string | null
}

const MONTH = /^\d{4}-(0[1-9]|1[0-2])$/
const OPS: readonly RecordsFilterOp[] = [`gt`, `gte`, `lt`, `lte`]

/** The `exp:records` query params (sorted keys, as `parseSource` gives
 *  them) → the query. Unknown keys are data-field filters. */
export function parseRecordsQuery(params: Record<string, string>): RecordsQuery {
  const reserved = new Set(APP_RECORDS_QUERY.reserved)
  const limitRaw = Number(params.limit)
  const limit = Number.isInteger(limitRaw) && limitRaw > 0 ? Math.min(limitRaw, APP_RECORDS_QUERY.limitMax) : APP_RECORDS_QUERY.limitDefault
  const filters: RecordsFilter[] = []
  for (const key of Object.keys(params).sort()) {
    if (reserved.has(key)) continue
    const dot = key.lastIndexOf(`.`)
    const suffix = dot >= 0 ? key.slice(dot + 1) : ``
    if (dot > 0 && (OPS as readonly string[]).includes(suffix)) filters.push({ field: key.slice(0, dot), op: suffix as RecordsFilterOp, value: params[key]! })
    else if (key) filters.push({ field: key, op: `eq`, value: params[key]! })
  }
  filters.sort((a, b) => (a.field < b.field ? -1 : a.field > b.field ? 1 : a.op < b.op ? -1 : a.op > b.op ? 1 : 0))
  return {
    kind: params.kind?.trim() || null,
    app: params.app?.trim() || null,
    mine: params.mine === `1` || params.mine === `true`,
    user: params.user?.trim() || null,
    sort: params.sort?.trim() || APP_RECORDS_QUERY.sortDefault,
    order: params.order === `asc` ? `asc` : params.order === `desc` ? `desc` : (APP_RECORDS_QUERY.orderDefault as `asc` | `desc`),
    limit,
    filters,
    sum: params.sum?.trim() || null,
    by: params.by?.trim() || null,
    month: params.month && MONTH.test(params.month) ? params.month : null,
  }
}

/** One synced `app_records` row as the host reads it. */
export interface AppRecordRow {
  id: string
  kind: string
  data: Record<string, unknown>
  createdBy: string | null
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
  /** ISO 8601 (UTC): the month view's default month. */
  now: string
  /** user id → display name (`author`; "" when unknown). */
  names: Record<string, string>
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

/** `a` against `b` the way a filter and a sort compare: numbers when both
 *  parse, else text (code points, no locale: the same ×4). */
function compare(a: unknown, b: unknown): number {
  const na = num(a)
  const nb = num(b)
  if (na !== null && nb !== null) return na < nb ? -1 : na > nb ? 1 : 0
  const ta = text(a)
  const tb = text(b)
  return ta < tb ? -1 : ta > tb ? 1 : 0
}

function matches(filter: RecordsFilter, value: unknown): boolean {
  if (filter.op === `eq`) return text(value) === filter.value
  const c = compare(value, filter.value)
  return filter.op === `gt` ? c > 0 : filter.op === `gte` ? c >= 0 : filter.op === `lt` ? c < 0 : c <= 0
}

/** `YYYY-MM` ± 1. */
export function shiftMonth(key: string, by: number): string {
  const [y, m] = key.split(`-`).map(Number) as [number, number]
  const index = y * 12 + (m - 1) + by
  const year = Math.floor(index / 12)
  const month = (index % 12) + 1
  return `${year}-${String(month).padStart(2, `0`)}`
}

export function monthLabel(key: string): string {
  const [y, m] = key.split(`-`).map(Number) as [number, number]
  return `${MONTHS[m - 1]} ${y}`
}

/** Every day of `key` (`YYYY-MM`), empty. */
export function monthDays(key: string): RecordsDay[] {
  const [y, m] = key.split(`-`).map(Number) as [number, number]
  const count = new Date(Date.UTC(y, m, 0)).getUTCDate()
  const days: RecordsDay[] = []
  for (let day = 1; day <= count; day++) {
    const date = `${key}-${String(day).padStart(2, `0`)}`
    days.push({ date, day, weekday: WEEKDAYS[new Date(Date.UTC(y, m - 1, day)).getUTCDay()]!, count: 0, sum: 0 })
  }
  return days
}

/** The value `exp:records` emits for `query` over `rows` (every synced row
 *  of the app, any kind): null without a kind; else {rows, count, sum?,
 *  month?} as the fixture spells out. */
export function recordsValue(rows: readonly AppRecordRow[], query: RecordsQuery, ctx: RecordsContext): RecordsValue | null {
  if (!query.kind) return null
  const monthKey = query.by ? (query.month ?? ctx.now.slice(0, 7)) : null
  const field = (row: AppRecordRow, name: string): unknown =>
    name === `createdAt` ? row.createdAt : name === `updatedAt` ? row.updatedAt : name === `id` ? row.id : name === `createdBy` ? row.createdBy : row.data[name]
  let matched = rows.filter((row) => {
    if (row.kind !== query.kind) return false
    if (query.mine && (ctx.me === null || row.createdBy !== ctx.me)) return false
    if (query.user !== null && row.createdBy !== query.user) return false
    if (monthKey !== null && query.by && !text(row.data[query.by]).startsWith(monthKey)) return false
    return query.filters.every((f) => matches(f, field(row, f.field)))
  })
  matched = [...matched].sort((a, b) => {
    const c = compare(field(a, query.sort), field(b, query.sort))
    if (c !== 0) return query.order === `asc` ? c : -c
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  })
  const out: RecordsValue = {
    rows: matched.slice(0, query.limit).map((row) => ({
      ...row.data,
      id: row.id,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
      createdBy: row.createdBy,
      author: (row.createdBy && ctx.names[row.createdBy]) || ``,
    })),
    count: matched.length,
  }
  if (query.sum !== null) out.sum = round(matched.reduce((acc, row) => acc + (num(row.data[query.sum!]) ?? 0), 0))
  if (monthKey !== null && query.by) {
    const days = monthDays(monthKey)
    const byDate = new Map(days.map((d) => [d.date, d]))
    for (const row of matched) {
      const day = byDate.get(text(row.data[query.by]).slice(0, 10))
      if (!day) continue
      day.count += 1
      if (query.sum !== null) day.sum = round(day.sum + (num(row.data[query.sum]) ?? 0))
    }
    out.month = {
      key: monthKey,
      label: monthLabel(monthKey),
      prev: shiftMonth(monthKey, -1),
      next: shiftMonth(monthKey, 1),
      today: ctx.now.slice(0, 10),
      days,
      labels: days.map((d) => String(d.day)),
      counts: days.map((d) => d.count),
      sums: days.map((d) => d.sum),
    }
  }
  return out
}

/** Sums of hours like 0.1 + 0.2 stay readable (and identical ×4). */
function round(n: number): number {
  return Math.round(n * 1_000_000) / 1_000_000
}

/** Settings → Apps → the app: "1,240 records", "1,240 of 25,000 records"
 *  (max = the team's quota, null = unlimited), "No records". */
export function recordsCountLabel(count: number, max: number | null): string {
  if (count === 0) return APPS_COPY.recordsNone
  const n = count.toLocaleString(`en-US`)
  if (max === null) return count === 1 ? APPS_COPY.recordsOne : APPS_COPY.records.replace(`{n}`, n)
  return APPS_COPY.recordsOf.replace(`{n}`, n).replace(`{max}`, max.toLocaleString(`en-US`))
}
