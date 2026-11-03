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

// VAPP-215 (R4) RECORD HISTORY + UNDO (fixture `history`): what one
// `app_record_events` row means, its undo and its History row, pure so the
// four clients replay the same cases.

export const APP_RECORD_HISTORY = fixture.history
export const APP_HISTORY_COPY = fixture.history.copy

export type RecordEventOp = `create` | `update` | `delete` | `restore`
export type RecordEventVia = `editor` | `app` | `mcp` | `action` | `import` | `migration`

/** The part of an event the pure rules read. */
export interface RecordEventCore {
  op: RecordEventOp
  before: Record<string, unknown> | null
  patch: Record<string, unknown> | null
}

/** One event as `apps.records.history` returns it (newest first). */
export interface RecordEvent extends RecordEventCore {
  id: string
  actor: string | null
  createdAt: string
}

/** The write that undoes an event: a delete, or a restore of `data`. */
export type InverseWrite = { op: `delete` } | { op: `restore`; data: Record<string, unknown> }

/** The record an event left (`stateCases`): null = gone. */
export function eventState(event: RecordEventCore): Record<string, unknown> | null {
  if (event.op === `delete`) return null
  if (event.op === `update`) return { ...(event.before ?? {}), ...(event.patch ?? {}) }
  return event.patch ?? {}
}

/** Undo = a new write of the event's `before` (`inverseCases`). */
export function inverseWrite(event: RecordEventCore): InverseWrite {
  return event.before === null ? { op: `delete` } : { op: `restore`, data: event.before }
}

/** Deep JSON equality (objects by key, numbers by value). */
export function jsonEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true
  if (a === null || b === null || typeof a !== `object` || typeof b !== `object`) return false
  if (Array.isArray(a) !== Array.isArray(b)) return false
  if (Array.isArray(a)) {
    const other = b as unknown[]
    return a.length === other.length && a.every((item, i) => jsonEqual(item, other[i]))
  }
  const ka = Object.keys(a as object)
  const kb = Object.keys(b as object)
  return ka.length === kb.length && ka.every((k) => k in (b as object) && jsonEqual((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]))
}

/** A History row's title (`labelCases`). */
export function eventLabel(event: RecordEventCore): string {
  const c = APP_HISTORY_COPY
  if (event.op === `create`) return c.created
  if (event.op === `delete`) return c.deleted
  if (event.op === `restore`) return c.restored
  const before = event.before ?? {}
  const patch = event.patch ?? {}
  const changed = Object.keys(patch)
    .filter((k) => !jsonEqual(before[k], patch[k]))
    .sort()
  if (changed.length === 0) return c.saved
  const max = APP_RECORD_HISTORY.maxFields
  const shown = changed.slice(0, max).join(`, `)
  const rest = changed.length - max
  return c.changed.replace(`{fields}`, rest > 0 ? `${shown} ${c.more.replace(`{n}`, String(rest))}` : shown)
}

export interface HistoryRow {
  id: string
  label: string
  who: string
  at: string
  restorable: boolean
}

/** The History list (`rowsCases`): the events in the server's order (newest
 *  first); Restore on every row but the newest, never on a delete. */
export function historyRows(events: readonly RecordEvent[], names: Readonly<Record<string, string>>, fallback: string): HistoryRow[] {
  return events.map((event, i) => ({
    id: event.id,
    label: eventLabel(event),
    who: (event.actor !== null ? names[event.actor] : undefined) ?? fallback,
    at: event.createdAt,
    restorable: i > 0 && event.op !== `delete`,
  }))
}

/** The Undo toast's title for a UI write. */
export function undoTitle(write: `create` | `update` | `delete` | `batch`): string {
  const c = APP_HISTORY_COPY
  return write === `create` ? c.undoCreate : write === `update` ? c.undoUpdate : write === `delete` ? c.undoDelete : c.undoBatch
}

// VAPP-235 (O1) THE OFFLINE OUTBOX (fixture `outbox`): the desktop, iOS and
// Android app hosts queue every record write, lay it over the synced rows
// and replay it in order with an idempotency key; pure so the three hosts
// replay the same cases (the web stays online-only).

export const APP_RECORDS_OUTBOX = fixture.outbox
export const APP_OUTBOX_COPY = fixture.outbox.copy

export type OutboxCall = `upsert` | `delete` | `batch`
export type OutboxWrite = { op: `create` | `update`; id: string; kind: string; data: Record<string, unknown> } | { op: `delete`; id: string }
export interface OutboxEntry {
  writeId: string
  appId: string
  call: OutboxCall
  writes: OutboxWrite[]
  /** ISO time the write was made. */
  at: string
  state: `pending` | `acked`
  ackedAt?: string | null
}
/** A replay's answer: ok, no network, or the server's tRPC code. */
export type OutboxResult = { ok: true } | { transport: true } | { code: string }
export type OutboxOutcome = `done` | `retry` | `dropped` | `refused`

type Obj = Record<string, unknown>
const isObj = (v: unknown): v is Obj => typeof v === `object` && v !== null && !Array.isArray(v)

