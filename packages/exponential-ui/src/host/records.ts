// VAPP-122: an app's RECORDS, its own data. The manifest declares every
// kind with a JSON Schema SUBSET (`records: {<kind>: {schema}}`); a host
// validates a record against it before it is stored (the Exponential
// server does; a client may too). Pure, so the Rust core mirrors it line by
// line (`host/records.rs`); fixtures/host-router.json `records` replays it.

import type { PackageIssue } from "./package"

/** `^[a-z][a-z0-9_]{0,31}$`: a kind's name (catalog/host.json `package.records`). */
export const RECORD_KIND_PATTERN = /^[a-z][a-z0-9_]{0,31}$/
/** The keys every record row carries; a schema may not declare them. */
export const RECORD_RESERVED_KEYS: readonly string[] = [`id`, `createdAt`, `updatedAt`, `createdBy`, `authorId`, `author`]
export const RECORD_FIELD_TYPES: readonly string[] = [`string`, `number`, `integer`, `boolean`, `array`]
export const RECORD_ITEM_TYPES: readonly string[] = [`string`, `number`, `integer`, `boolean`]
export const RECORD_FORMATS: readonly string[] = [`date`, `date-time`, `time`, `attachment`]
/** eApps R7 (VAPP-221): an `attachment` value is the instance's attachment
 *  route, relative, with a lowercase uuid: `/api/attachments/<id>`. */
export const RECORD_ATTACHMENT_REF = /^\/api\/attachments\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/

export interface RecordItems {
  type: `string` | `number` | `integer` | `boolean`
  enum?: string[]
  /** String items only (eApps R7: `attachment` = a list of files). */
  format?: `date` | `date-time` | `time` | `attachment`
}

export interface RecordField {
  type: `string` | `number` | `integer` | `boolean` | `array`
  /** Strings only: the allowed values. */
  enum?: string[]
  /** Arrays only: what an item is. */
  items?: RecordItems
  /** Strings: characters; arrays: items. */
  maxLength?: number
  minimum?: number
  maximum?: number
  /** Strings only: `date` (YYYY-MM-DD), `date-time` (ISO 8601), `time`
   *  (HH:MM[:SS]), `attachment` (eApps R7: `/api/attachments/<id>`). */
  format?: `date` | `date-time` | `time` | `attachment`
  description?: string
}

export interface RecordSchema {
  type: `object`
  properties: Record<string, RecordField>
  required?: string[]
  /** Default false: a key outside `properties` is refused. */
  additionalProperties?: boolean
}

export interface RecordKind {
  schema: RecordSchema
  title?: string
  description?: string
  /** eApps R3: who may read, create, update and delete its records
   *  (`host/record-policy.ts`; absent ops keep `DEFAULT_RECORD_POLICY`). */
  policy?: Partial<import("./record-policy").RecordPolicy>
  /** eApps Q3 (VAPP-214): fields evaluated on read (name → CEL over the
   *  record's fields), never stored; queries filter, sort and group on them. */
  computed?: Record<string, string>
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const isStringList = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === `string`)

/** Every problem with a kind's schema, each with a JSON pointer under `at`. */
export function validateRecordSchema(schema: unknown, at: string): PackageIssue[] {
  const issues: PackageIssue[] = []
  if (!isObject(schema) || schema.type !== `object` || !isObject(schema.properties)) return [{ path: at, message: `a record schema is {type: "object", properties}` }]
  // Keys in sorted order: the issue order never depends on how a host's JSON
  // layer ordered the object (Swift and Kotlin dictionaries do not keep it).
  for (const name of Object.keys(schema.properties).sort()) {
    const field = (schema.properties as Record<string, unknown>)[name]
    const fp = `${at}/properties/${name}`
    if (RECORD_RESERVED_KEYS.includes(name)) issues.push({ path: fp, message: `${name} is a reserved record key` })
    if (!isObject(field) || typeof field.type !== `string` || !RECORD_FIELD_TYPES.includes(field.type)) {
      issues.push({ path: fp, message: `a field is {type: ${RECORD_FIELD_TYPES.join(`|`)}}` })
      continue
    }
    if (field.enum !== undefined && (!isStringList(field.enum) || !field.enum.length || field.type !== `string`)) issues.push({ path: `${fp}/enum`, message: `enum is a non-empty list of strings on a string field` })
    if (field.type === `array`) {
      const items = field.items
      if (!isObject(items) || typeof items.type !== `string` || !RECORD_ITEM_TYPES.includes(items.type)) issues.push({ path: `${fp}/items`, message: `items is {type: ${RECORD_ITEM_TYPES.join(`|`)}}` })
      else {
        if (items.enum !== undefined && (!isStringList(items.enum) || !items.enum.length || items.type !== `string`)) issues.push({ path: `${fp}/items/enum`, message: `enum is a non-empty list of strings on a string item` })
        if (items.format !== undefined && (typeof items.format !== `string` || !RECORD_FORMATS.includes(items.format) || items.type !== `string`)) issues.push({ path: `${fp}/items/format`, message: `format is ${RECORD_FORMATS.join(`|`)} on a string item` })
      }
    } else if (field.items !== undefined) issues.push({ path: `${fp}/items`, message: `items belongs to an array field` })
    if (field.format !== undefined && (typeof field.format !== `string` || !RECORD_FORMATS.includes(field.format) || field.type !== `string`)) issues.push({ path: `${fp}/format`, message: `format is ${RECORD_FORMATS.join(`|`)} on a string field` })
    for (const key of [`maxLength`, `minimum`, `maximum`]) if (field[key] !== undefined && typeof field[key] !== `number`) issues.push({ path: `${fp}/${key}`, message: `${key} is a number` })
  }
  if (schema.required !== undefined) {
    if (!isStringList(schema.required)) issues.push({ path: `${at}/required`, message: `required is a list of field names` })
    else schema.required.forEach((name, i) => {
      if (!Object.prototype.hasOwnProperty.call(schema.properties as object, name)) issues.push({ path: `${at}/required/${i}`, message: `${name} is not a declared field` })
    })
  }
  if (schema.additionalProperties !== undefined && typeof schema.additionalProperties !== `boolean`) issues.push({ path: `${at}/additionalProperties`, message: `additionalProperties is a boolean` })
  return issues
}

