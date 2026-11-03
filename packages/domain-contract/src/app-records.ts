// VAPP-122: app RECORDS, the Exponential-only rules every app host mirrors
// (fixtures/app-records.json is the spec; desktop `domain::app_records`,
// iOS `AppRecords.swift`, Android `AppRecords.kt` replay its cases): the
// ownership + on-behalf rules, the count label, the purge and the Data tab.
// VAPP-189: the `records:<kind>?…` query and its value live in eUI
// (`@exponential_app/eui/host/query`, one evaluator ×4,
// packages/exponential-ui fixtures/records-query.json).

import fixture from "../fixtures/app-records.json" with { type: "json" }
import { APPS_COPY } from "./apps"

/** The keys every record row carries; a kind's schema may not declare them. */
export const APP_RECORD_ROW_KEYS: readonly string[] = fixture.rowKeys

/** One synced `app_records` row as the host reads it. */
export interface AppRecordRow {
  id: string
  kind: string
  data: Record<string, unknown>
  createdBy: string | null
  /** VAPP-187: whose record it is (null/missing on a legacy row = `createdBy`). */
  authorId?: string | null
  createdAt: string
  updatedAt: string
}

/** VAPP-187: a record's AUTHOR (`authorId ?? createdBy`): whose record it
 *  is, what `mine`/`user=` and the per-member views read. */
export function recordAuthor(row: { authorId?: string | null; createdBy: string | null }): string | null {
  return row.authorId ?? row.createdBy ?? null
}

/** VAPP-187 OWNERSHIP (fixture `ownership`): the default per-kind policy and
 *  the server's refusal. */
export const APP_RECORDS_OWNERSHIP = {
  policy: fixture.ownership.policy as Record<`update` | `delete`, `author` | `member`>,
  refusal: fixture.ownership.refusal,
}

/** May `viewer` (with team `role`; null = not a member) `op` the record?
 *  Policy `author` = the author or a team owner (`canChangeCases`). */
export function canChangeRecord(
  row: { authorId?: string | null; createdBy: string | null },
  viewer: string | null,
  role: string | null,
  op: `update` | `delete` = `update`
): boolean {
  if (viewer === null || role === null) return false
  if (role === `owner`) return true
  if (APP_RECORDS_OWNERSHIP.policy[op] === `member`) return true
  const author = recordAuthor(row)
  return author !== null && author === viewer
}

/** VAPP-188 ON-BEHALF (fixture `onBehalf`): who may name another member as
 *  a write's author, and the server's refusals. */
export const APP_RECORDS_ON_BEHALF = {
  refusal: fixture.onBehalf.refusal,
  notMember: fixture.onBehalf.notMember,
}

export type WriteAsVerdict = `ok` | `forbidden` | `notMember`

/** May `viewer` (team `role`; null = not a member) write a record as
 *  `author` (null = as themself)? Another member = owners only, and the
 *  author must be one of `members` (`writeAsCases`). */
export function writeAsVerdict(viewer: string | null, role: string | null, author: string | null | undefined, members: readonly string[]): WriteAsVerdict {
  if (viewer === null || role === null) return `forbidden`
  if (author == null || author === viewer) return `ok`
  if (role !== `owner`) return `forbidden`
  return members.includes(author) ? `ok` : `notMember`
}

/** An imported record's author (`importCases`): absent, the importer or a
 *  stranger to the team = the importer; another member = them when the
 *  importer is an owner, else the importer too (VAPP-192: a member
 *  importing a teammate's export keeps the rows as their own). */
export function importAuthor(importer: string, role: string | null, author: string | null | undefined, members: readonly string[]): string {
  if (author == null || author === importer || !members.includes(author)) return importer
  return role === `owner` ? author : importer
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

/** VAPP-188: the `by` cell (`bylineCases`): the author's name (else
 *  `fallback`, apps.json `copy.byAgent`), `{writer} for {author}` when
 *  someone else with a name wrote it. */
export function recordByline(
  row: { authorId?: string | null; createdBy: string | null },
  names: Readonly<Record<string, string>>,
  fallback: string
): string {
  const authorId = recordAuthor(row)
  const author = (authorId !== null ? names[authorId] : undefined) || fallback
  const writer = row.createdBy !== null && row.createdBy !== authorId ? names[row.createdBy] : undefined
  return writer ? APP_DATA_COPY.byFor.replace(`{writer}`, writer).replace(`{author}`, author) : author
}

/** VAPP-188: the owner's Author picker, members by name (case-insensitive,
 *  ties by id: `authorOptionCases`). */
export function authorOptions<T extends { id: string; name: string }>(users: readonly T[]): T[] {
  return [...users].sort((a, b) => {
    const x = a.name.toLowerCase()
    const y = b.name.toLowerCase()
    return x < y ? -1 : x > y ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  })
}

/** VAPP-188: the picker's default (the record's author on edit, the viewer
 *  on create) and what Save sends (`send`: the pick when it differs from
 *  the record's author / the viewer, else null = omit; `formAuthorCases`). */
export function formAuthor(
  viewer: string | null,
  record: { authorId?: string | null; createdBy: string | null } | null,
  picked: string | null
): { default: string | null; send: string | null } {
  const base = record ? recordAuthor(record) : viewer
  return { default: base ?? viewer, send: picked !== null && picked !== base ? picked : null }
}