/** A host call's writes (`writesCases`): a create takes the next client id. */
export function outboxWrites(call: OutboxCall, args: Obj, newIds: readonly string[]): OutboxWrite[] {
  let next = 0
  const upsert = (w: Obj): OutboxWrite => {
    const data = isObj(w.data) ? w.data : {}
    const kind = typeof w.kind === `string` ? w.kind : ``
    return typeof w.id === `string` ? { op: `update`, id: w.id, kind, data } : { op: `create`, id: newIds[next++] ?? ``, kind, data }
  }
  if (call === `upsert`) return [upsert(args)]
  if (call === `delete`) return [{ op: `delete`, id: typeof args.id === `string` ? args.id : `` }]
  const writes = Array.isArray(args.writes) ? args.writes.filter(isObj) : []
  return writes.map((w) => (w.op === `delete` ? { op: `delete`, id: typeof w.id === `string` ? w.id : `` } : upsert(w)))
}

/** The replay request of an entry (`requestCases`); the host adds the app. */
export function outboxRequest(entry: OutboxEntry): { call: OutboxCall; input: Obj } {
  const { writeId } = entry
  if (entry.call === `batch`) {
    const writes = entry.writes.map((w) =>
      w.op === `delete` ? { op: `delete`, id: w.id } : w.op === `create` ? { op: `upsert`, kind: w.kind, data: w.data, newId: w.id } : { op: `upsert`, kind: w.kind, data: w.data, id: w.id }
    )
    return { call: `batch`, input: { writes, writeId } }
  }
  const w = entry.writes[0]!
  if (w.op === `delete`) return { call: `delete`, input: { recordId: w.id, writeId } }
  return { call: `upsert`, input: { kind: w.kind, data: w.data, ...(w.op === `create` ? { newId: w.id } : { recordId: w.id }), writeId } }
}

/** The visible rows: the synced rows with every entry laid over them, oldest
 *  first (`rowsCases`). */
export function outboxRows(rows: readonly AppRecordRow[], entries: readonly OutboxEntry[], me: string | null): AppRecordRow[] {
  const out = rows.map((r) => ({ ...r }))
  for (const entry of entries) {
    for (const w of entry.writes) {
      const i = out.findIndex((r) => r.id === w.id)
      if (w.op === `delete`) {
        if (i >= 0) out.splice(i, 1)
      } else if (i >= 0) {
        out[i] = { ...out[i]!, data: { ...out[i]!.data, ...w.data } }
      } else if (w.op === `create`) {
        out.push({ id: w.id, kind: w.kind, data: { ...w.data }, createdBy: me, authorId: me, createdAt: entry.at, updatedAt: entry.at })
      }
    }
  }
  return out
}

/** What a replay's answer does to its entry (`outcomeCases`). */
export function outboxOutcome(entry: OutboxEntry, result: OutboxResult): OutboxOutcome {
  if (`ok` in result) return `done`
  if (`transport` in result) return `retry`
  if (APP_RECORDS_OUTBOX.retryCodes.includes(result.code)) return `retry`
  if (result.code === `NOT_FOUND` && entry.writes.every((w) => w.op !== `create`) && entry.call !== `batch`) return `dropped`
  return `refused`
}

/** The failed-action toast's verb for a refused entry. */
export function outboxVerb(entry: OutboxEntry): `save` | `delete` {
  return entry.writes.every((w) => w.op === `delete`) ? `delete` : `save`
}

/** One flush (`flushCases`): pending entries oldest first, one at a time;
 *  a retry stops it; `results` = each write id's answer. */
export function outboxFlush(
  entries: readonly OutboxEntry[],
  results: Record<string, OutboxResult>,
  now: string
): { sent: string[]; refused: { writeId: string; verb: `save` | `delete` }[]; after: OutboxEntry[] } {
  const sent: string[] = []
  const refused: { writeId: string; verb: `save` | `delete` }[] = []
  const after: OutboxEntry[] = []
  let stopped = false
  for (const entry of entries) {
    if (stopped || entry.state === `acked`) {
      after.push(entry)
      continue
    }
    sent.push(entry.writeId)
    const outcome = outboxOutcome(entry, results[entry.writeId] ?? { transport: true })
    if (outcome === `done`) after.push({ ...entry, state: `acked`, ackedAt: now })
    else if (outcome === `retry`) {
      stopped = true
      after.push(entry)
    } else if (outcome === `refused`) refused.push({ writeId: entry.writeId, verb: outboxVerb(entry) })
  }
  return { sent, refused, after }
}

/** Do the synced rows reflect an entry? */
export function outboxReflected(entry: OutboxEntry, rows: readonly AppRecordRow[]): boolean {
  return entry.writes.every((w) => {
    const row = rows.find((r) => r.id === w.id)
    if (w.op === `delete`) return row === undefined
    return row !== undefined && Object.entries(w.data).every(([k, v]) => jsonEqual(row.data[k], v))
  })
}

/** The entries that stay (`pruneCases`): pending ones, and acked ones the
 *  rows don't reflect yet within the grace. */
export function outboxPrune(entries: readonly OutboxEntry[], rows: readonly AppRecordRow[], now: string): OutboxEntry[] {
  const grace = APP_RECORDS_OUTBOX.ackGraceSeconds * 1000
  return entries.filter((e) => {
    if (e.state !== `acked`) return true
    if (outboxReflected(e, rows)) return false
    return Date.parse(now) - Date.parse(e.ackedAt ?? e.at) < grace
  })
}

/** The header glyph's label for an app's pending entries (`pendingCases`);
 *  null = no glyph. */
export function outboxPendingLabel(entries: readonly OutboxEntry[], appId: string): string | null {
  const n = entries.filter((e) => e.appId === appId && e.state === `pending`).length
  if (n === 0) return null
  return n === 1 ? APP_OUTBOX_COPY.pendingOne : APP_OUTBOX_COPY.pending.replace(`{n}`, String(n))
}