const DATE = /^\d{4}-\d{2}-\d{2}$/
const TIME = /^\d{2}:\d{2}(:\d{2})?$/
const DATE_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:\d{2})$/

function checkScalar(type: string, value: unknown, at: string, field: { enum?: string[]; format?: string; maxLength?: number; minimum?: number; maximum?: number }): PackageIssue[] {
  const issues: PackageIssue[] = []
  switch (type) {
    case `string`:
      if (typeof value !== `string`) return [{ path: at, message: `a string` }]
      if (field.enum && !field.enum.includes(value)) issues.push({ path: at, message: `one of ${field.enum.join(`|`)}` })
      if (field.maxLength !== undefined && [...value].length > field.maxLength) issues.push({ path: at, message: `at most ${field.maxLength} characters` })
      if (field.format === `date` && !DATE.test(value)) issues.push({ path: at, message: `a date (YYYY-MM-DD)` })
      if (field.format === `date-time` && !DATE_TIME.test(value)) issues.push({ path: at, message: `a date-time (ISO 8601)` })
      if (field.format === `time` && !TIME.test(value)) issues.push({ path: at, message: `a time (HH:MM)` })
      if (field.format === `attachment` && !RECORD_ATTACHMENT_REF.test(value)) issues.push({ path: at, message: `an attachment (/api/attachments/<id>)` })
      return issues
    case `number`:
    case `integer`:
      if (typeof value !== `number` || !Number.isFinite(value)) return [{ path: at, message: `a number` }]
      if (type === `integer` && !Number.isInteger(value)) return [{ path: at, message: `an integer` }]
      if (field.minimum !== undefined && value < field.minimum) issues.push({ path: at, message: `at least ${field.minimum}` })
      if (field.maximum !== undefined && value > field.maximum) issues.push({ path: at, message: `at most ${field.maximum}` })
      return issues
    case `boolean`:
      return typeof value === `boolean` ? [] : [{ path: at, message: `a boolean` }]
    default:
      return []
  }
}

/** Every problem with `data` against `schema` (a valid one): `/field`
 *  pointers. A null or missing optional field passes; a required one does
 *  not; a key the schema does not declare is refused unless
 *  `additionalProperties` is true. */
export function validateRecord(schema: RecordSchema, data: unknown): PackageIssue[] {
  if (!isObject(data)) return [{ path: ``, message: `a record is an object` }]
  const issues: PackageIssue[] = []
  const required = new Set(schema.required ?? [])
  for (const name of Object.keys(schema.properties).sort()) {
    const field = schema.properties[name]!
    const value = data[name]
    if (value === undefined || value === null) {
      if (required.has(name)) issues.push({ path: `/${name}`, message: `required` })
      continue
    }
    if (field.type === `array`) {
      if (!Array.isArray(value)) {
        issues.push({ path: `/${name}`, message: `a list` })
        continue
      }
      if (field.maxLength !== undefined && value.length > field.maxLength) issues.push({ path: `/${name}`, message: `at most ${field.maxLength} items` })
      const items = field.items ?? { type: `string` }
      value.forEach((item, i) => issues.push(...checkScalar(items.type, item, `/${name}/${i}`, { enum: items.enum, format: items.format })))
      continue
    }
    issues.push(...checkScalar(field.type, value, `/${name}`, field))
  }
  if (schema.additionalProperties !== true)
    for (const key of Object.keys(data).sort()) if (!Object.prototype.hasOwnProperty.call(schema.properties, key)) issues.push({ path: `/${key}`, message: `not in the schema` })
  return issues
}

/** eApps R7 (VAPP-221): the attachment ids `data` references through its
 *  schema's `attachment` fields (a string field or a list's string items),
 *  fields in sorted order, each id once, in the order first met. Values that
 *  are no attachment ref are skipped (`validateRecord` names them). The
 *  server checks every id belongs to the app and its team before a write. */
export function recordAttachmentIds(schema: RecordSchema, data: unknown): string[] {
  if (!isObject(data)) return []
  const ids: string[] = []
  const take = (value: unknown) => {
    const id = typeof value === `string` ? RECORD_ATTACHMENT_REF.exec(value)?.[1] : undefined
    if (id && !ids.includes(id)) ids.push(id)
  }
  for (const name of Object.keys(schema.properties).sort()) {
    const field = schema.properties[name]!
    const value = data[name]
    if (field.type === `array`) {
      if (field.items?.type === `string` && field.items.format === `attachment` && Array.isArray(value)) value.forEach(take)
    } else if (field.type === `string` && field.format === `attachment`) take(value)
  }
  return ids
}

/** eApps R7: the attachment fields of a schema (sorted): a string field or
 *  a list of strings with `format: "attachment"`. */
export function recordAttachmentFields(schema: RecordSchema): string[] {
  return Object.keys(schema.properties)
    .sort()
    .filter((name) => {
      const field = schema.properties[name]!
      return field.type === `array` ? field.items?.type === `string` && field.items.format === `attachment` : field.type === `string` && field.format === `attachment`
    })
}
