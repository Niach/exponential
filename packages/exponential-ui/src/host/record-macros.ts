// VAPP-217 (eApps U2, fixtures/record-macros.json, docs/package-format.md
// "Record macros"): the HOST half of the RecordForm and RecordTable macros.
// A macro (catalog/macros.json) sees only its node's props, never the
// package, so before a template's components are reduced the host FILLS
// every RecordForm / RecordTable node from its kind's schema: the fields
// (one per schema field, typed, with its checks), the table columns (the
// Data tab's typed widths), the row binding, the draft path and the
// actions (`records_upsert`, `records_delete`, the drawer and confirm
// writes). The macro table then expands the filled node like any other.
// Pure; the Rust core mirrors it (host::record_macros) and every
// renderer gets it through `templateMessages`.

import type { RecordField, RecordKind } from "./records"

type Json = Record<string, unknown>
type Binding = { "@path": string }

/** One RecordForm field: `widget` picks the control (macros.json). */
export interface RecordFormField {
  name: string
  label: string
  widget: `text` | `textarea` | `number` | `switch` | `select` | `date` | `time` | `datetime` | `chips`
  required: boolean
  /** Spans both FieldGroup columns on wide frames. */
  wide: boolean
  value: Binding
  /** `datetime`: the pickers' draft parts (`<path>Parts/<name>/date|time`). */
  date?: Binding
  time?: Binding
  placeholder?: string
  options?: { label: string; value: string }[] | Binding
  searchable?: boolean
  multiple?: boolean
  min?: number
  max?: number
  step?: number
  precision?: number
  checks?: { condition: unknown; message: string }[]
}

/** The Data tab's typed column widths (domain-contract app-records.json
 *  `dataTab.columns`). */
export const RECORD_COLUMN_WIDTHS: Readonly<Record<string, number>> = { date: 104, time: 64, "date-time": 152, number: 64, integer: 64, boolean: 56 }
export const RECORD_UPDATED_WIDTH = 136
/** A date-time cell's formatted `Nov 12, 2026, 3:30 AM` (the Data tab's raw editor keeps 152). */
export const RECORD_DATE_TIME_WIDTH = 188
export const RECORD_MENU_WIDTH = 80
/** FEED-202: a `currency` column without the author's width. */
export const RECORD_CURRENCY_WIDTH = 96
/** A string longer than this (its `maxLength`) is a Textarea. */
export const RECORD_LONG_TEXT = 200
export const RECORD_FORM_ROOT = `/recordForm`
export const RECORD_TABLE_ROOT = `/recordTable`
/** The team's members and issues the macros' Selects read (one binding per
 *  surface, outside the package's `sources`: the macro's own). */
export const RECORD_MEMBERS_PATH = `/recordMembers`
export const RECORD_ISSUES_PATH = `/recordIssues`
const MEMBERS_SOURCE = `exp:members`
const ISSUES_SOURCE = `exp:issues`

/** A record macro's own source at its own path passes any `sources`. */
export function isRecordMacroBinding(path: unknown, source: unknown): boolean {
  return (path === RECORD_MEMBERS_PATH && source === MEMBERS_SOURCE) || (path === RECORD_ISSUES_PATH && source === ISSUES_SOURCE)
}

/** A refused record write the form shows inline (invalid input) instead of a
 *  failed-action toast: `records_upsert` rejects with it, the runtime writes
 *  the message at the call's `form.error`. */
export class RecordRefusal extends Error {
  override name = `RecordRefusal`
}

/** A record form's targets in `records_upsert`'s args. */
export function recordFormTargets(args: Record<string, unknown>): { error?: string; close?: string } {
  const form = args.form
  if (!form || typeof form !== `object`) return {}
  const { error, close } = form as Json
  return { ...(typeof error === `string` ? { error } : {}), ...(typeof close === `string` ? { close } : {}) }
}

