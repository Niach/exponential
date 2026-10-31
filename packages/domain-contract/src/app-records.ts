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

/** VAPP-161: a parsed binding source → the params `parseRecordsQuery`
 *  takes, for both spellings: the format's neutral `records:<kind>?q` (the
 *  name wins over a `kind` param) and `exp:records?kind=<kind>&q`; null =
 *  not a records source. */
export function recordsSourceParams(source: { scheme: string; name: string; params: Record<string, string> }): Record<string, string> | null {
  if (source.scheme === `records`) return { ...source.params, kind: source.name }
  if (source.scheme === `exp` && source.name === `records`) return source.params
  return null
}

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

/** VAPP-159 PURGE: how long a deleted record (or a trashed app's record)
 *  stays before the server hard-deletes it, and the rows per sweep. */
export const APP_RECORDS_PURGE = { retentionDays: fixture.purge.retentionDays, batch: fixture.purge.batch }
const RETENTION_MS = APP_RECORDS_PURGE.retentionDays * 24 * 60 * 60 * 1000

/** A record is due for the purge once its own `deleted_at` or its app's
 *  trash stamp (`app_deleted_at`) is older than the retention. */
export function appRecordPurgeDue(deletedAt: Date | null, appDeletedAt: Date | null, now: Date): boolean {
  const cutoff = now.getTime() - RETENTION_MS
  return (deletedAt !== null && deletedAt.getTime() < cutoff) || (appDeletedAt !== null && appDeletedAt.getTime() < cutoff)
}

// VAPP-178 THE DATA TAB (fixture `dataTab`): the editor's records browser
// and form, pure so the four clients replay the same cases.

export const APP_DATA_TAB = fixture.dataTab
export const APP_DATA_COPY = fixture.dataTab.copy

/** A kind's schema field (the SDK's JSON Schema subset, host/records.ts). */
export interface DataField {
  type: `string` | `number` | `integer` | `boolean` | `array`
  enum?: string[]
  items?: { type: `string` | `number` | `integer` | `boolean` }
  format?: `date` | `date-time` | `time`
}

export interface DataSchema {
  type?: `object`
  properties: Record<string, DataField>
  required?: string[]
}

/** The head manifest's kinds, sorted by name, each with its live count
 *  (`kindCases`); a row of an undeclared kind is ignored. */
export function dataKinds(
  manifest: { records?: Record<string, { schema: DataSchema }> } | null | undefined,
  rows: readonly { kind: string }[]
): { kind: string; count: number }[] {
  const kinds = Object.keys(manifest?.records ?? {}).sort()
  return kinds.map((kind) => ({ kind, count: rows.filter((row) => row.kind === kind).length }))
}

/** The columns (`columnCases`): the `required` names in their order, then
 *  every other property sorted by name (never the declaration order: a
 *  host's JSON layer may lose it). */
export function dataColumns(schema: DataSchema): string[] {
  const names = Object.keys(schema.properties)
  const required = (schema.required ?? []).filter((name, i, all) => names.includes(name) && all.indexOf(name) === i)
  return [...required, ...names.filter((name) => !required.includes(name)).sort()]
}

/** A kind's rows, `updatedAt` desc, ties by id (`rowsCases`). */
export function dataRows<T extends { id: string; kind: string; updatedAt: string }>(rows: readonly T[], kind: string): T[] {
  return rows
    .filter((row) => row.kind === kind)
    .sort((a, b) => (a.updatedAt === b.updatedAt ? (a.id < b.id ? -1 : a.id > b.id ? 1 : 0) : a.updatedAt < b.updatedAt ? 1 : -1))
}

/** One cell's text (`cellCases`). */
export function cellText(value: unknown): string {
  if (value === null || value === undefined) return ``
  if (typeof value === `boolean`) return value ? APP_DATA_COPY.yes : APP_DATA_COPY.no
  if (typeof value === `number`) return String(value)
  if (typeof value === `string`) return value
  if (Array.isArray(value)) return value.map(cellText).join(`, `)
  return JSON.stringify(value)
}

export type DataControl = `text` | `number` | `switch` | `select` | `list`

export interface DataFormField {
  name: string
  control: DataControl
  required: boolean
  placeholder?: string
  options?: string[]
  itemType?: `string` | `number` | `integer` | `boolean`
}

/** The form's fields, in column order (`formCases`). */
export function formFields(schema: DataSchema): DataFormField[] {
  const required = schema.required ?? []
  return dataColumns(schema).map((name) => {
    const field = schema.properties[name]!
    const base = { name, required: required.includes(name) }
    if (field.type === `boolean`) return { ...base, control: `switch` }
    if (field.type === `number` || field.type === `integer`) return { ...base, control: `number` }
    if (field.type === `array`) return { ...base, control: `list`, itemType: field.items?.type ?? `string` }
    if (field.enum) return { ...base, control: `select`, options: field.enum }
    const placeholder = field.format ? (APP_DATA_TAB.placeholders as Record<string, string>)[field.format] : undefined
    return placeholder ? { ...base, control: `text`, placeholder } : { ...base, control: `text` }
  })
}

/** A stored value → the control's initial input (`inputCases`): a switch
 *  takes a boolean, every other control text. */
export function formInput(control: DataControl, value: unknown): string | boolean {
  if (control === `switch`) return value === true
  if (value === null || value === undefined) return ``
  if (control === `list` && Array.isArray(value)) return value.map((item) => cellText(item)).join(`, `)
  return typeof value === `string` ? value : String(value)
}

function parseNumber(text: string): number | string {
  const n = Number(text)
  return text.trim() !== `` && Number.isFinite(n) ? n : text
}

function parseItem(text: string, type: DataFormField[`itemType`]): unknown {
  if (type === `number` || type === `integer`) return parseNumber(text)
  if (type === `boolean`) return text === `true` ? true : text === `false` ? false : text
  return text
}

/** The inputs → the record's data (`parseCases`): an empty field = null,
 *  numbers and list items parse (unparsable stays the raw text, so the
 *  server names it); CREATE omits the nulls, EDIT keeps them (null clears). */
export function formData(schema: DataSchema, inputs: Record<string, string | boolean | undefined>, editing: boolean): Record<string, unknown> {
  const data: Record<string, unknown> = {}
  for (const field of formFields(schema)) {
    const input = inputs[field.name]
    let value: unknown
    if (field.control === `switch`) value = input === true
    else {
      const text = typeof input === `string` ? input : ``
      if (field.control === `list`) {
        const items = text
          .split(`,`)
          .map((item) => item.trim())
          .filter((item) => item.length > 0)
        value = items.length > 0 ? items.map((item) => parseItem(item, field.itemType)) : null
      } else if (text === ``) value = null
      else value = field.control === `number` ? parseNumber(text) : text
    }
    if (value !== null || editing) data[field.name] = value
  }
  return data
}

/** A column's fixed width in px (`columns.typeWidths`), or null = it shares
 *  the rest of the row. */
export function columnWidth(field: DataField): number | null {
  const widths = APP_DATA_TAB.columns.typeWidths as Record<string, number>
  if (field.type === `string`) return field.format ? (widths[field.format] ?? null) : null
  return widths[field.type] ?? null
}
