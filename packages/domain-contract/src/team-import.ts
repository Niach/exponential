import fixture from "../fixtures/team-import.json" with { type: "json" }

// #EXP-1713 (fixtures/team-import.json): TEAM IMPORT, the tRPC-only API
// `imports.create / chunk / commit` that Share a Local desktop as a team
// (#EDE-57) uploads through. TS reference: the server (apps/web
// lib/team-import) validates chunks, matches statuses, remaps ids and
// rewrites identity and mentions with the functions here; elocal's share
// (Rust) replays `chunking` and `validateChunk` for the uploader.

export const TEAM_IMPORT_TABLES: readonly string[] = fixture.tables
export const TEAM_IMPORT_CHUNK = fixture.chunk
export const TEAM_IMPORT_ATTACHMENTS_CAP_BYTES = fixture.attachmentsCapBytes
export const TEAM_IMPORT_ABANDONED_AFTER_HOURS = fixture.abandonedAfterHours
export const TEAM_IMPORT_SESSION_ARRIVAL = fixture.sessionArrival
export const TEAM_IMPORT_TEXT_COLUMNS: Readonly<Record<string, readonly string[]>> = fixture.textColumns
export const TEAM_IMPORT_MANIFEST_VERSION = fixture.manifest.version

/** The tables whose rows carry no `id` of their own (a composite key). */
const COMPOSITE_KEY_TABLES = new Set([`issue_labels`])

export interface ImportRow {
  table: string
  id: string
  row: Record<string, unknown>
}

export interface TeamImportManifest {
  version: number
  desktopId: string
  name: string
  identity: { userId: string; email: string }
  deviceId?: string
  counts: Record<string, number>
  attachmentBytes: number
}

function utf8Length(text: string): number {
  return new TextEncoder().encode(text).length
}

/** The refusal for a chunk, or null when it may be staged. */
export function validateChunk(rows: readonly ImportRow[]): string | null {
  if (rows.length === 0) return `A chunk needs at least one row`
  if (rows.length > TEAM_IMPORT_CHUNK.maxRows) return `A chunk holds at most ${TEAM_IMPORT_CHUNK.maxRows} rows`
  if (utf8Length(JSON.stringify(rows)) > TEAM_IMPORT_CHUNK.maxBytes) {
    return `A chunk holds at most ${Math.round(TEAM_IMPORT_CHUNK.maxBytes / (1024 * 1024))} MB of rows`
  }
  for (const entry of rows) {
    if (!TEAM_IMPORT_TABLES.includes(entry.table)) return `Unknown table: ${entry.table}`
    if (typeof entry.id !== `string` || entry.id.length === 0) return `${entry.table}: a row needs an id`
    if (entry.row === null || typeof entry.row !== `object` || Array.isArray(entry.row)) return `${entry.table} ${entry.id}: row must be an object`
    if (!COMPOSITE_KEY_TABLES.has(entry.table) && entry.row.id !== entry.id) return `${entry.table} ${entry.id}: row.id must equal id`
  }
  return null
}

/** A user column's value on the server: the local identity becomes the
 *  caller, anyone else becomes null (the caller when the column is required);
 *  an array keeps the caller once. */
export function mapUserId(value: unknown, localUserId: string, callerId: string, required: boolean): unknown {
  if (Array.isArray(value)) {
    const mapped = value.map((v) => mapUserId(v, localUserId, callerId, false)).filter((v): v is string => typeof v === `string`)
    return [...new Set(mapped)]
  }
  if (value === localUserId) return callerId
  return required ? callerId : null
}

const MENTION_SOURCE = `@([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,})`

/** `@<localEmail>` becomes `@<callerEmail>`; every other mention stays. */
export function rewriteLocalMentions<T extends string | null | undefined>(text: T, localEmail: string, callerEmail: string): T {
  if (typeof text !== `string`) return text
  const local = localEmail.toLowerCase()
  return text.replace(new RegExp(MENTION_SOURCE, `g`), (token, email: string) => (email.toLowerCase() === local ? `@${callerEmail}` : token)) as T
}

export interface StatusLike {
  id: string
  category: string
  name: string
  sortOrder: number
  builtinKey: string | null
}

export interface StatusMatch {
  /** local status id → the team's status id */
  map: Record<string, string>
  /** the local statuses nothing matched, in the order given */
  create: string[]
}