const isObject = (v: unknown): v is Json => !!v && typeof v === `object` && !Array.isArray(v)
const hasIssueRef = (fields: RecordFormField[]) => fields.some((f) => isObject(f.options) && f.options[`@path`] === `${RECORD_ISSUES_PATH}/rows`)

/** `closeDate` / `close_date` → `Close date`. */
export function fieldLabel(name: string): string {
  const words = name
    .replace(/_/g, ` `)
    .replace(/([a-z0-9])([A-Z])/g, `$1 $2`)
    .trim()
    .toLowerCase()
  return words ? words[0]!.toUpperCase() + words.slice(1) : name
}

/** The Data tab's column order: required (in order), then the rest by name. */
export function recordColumns(kind: RecordKind): string[] {
  const names = Object.keys(kind.schema.properties)
  const required = (kind.schema.required ?? []).filter((n, i, all) => names.includes(n) && all.indexOf(n) === i)
  return [...required, ...names.filter((n) => !required.includes(n)).sort()]
}

const isFile = (f: RecordField) => f.format === `attachment` || (f.type === `array` && f.items?.format === `attachment`)

/** The form fields of a kind, values under `path`. Files are left out (a
 *  FileUpload needs the host's upload flow). */
export function recordFormFields(kind: RecordKind, path: string): RecordFormField[] {
  const required = kind.schema.required ?? []
  const refs = kind.refs ?? {}
  return recordColumns(kind).flatMap((name): RecordFormField[] => {
    const f = kind.schema.properties[name]!
    if (isFile(f)) return []
    const value = { "@path": `${path}/${name}` }
    const out: RecordFormField = { name, label: fieldLabel(name), widget: `text`, required: required.includes(name), wide: false, value }
    if (f.description) out.placeholder = f.description
    const ref = refs[name]
    if (ref === `member`) Object.assign(out, { widget: `select`, options: { "@path": `${RECORD_MEMBERS_PATH}/rows` }, searchable: true })
    else if (ref === `issue`) Object.assign(out, { widget: `select`, options: { "@path": `${RECORD_ISSUES_PATH}/rows` }, searchable: true })
    else if (f.type === `boolean`) out.widget = `switch`
    else if (f.type === `number` || f.type === `integer`) {
      out.widget = `number`
      if (f.minimum !== undefined) out.min = f.minimum
      if (f.maximum !== undefined) out.max = f.maximum
      if (f.type === `integer`) Object.assign(out, { step: 1, precision: 0 })
    } else if (f.type === `array`) {
      if (f.items?.enum) Object.assign(out, { widget: `select`, multiple: true, options: f.items.enum.map((v) => ({ label: v, value: v })) })
      else out.widget = `chips`
      if (f.maxLength !== undefined) out.max = f.maxLength
      out.wide = true
    } else if (f.enum) Object.assign(out, { widget: `select`, options: f.enum.map((v) => ({ label: v, value: v })) })
    else if (f.format === `date`) out.widget = `date`
    else if (f.format === `time`) out.widget = `time`
    else if (f.format === `date-time`) {
      const parts = `${path}Parts/${name}`
      Object.assign(out, { widget: `datetime`, wide: true, date: { "@path": `${parts}/date` }, time: { "@path": `${parts}/time` } })
    }
    else if (f.maxLength === undefined || f.maxLength > RECORD_LONG_TEXT) Object.assign(out, { widget: `textarea`, wide: true })
    const checks: { condition: unknown; message: string }[] = []
    if (out.required && out.widget !== `switch`) checks.push({ condition: { "@call": `required`, args: { value } }, message: `$string.required` })
    if (f.type === `string` && f.maxLength !== undefined && !f.enum && out.widget !== `select`)
      checks.push({ condition: { "@call": `length`, args: { value, max: f.maxLength } }, message: `$string.invalidValue` })
    if ((f.type === `number` || f.type === `integer`) && (f.minimum !== undefined || f.maximum !== undefined))
      checks.push({ condition: { "@call": `numeric`, args: { value, ...(f.minimum !== undefined ? { min: f.minimum } : {}), ...(f.maximum !== undefined ? { max: f.maximum } : {}) } }, message: `$string.invalidValue` })
    if (checks.length) out.checks = checks
    return [out]
  })
}

