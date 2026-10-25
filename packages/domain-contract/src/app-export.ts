// VAPP-121: an app as ONE JSON file, the pure reference every client mirrors
// (fixtures/app-export.json is the spec; desktop `domain::app_export`, iOS
// `AppExport.swift`, Android `AppExport.kt` replay `exportCases`). The
// server runs `parseAppImport` before the package and record validators.

import fixture from "../fixtures/app-export.json" with { type: "json" }
import { APP_DESCRIPTION_MAX, APP_NAME_MAX, APP_RECORD_KIND_PATTERN } from "./apps"

export const APP_EXPORT_FORMAT: string = fixture.format
export const APP_EXPORT_SUFFIX: string = fixture.fileSuffix
export const APP_EXPORT_MIME: string = fixture.mime
export const APP_EXPORT_LIMITS = fixture.limits
export const APP_EXPORT_COPY = fixture.copy

export interface AppExportApp {
  name: string
  icon: string | null
  color: string | null
  description: string
}

export interface AppExportRecord {
  kind: string
  data: Record<string, unknown>
}

export interface AppExportFile {
  format: string
  app: AppExportApp
  package: Record<string, unknown>
  records?: AppExportRecord[]
}

/** `working-hours` → `working-hours.app.json`. */
export function appExportFileName(slug: string): string {
  return `${slug}${APP_EXPORT_SUFFIX}`
}

/** The file for an app: its identity, the head package and, when switched
 *  on, every record as `{kind, data}` oldest first (createdAt, then id).
 *  Never ids, authors, times or grants. */
export function buildAppExport(input: {
  app: { name: string; icon?: string | null; color?: string | null; description?: string | null }
  package: Record<string, unknown>
  records?: readonly { id: string; kind: string; data: Record<string, unknown>; createdAt: string | Date }[]
  includeRecords: boolean
}): AppExportFile {
  const file: AppExportFile = {
    format: APP_EXPORT_FORMAT,
    app: {
      name: input.app.name,
      icon: input.app.icon ?? null,
      color: input.app.color ?? null,
      description: input.app.description ?? ``,
    },
    package: input.package,
  }
  const records = input.includeRecords ? [...(input.records ?? [])] : []
  if (records.length > 0) {
    const at = (r: { createdAt: string | Date }) => new Date(r.createdAt).getTime()
    records.sort((a, b) => at(a) - at(b) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    file.records = records.map((r) => ({ kind: r.kind, data: r.data }))
  }
  return file
}

/** The file's text: two-space JSON and a trailing newline. */
export function appExportText(file: AppExportFile): string {
  return `${JSON.stringify(file, null, 2)}\n`
}

export interface ImportIssue {
  path: string
  message: string
}

export interface ParsedAppImport {
  issues: ImportIssue[]
  /** Null while there are issues. */
  file: { app: AppExportApp; package: Record<string, unknown>; records: AppExportRecord[] } | null
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** Iteratively: does any value sit deeper than `max` levels? (The package and
 *  record validators and `JSON.stringify` recurse; a hostile file nested
 *  thousands deep would overflow the stack in them.) */
function nestedDeeperThan(root: unknown, max: number): boolean {
  const stack: [unknown, number][] = [[root, 1]]
  while (stack.length > 0) {
    const [value, depth] = stack.pop()!
    if (typeof value !== `object` || value === null) continue
    if (depth > max) return true
    for (const child of Object.values(value)) stack.push([child, depth + 1])
  }
  return false
}

/** The structural half of an import: JSON, the format, the app identity,
 *  the package's presence and the records' shape. A bare package (an object
 *  with `templates` and no `format`) is accepted and named by its `name`. */
export function parseAppImport(text: string): ParsedAppImport {
  const { issues, file } = parseAppImportShape(text)
  return { issues, file }
}

/** `parseAppImport` plus whether the text was a BARE package (the server
 *  roots a package's issue paths at `/` then, not `/package`): one JSON
 *  parse for the whole import check (VAPP-155). */
export function parseAppImportShape(text: string): ParsedAppImport & { bare: boolean } {
  const fail = (issues: ImportIssue[]) => ({ issues, file: null, bare: false })
  if (!text.trim()) return fail([{ path: `/`, message: `paste an app file or a package` }])
  if (new TextEncoder().encode(text).byteLength > APP_EXPORT_LIMITS.importMaxBytes) {
    return fail([{ path: `/`, message: `is over ${APP_EXPORT_LIMITS.importMaxBytes} bytes` }])
  }
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    return fail([{ path: `/`, message: `is not JSON` }])
  }
  if (!isObject(value)) return fail([{ path: `/`, message: `must be an app file {format, app, package} or a package` }])
  if (nestedDeeperThan(value, APP_EXPORT_LIMITS.maxDepth)) {
    return fail([{ path: `/`, message: `is nested deeper than ${APP_EXPORT_LIMITS.maxDepth} levels` }])
  }
  if (value.format === undefined && `templates` in value) {
    const name = typeof value.name === `string` && value.name.trim() ? value.name.trim().slice(0, APP_NAME_MAX) : `Imported app`
    return { issues: [], file: { app: { name, icon: null, color: null, description: `` }, package: value, records: [] }, bare: true }
  }
  if (value.format !== APP_EXPORT_FORMAT) return fail([{ path: `/format`, message: `must be "${APP_EXPORT_FORMAT}"` }])
  const issues: ImportIssue[] = []
  const app = isObject(value.app) ? value.app : null
  if (!app) issues.push({ path: `/app`, message: `must be {name, icon, color, description}` })
  const name = app && typeof app.name === `string` ? app.name.trim() : ``
  if (app && !name) issues.push({ path: `/app/name`, message: `must be a name` })
  const optionalString = (key: `icon` | `color` | `description`) => {
    const v = app?.[key]
    if (v === undefined || v === null || typeof v === `string`) return (v as string | null | undefined) ?? null
    issues.push({ path: `/app/${key}`, message: `must be a string or null` })
    return null
  }
  const icon = optionalString(`icon`)
  const color = optionalString(`color`)
  const description = optionalString(`description`) ?? ``
  if (!isObject(value.package)) issues.push({ path: `/package`, message: value.package === undefined ? `is required` : `must be an object` })
  const records: AppExportRecord[] = []
  if (value.records !== undefined) {
    if (!Array.isArray(value.records)) issues.push({ path: `/records`, message: `must be a list of {kind, data}` })
    else if (value.records.length > APP_EXPORT_LIMITS.recordsMax) issues.push({ path: `/records`, message: `has ${value.records.length} records; the cap is ${APP_EXPORT_LIMITS.recordsMax}` })
    else
      value.records.forEach((r, i) => {
        if (!isObject(r)) return void issues.push({ path: `/records/${i}`, message: `must be {kind, data}` })
        const kindOk = typeof r.kind === `string` && APP_RECORD_KIND_PATTERN.test(r.kind)
        if (!kindOk) issues.push({ path: `/records/${i}/kind`, message: `must be a kind` })
        if (!isObject(r.data)) issues.push({ path: `/records/${i}/data`, message: `must be an object` })
        if (kindOk && isObject(r.data)) records.push({ kind: r.kind as string, data: r.data })
      })
  }
  if (issues.length > 0) return fail(issues)
  return {
    issues: [],
    file: {
      app: { name: name.slice(0, APP_NAME_MAX), icon, color, description: description.slice(0, APP_DESCRIPTION_MAX) },
      package: value.package as Record<string, unknown>,
      records,
    },
    bare: false,
  }
}