function bySort(a: StatusLike, b: StatusLike): number {
  return a.sortOrder - b.sortOrder || a.name.localeCompare(b.name) || a.id.localeCompare(b.id)
}

/** Which team status each local one becomes: builtin key, then name in the
 *  category, then ordinal among the category's unmatched rest, else created. */
export function matchStatuses(local: readonly StatusLike[], team: readonly StatusLike[]): StatusMatch {
  const map: Record<string, string> = {}
  const taken = new Set<string>()
  const unmatched: StatusLike[] = []
  for (const row of local) {
    const byKey = row.builtinKey ? team.find((t) => t.builtinKey === row.builtinKey && !taken.has(t.id)) : undefined
    const hit = byKey ?? team.find((t) => t.category === row.category && t.name.toLowerCase() === row.name.toLowerCase() && !taken.has(t.id))
    if (hit) {
      map[row.id] = hit.id
      taken.add(hit.id)
    } else {
      unmatched.push(row)
    }
  }
  const create: string[] = []
  const categories = [...new Set(unmatched.map((r) => r.category))]
  for (const category of categories) {
    const rest = team.filter((t) => t.category === category && !taken.has(t.id)).sort(bySort)
    const mine = unmatched.filter((r) => r.category === category).sort(bySort)
    mine.forEach((row, i) => {
      const hit = rest[i]
      if (hit) {
        map[row.id] = hit.id
        taken.add(hit.id)
      } else {
        create.push(row.id)
      }
    })
  }
  return { map, create: local.filter((r) => create.includes(r.id)).map((r) => r.id) }
}

/** A row with every foreign key (a string or an array of strings) passed
 *  through `lookup(table, id)`; counts the ids that changed. */
export function remapRow(
  row: Record<string, unknown>,
  foreignKeys: Readonly<Record<string, string>>,
  lookup: (table: string, id: string) => string
): { row: Record<string, unknown>; remapped: number } {
  let remapped = 0
  const out: Record<string, unknown> = { ...row }
  for (const [column, table] of Object.entries(foreignKeys)) {
    const value = row[column]
    if (typeof value === `string`) {
      const next = lookup(table, value)
      if (next !== value) remapped += 1
      out[column] = next
    } else if (Array.isArray(value)) {
      out[column] = value.map((v) => {
        if (typeof v !== `string`) return v
        const next = lookup(table, v)
        if (next !== v) remapped += 1
        return next
      })
    }
  }
  return { row: out, remapped }
}

/** The uploader's chunks: rows in `tables` order, each chunk within both
 *  caps, a row never split (one wider than the byte cap stands alone). */
export function chunkRows(rows: readonly ImportRow[], caps: { maxRows: number; maxBytes: number } = TEAM_IMPORT_CHUNK): ImportRow[][] {
  const rank = (row: ImportRow) => {
    const i = TEAM_IMPORT_TABLES.indexOf(row.table)
    return i < 0 ? TEAM_IMPORT_TABLES.length : i
  }
  const ordered = rows.map((row, i) => ({ row, i })).sort((a, b) => rank(a.row) - rank(b.row) || a.i - b.i).map((x) => x.row)
  const chunks: ImportRow[][] = []
  let current: ImportRow[] = []
  let bytes = 2
  for (const row of ordered) {
    const size = utf8Length(JSON.stringify(row))
    const next = bytes + size + (current.length > 0 ? 1 : 0)
    if (current.length > 0 && (current.length >= caps.maxRows || next > caps.maxBytes)) {
      chunks.push(current)
      current = []
      bytes = 2
    }
    bytes += size + (current.length > 0 ? 1 : 0)
    current.push(row)
  }
  if (current.length > 0) chunks.push(current)
  return chunks
}

/** How a session row arrives: ended and private. */
export function sessionArrival<T extends Record<string, unknown>>(row: T, now: string): T {
  const arrival = TEAM_IMPORT_SESSION_ARRIVAL
  return {
    ...row,
    status: arrival.status,
    shared: arrival.shared,
    busy: arrival.busy,
    needs_input: arrival.needsInput,
    blocked: null,
    ended_at: typeof row.ended_at === `string` ? row.ended_at : now,
  }
}