/** A fixed column never truncates its own header: the label at the header's
 *  8 px a letter, its 8 + 8 padding and the 16 + 4 sort arrow. */
const recordHeaderWidth = (label: string) => label.length * 8 + 36

/** FEED-202: what an author's column object may set over the field's
 *  defaults (Table's column shape; never a slot). */
const COLUMN_KEYS = [`label`, `type`, `currency`, `decimals`, `align`, `width`, `format`, `truncate`, `sortable`] as const

/** A RecordTable's `columns` entry: a field name or a column object
 *  (`{key, …}`), keyed by its field. */
export type RecordColumnEntry = string | Json
const entryKey = (e: unknown): string | undefined => (typeof e === `string` ? e : isObject(e) && typeof e.key === `string` ? e.key : undefined)

/** The shown fields of `columns` (default every one), files left out. */
function shownEntries(kind: RecordKind, columns: readonly unknown[] | undefined): { key: string; entry?: Json }[] {
  const entries = columns ?? recordColumns(kind)
  return entries.flatMap((e) => {
    const key = entryKey(e)
    if (key === undefined || !kind.schema.properties[key] || isFile(kind.schema.properties[key]!)) return []
    return [isObject(e) ? { key, entry: e } : { key }]
  })
}

/** The table columns: the chosen fields (default every one, files left
 *  out), typed and sized like the Data tab, an author's column object
 *  over them, then `updatedAt`, then the row menu (when editable). */
export function recordTableColumns(kind: RecordKind, columns: readonly unknown[] | undefined, editable: boolean): Json[] {
  const out: Json[] = shownEntries(kind, columns).map(({ key, entry }) => {
    const f = kind.schema.properties[key]!
    const col: Json = { key, label: fieldLabel(key), sortable: true }
    if (f.type === `number` || f.type === `integer`) Object.assign(col, { type: `number`, align: `end` })
    else if (f.type === `boolean`) col.type = `boolean`
    else if (f.format === `date`) col.type = `date`
    else if (f.format === `date-time`) col.type = `dateTime`
    for (const k of COLUMN_KEYS) if (entry?.[k] != null && !(k === `type` && entry[k] === `slot`)) col[k] = entry[k]
    if (entry?.width == null) {
      const width = col.type === `currency` ? RECORD_CURRENCY_WIDTH : f.format === `date-time` ? RECORD_DATE_TIME_WIDTH : f.type === `string` ? (f.format ? RECORD_COLUMN_WIDTHS[f.format] : undefined) : RECORD_COLUMN_WIDTHS[f.type]
      if (width !== undefined) col.width = Math.max(width, recordHeaderWidth(String(col.label)))
      else if (col.truncate === undefined) col.truncate = true
    }
    return col
  })
  out.push({ key: `updatedAt`, label: `Updated`, type: `relativeTime`, width: RECORD_UPDATED_WIDTH, sortable: true })
  if (editable) out.push({ key: `menu`, label: ``, type: `slot`, slot: `menu`, width: RECORD_MENU_WIDTH })
  return out
}

export interface RecordMacroOptions {
  /** The viewer may only read (the app's access level): no Add, no edit. */
  readOnly?: boolean
}

export interface FilledTemplate {
  components: Json[]
  /** The template's bindings plus one per RecordTable (its rows). */
  bindings: { path: string; source: string }[]
}

const set = (path: string, value: unknown) => ({ functionCall: { "@call": `set`, args: { path, value } } })
const setOn = (node: Json, event: string, action: unknown) => {
  const on = (node.on = isObject(node.on) ? { ...node.on } : {}) as Json
  if (on[event] === undefined) on[event] = action
}

