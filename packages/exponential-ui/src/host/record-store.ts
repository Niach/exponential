// VAPP-161: the format's NEUTRAL records (docs/package-format.md "Records").
// A surface reads a kind through the binding `records:<kind>?query` and
// writes through the host functions `records.upsert` / `records.delete`;
// the HOST provides the store (Exponential: its synced store, spezl: a p2p
// store over eNet). `RecordStore` is what a host implements;
// `MemoryRecordStore` + `recordsHost` are the reference, `recordsValue` the
// derivation fixtures/host-package.json `recordsValue` locks.

import type { AppPackage } from "./package"
import { validateRecord } from "./records"
import type { ParsedSource, SourceResolver } from "./sources"

/** The binding scheme: `records:<kind>[?query]`. */
export const RECORDS_SCHEME = `records`
/** The write functions (no consent card: the app's own rows). */
export const RECORDS_FUNCTIONS: readonly string[] = [`records.upsert`, `records.delete`]
/** The neutral query's defaults (catalog/host.json `package.records.query`). */
export const RECORDS_QUERY = { sortDefault: `createdAt`, orderDefault: `desc`, limitDefault: 100, limitMax: 500 } as const
/** Query keys a host may give a meaning (Exponential does); never a field filter. */
export const RECORDS_RESERVED_PARAMS: readonly string[] = [`app`, `by`, `kind`, `limit`, `mine`, `month`, `order`, `sort`, `sum`, `user`]

export interface RecordRow {
  id: string
  kind: string
  data: Record<string, unknown>
  createdBy: string | null
  createdAt: string
  updatedAt: string
}

export type RecordsFilterOp = `eq` | `gt` | `gte` | `lt` | `lte`

export interface RecordsQuery {
  kind: string
  mine: boolean
  sort: string
  order: `asc` | `desc`
  limit: number
  /** By field, then operator. */
  filters: { field: string; op: RecordsFilterOp; value: string }[]
}

export interface RecordsValue {
  rows: Record<string, unknown>[]
  count: number
}

const OPS: readonly RecordsFilterOp[] = [`gt`, `gte`, `lt`, `lte`]

/** `records:<kind>?…` (as `parseSource` gives it) → the query; null = not
 *  a records source. */