/** A date-time field's value: its picked day + time in the team zone; no
 *  day picked = the draft's value as it is. */
const dateTimeValue = (f: RecordFormField) => ({ "@call": `cond`, args: { if: f.date, then: { "@call": `dateTime`, args: { date: f.date, time: f.time } }, else: f.value } })

const MY_ID = { "@path": `${RECORD_MEMBERS_PATH}/me` }

/** The Author Select's options: the members for a team owner, else none (so
 *  no picker). */
const AUTHOR_OPTIONS = {
  "@call": `cond`,
  args: {
    if: { "@call": `lt`, args: { a: 0, b: { "@call": `len`, args: { value: { "@call": `filter`, args: { items: { "@path": `${RECORD_MEMBERS_PATH}/rows` }, where: { id: MY_ID, role: `owner` } } } } } } },
    then: { "@path": `${RECORD_MEMBERS_PATH}/rows` },
    else: null,
  },
}

/** The form's `author`: an owner's pick, only when it moved off the record's
 *  author (domain-contract app-records.json `formAuthor`), else nothing. */
const authorArg = (path: string) => ({
  "@call": `cond`,
  args: {
    if: AUTHOR_OPTIONS,
    then: { "@call": `cond`, args: { if: { "@call": `or`, args: { values: [{ "@call": `not`, args: { value: { "@path": `${path}Author` } } }, { "@call": `eq`, args: { a: { "@path": `${path}Author` }, b: { "@path": `${path}AuthorBase` } } }] } }, then: null, else: { "@path": `${path}Author` } } },
    else: null,
  },
})

/** Opens the editor on a row: the draft = the row, a date-time's parts = its
 *  day and time in the team zone, the Author = the row's. */
function editAction(base: string, fields: RecordFormField[]): Json {
  const parts: Json = {}
  for (const f of fields) {
    if (f.widget !== `datetime`) continue
    const value = { "@path": f.name }
    parts[f.name] = { date: { "@call": `formatDate`, args: { value, format: `yyyy-MM-dd` } }, time: { "@call": `formatDate`, args: { value, format: `HH:mm` } } }
  }
  const author = { draftAuthor: { "@call": `coalesce`, args: { values: [{ "@path": `authorId` }, MY_ID] } }, draftAuthorBase: { "@path": `authorId` } }
  return set(`${base}/editor`, { open: true, draft: { "@path": `` }, ...author, ...(Object.keys(parts).length ? { draftParts: parts } : {}) })
}

/** The form's write: `records_upsert {kind, id?, data, author, form}`, data =
 *  one binding per field; `form` tells the host where a refusal shows
 *  (`error`) and which flag a landed write clears (`close`). VAPP-206: an
 *  optional field left "" (an unpicked Select, an emptied Input) writes null:
 *  absent on a create, cleared on a patch. */
function upsertAction(kind: string, fields: RecordFormField[], recordId: unknown, path: string, close?: string): Json {
  const data: Json = {}
  for (const f of fields) {
    const value = f.widget === `datetime` ? dateTimeValue(f) : f.value
    data[f.name] = f.required ? value : { "@call": `expr`, args: { expr: `v == '' ? null : v`, vars: { v: value }, default: null } }
  }
  const args: Json = { kind }
  if (recordId !== undefined) args.id = recordId
  args.data = data
  args.author = authorArg(path)
  args.form = { error: `${path}Error`, ...(close ? { close } : {}) }
  return { functionCall: { "@call": `records_upsert`, args } }
}

/** Every RecordForm / RecordTable of a template's flat component list,
 *  filled from `records`; other components untouched. Pure. A kind the
 *  package does not declare fills `missing: true` (the macro's unknown
 *  state). A RecordTable also gets a phone row component `<id>__row`
 *  and, when editable, its Delete `<id>__rowDelete` (appended) and the rows binding `/recordTable/<id>/rows`. */
export function fillRecordMacros(components: readonly Json[], bindings: readonly { path: string; source: string }[] = [], records: Record<string, RecordKind> | undefined, options: RecordMacroOptions = {}): FilledTemplate {
  const out: Json[] = []
  const extra: Json[] = []
  const binds = [...bindings]
  const bindOnce = (source: string, path: string) => {
    if (!binds.some((b) => b.path === path)) binds.push({ path, source })
  }
  for (const c of components) {
    if (!isObject(c) || (c.component !== `RecordForm` && c.component !== `RecordTable`)) {
      out.push(c)
      continue
    }
    const node: Json = { ...c }
    const id = String(node.id)
    const kindName = typeof node.kind === `string` ? node.kind : ``
    const kind = records && Object.prototype.hasOwnProperty.call(records, kindName) ? records[kindName] : undefined
    if (!kind) {
      node.missing = true
      out.push(node)
      continue
    }
    if (node.component === `RecordForm`) {
      const path = typeof node.path === `string` ? node.path : `${RECORD_FORM_ROOT}/${id}`
      const fields = recordFormFields(kind, path)
      node.path = path
      node.fields = fields
      if (options.readOnly) node.readOnly = true
      if (node.readOnly !== true) {
        node.error = { "@path": `${path}Error` }
        node.author = { "@path": `${path}Author` }
        node.authorOptions = AUTHOR_OPTIONS
        bindOnce(MEMBERS_SOURCE, RECORD_MEMBERS_PATH)
      }
      if (hasIssueRef(fields)) bindOnce(ISSUES_SOURCE, RECORD_ISSUES_PATH)
      setOn(node, `submit`, upsertAction(kindName, fields, node.recordId, path))
      out.push(node)
      continue
    }
    const base = `${RECORD_TABLE_ROOT}/${id}`
    const editable = node.editable !== false && !options.readOnly
    const columns = Array.isArray(node.columns) ? (node.columns as unknown[]) : undefined
    const draft = `${base}/editor/draft`
    const fields = recordFormFields(kind, draft)
    node.title = kind.title ?? fieldLabel(kindName)
    node.cols = recordTableColumns(kind, columns, editable)
    const shown = shownEntries(kind, columns).map((e) => e.key)
    // FEED-202: the search (over the shown text columns) and the enum
    // filter narrow the rows through the core `filter`; the count, the
    // totals and the phone rows follow.
    const searchFields = node.search === true ? shown.filter((n) => kind.schema.properties[n]!.type === `string` && !kind.schema.properties[n]!.format) : []
    const filterField = typeof node.filter === `string` ? kind.schema.properties[node.filter] : undefined
    const filterBy = filterField?.type === `string` && filterField.enum ? (node.filter as string) : undefined
    const narrow: Json = {}
    if (searchFields.length) Object.assign(narrow, { query: { "@path": `${base}/search` }, fields: searchFields })
    if (filterBy) narrow.where = { [filterBy]: { "@path": `${base}/filter` } }
    const narrows = Object.keys(narrow).length > 0
    const allRows = { "@path": `${base}/rows/rows` }
    if (node.search !== undefined) node.search = searchFields.length > 0
    if (searchFields.length) node.searchValue = { "@path": `${base}/search` }
    if (filterBy) {
      node.filterLabel = fieldLabel(filterBy)
      node.filterOptions = [{ label: `$string.all`, value: `` }, ...filterField!.enum!.map((v) => ({ label: v, value: v }))]
      node.filterValue = { "@path": `${base}/filter` }
    } else delete node.filter
    node.rows = narrows ? { "@call": `filter`, args: { items: allRows, ...narrow } } : allRows
    const total = narrows ? { "@call": `len`, args: { value: node.rows } } : { "@call": `coalesce`, args: { values: [{ "@path": `${base}/rows/count` }, 0] } }
    node.count = { "@call": `pluralize`, args: { value: total, one: `$string.recordCountOne`, other: `$string.recordCount` } }
    // Presence, not `not`: A2UI 1.0 coerces an object to false (#EXP-1757).
    node.loading = { "@call": `cond`, args: { if: { "@path": `${base}/rows` }, then: false, else: true } }
    if (options.readOnly) node.add = false
    node.editable = editable
    node.children = { componentId: `${id}__row`, path: `${base}/rows/rows`, key: `id` }
    if (editable) {
      node.fields = fields
      node.path = draft
      node.error = { "@path": `${draft}Error` }
      node.author = { "@path": `${draft}Author` }
      node.authorOptions = AUTHOR_OPTIONS
      bindOnce(MEMBERS_SOURCE, RECORD_MEMBERS_PATH)
      if (hasIssueRef(fields)) bindOnce(ISSUES_SOURCE, RECORD_ISSUES_PATH)
      node.editorOpen = { "@path": `${base}/editor/open` }
      node.editorId = { "@path": `${draft}/id` }
      node.deleteOpen = { "@path": `${base}/delete/open` }
      setOn(node, `add`, set(`${base}/editor`, { open: true, draft: {}, draftAuthor: MY_ID, draftAuthorBase: MY_ID }))
      setOn(node, `edit`, editAction(base, fields))
      setOn(node, `delete`, set(`${base}/delete`, { open: true, id: { "@path": `id` } }))
      setOn(node, `submit`, upsertAction(kindName, fields, { "@path": `${draft}/id` }, draft, `${base}/editor/open`))
      setOn(node, `cancel`, set(`${base}/editor/open`, false))
      setOn(node, `confirm`, { functionCall: { "@call": `records_delete`, args: { id: { "@path": `${base}/delete/id` } } } })
      setOn(node, `cancelDelete`, set(`${base}/delete/open`, false))
    } else node.add = false
    out.push(node)
    // The phone list's row: the first column as the title, the second as
    // the subtitle, the relative update time as the meta; hidden when the
    // search or the filter leaves it out.
    const row: Json = { id: `${id}__row`, component: `Row`, title: { "@call": `coalesce`, args: { values: [{ "@call": `text`, args: { value: { "@path": shown[0] ?? `id` } } }, `—`] } } }
    if (shown[1]) row.subtitle = { "@call": `coalesce`, args: { values: [{ "@call": `text`, args: { value: { "@path": shown[1] } } }, ``] } }
    row.meta = { "@call": `formatRelativeTime`, args: { value: { "@path": `updatedAt` } } }
    if (narrows) row.visible = { "@call": `lt`, args: { a: 0, b: { "@call": `len`, args: { value: { "@call": `filter`, args: { items: [{ "@path": `` }], ...narrow } } } } } }
    if (editable) {
      row.on = { press: editAction(base, fields) }
      // The row's own Delete (the table's Delete cell is hidden on phones).
      row.slots = { trailing: `${id}__rowDelete` }
    }
    extra.push(row)
    if (editable) extra.push({ id: `${id}__rowDelete`, component: `Button`, icon: `ui-delete`, variant: `ghost`, size: `icon`, accessibility: { label: `$string.delete` }, on: { press: (node.on as Json).delete } })
    const query = typeof node.query === `string` && node.query.trim() ? node.query.trim().replace(/^\?/, ``) : `sort=updatedAt&order=desc`
    binds.push({ path: `${base}/rows`, source: `records:${kindName}?${query}` })
  }
  return { components: [...out, ...extra], bindings: binds }
}

/** True when a template uses a record macro (the fill pass has work). */
export function usesRecordMacros(components: readonly unknown[]): boolean {
  return components.some((c) => isObject(c) && (c.component === `RecordForm` || c.component === `RecordTable`))
}