export function parseRecordsSource(source: Pick<ParsedSource, `scheme` | `name` | `params`>): RecordsQuery | null {
  if (source.scheme !== RECORDS_SCHEME) return null
  const p = source.params
  const limitRaw = Number(p.limit)
  const filters: RecordsQuery[`filters`] = []
  for (const key of Object.keys(p).sort()) {
    if (RECORDS_RESERVED_PARAMS.includes(key)) continue
    const dot = key.lastIndexOf(`.`)
    const suffix = dot >= 0 ? key.slice(dot + 1) : ``
    if (dot > 0 && (OPS as readonly string[]).includes(suffix)) filters.push({ field: key.slice(0, dot), op: suffix as RecordsFilterOp, value: p[key]! })
    else filters.push({ field: key, op: `eq`, value: p[key]! })
  }
  filters.sort((a, b) => (a.field < b.field ? -1 : a.field > b.field ? 1 : a.op < b.op ? -1 : a.op > b.op ? 1 : 0))
  return {
    kind: source.name,
    mine: p.mine === `1` || p.mine === `true`,
    sort: p.sort?.trim() || RECORDS_QUERY.sortDefault,
    order: p.order === `asc` ? `asc` : p.order === `desc` ? `desc` : RECORDS_QUERY.orderDefault,
    limit: Number.isInteger(limitRaw) && limitRaw > 0 ? Math.min(limitRaw, RECORDS_QUERY.limitMax) : RECORDS_QUERY.limitDefault,
    filters,
  }
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

/** Numbers when both parse, else text (code points, no locale). */
function compare(a: unknown, b: unknown): number {
  const na = num(a)
  const nb = num(b)
  if (na !== null && nb !== null) return na < nb ? -1 : na > nb ? 1 : 0
  const ta = text(a)
  const tb = text(b)
  return ta < tb ? -1 : ta > tb ? 1 : 0
}

/** The value a `records:` binding emits over `rows` (any kind): the kind's
 *  matching rows sorted (ties by id) then cut at the limit, `count` before
 *  the limit. `names` = user id → the display name (`author`). */
export function recordsValue(rows: readonly RecordRow[], query: RecordsQuery, ctx: { me: string | null; names?: Record<string, string> }): RecordsValue {
  const field = (row: RecordRow, name: string): unknown =>
    name === `createdAt` ? row.createdAt : name === `updatedAt` ? row.updatedAt : name === `id` ? row.id : name === `createdBy` ? row.createdBy : row.data[name]
  const matched = rows
    .filter((row) => {
      if (row.kind !== query.kind) return false
      if (query.mine && (ctx.me === null || row.createdBy !== ctx.me)) return false
      return query.filters.every((f) => {
        const v = field(row, f.field)
        if (f.op === `eq`) return text(v) === f.value
        const c = compare(v, f.value)
        return f.op === `gt` ? c > 0 : f.op === `gte` ? c >= 0 : f.op === `lt` ? c < 0 : c <= 0
      })
    })
    .sort((a, b) => {
      const c = compare(field(a, query.sort), field(b, query.sort))
      if (c !== 0) return query.order === `asc` ? c : -c
      return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
    })
  return {
    rows: matched.slice(0, query.limit).map((row) => ({
      ...row.data,
      id: row.id,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
      createdBy: row.createdBy,
      author: (row.createdBy && ctx.names?.[row.createdBy]) || ``,
    })),
    count: matched.length,
  }
}

/** What a host provides: one package's records, any transport. */
export interface RecordStore {
  /** Every row of the package (any kind) now, then on every change; returns the cancel. */
  subscribe(emit: (rows: readonly RecordRow[]) => void): () => void
  /** Create (no id) or shallow-patch `data` (id); resolves the row's id. The
   *  caller has validated the result against the kind's schema. */
  upsert(input: { kind: string; data: Record<string, unknown>; id?: string }): Promise<{ id: string }>
  delete(id: string): Promise<void>
  /** One row by id (the patch base). */
  get(id: string): RecordRow | undefined
}

/** The reference store: in memory, `now` + `newId` injectable. */
export class MemoryRecordStore implements RecordStore {
  private rows: RecordRow[] = []
  private listeners = new Set<(rows: readonly RecordRow[]) => void>()
  private seq = 0

  constructor(
    private readonly options: { me?: string | null; now?: () => string; newId?: () => string; rows?: readonly RecordRow[] } = {}
  ) {
    this.rows = [...(options.rows ?? [])]
  }

  subscribe(emit: (rows: readonly RecordRow[]) => void): () => void {
    this.listeners.add(emit)
    emit(this.rows)
    return () => void this.listeners.delete(emit)
  }

  get(id: string): RecordRow | undefined {
    return this.rows.find((r) => r.id === id)
  }

  async upsert(input: { kind: string; data: Record<string, unknown>; id?: string }): Promise<{ id: string }> {
    const now = this.options.now?.() ?? new Date().toISOString()
    if (input.id !== undefined) {
      const row = this.get(input.id)
      if (!row) throw new Error(`record ${input.id} not found`)
      this.rows = this.rows.map((r) => (r.id === input.id ? { ...r, data: { ...r.data, ...input.data }, updatedAt: now } : r))
      this.changed()
      return { id: input.id }
    }
    const id = this.options.newId?.() ?? `r${++this.seq}`
    this.rows = [...this.rows, { id, kind: input.kind, data: input.data, createdBy: this.options.me ?? null, createdAt: now, updatedAt: now }]
    this.changed()
    return { id }
  }

  async delete(id: string): Promise<void> {
    if (!this.get(id)) throw new Error(`record ${id} not found`)
    this.rows = this.rows.filter((r) => r.id !== id)
    this.changed()
  }

  private changed(): void {
    for (const emit of this.listeners) emit(this.rows)
  }
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

/** A package's records on a host: the `records` source resolver and the
 *  two write functions (pass them to `ExponentialHost` as `sources` /
 *  `functions`). A write is validated against the kind's schema (a patch:
 *  the merged data) before the store sees it; a refusal throws. */
export function recordsHost(
  pkg: Pick<AppPackage, `records`>,
  store: RecordStore,
  ctx: { me: string | null; names?: Record<string, string> } = { me: null }
): { sources: Record<string, SourceResolver>; functions: Record<string, (args: Record<string, unknown>) => Promise<unknown>> } {
  const schemaOf = (kind: string) => {
    const def = pkg.records?.[kind]
    if (!def) throw new Error(`records: no kind ${kind}`)
    return def.schema
  }
  return {
    sources: {
      [RECORDS_SCHEME]: (source, emit) => {
        const query = parseRecordsSource(source)
        if (!query || !pkg.records?.[query.kind]) return void emit(null)
        return store.subscribe((rows) => emit(recordsValue(rows, query, ctx)))
      },
    },
    functions: {
      "records.upsert": async (args) => {
        const id = typeof args.id === `string` && args.id ? args.id : undefined
        const data = isObject(args.data) ? args.data : {}
        const base = id ? store.get(id) : undefined
        if (id && !base) throw new Error(`record ${id} not found`)
        const kind = base?.kind ?? String(args.kind ?? ``)
        const issues = validateRecord(schemaOf(kind), base ? { ...base.data, ...data } : data)
        if (issues.length) throw new Error(`records: ${issues.map((i) => `${i.path || `/`} ${i.message}`).join(`; `)}`)
        return store.upsert({ kind, data, ...(id ? { id } : {}) })
      },
      "records.delete": async (args) => {
        await store.delete(String(args.id ?? ``))
        return null
      },
    },
  }
}
