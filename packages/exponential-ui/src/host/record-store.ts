// VAPP-161: the format's NEUTRAL records (docs/package-format.md "Records").
// A surface reads a kind through the binding `records:<kind>?query` and
// writes through the host functions `records.upsert` / `records.delete`;
// the HOST provides the store (Exponential: its synced store, spezl: a p2p
// store over eNet). `RecordStore` is what a host implements;
// `MemoryRecordStore` + `recordsHost` are the reference; the query and its
// value are query.ts (VAPP-189: one evaluator over the rows of any store,
// fixtures/records-query.json).

import type { AppPackage } from "./package"
import { RECORDS_SCHEME, parseRecordsSource, recordsValue, type RecordRow, type RecordsContext } from "./query"
import { validateRecord, withoutNulls } from "./records"
import { RECORDS_BATCH_FUNCTION, applyRecordBatch, type ActionWrite } from "./actions"
import { RecordRefusal } from "./record-macros"
import type { SourceResolver } from "./sources"

/** The write functions (no consent card: the app's own rows). */
export const RECORDS_FUNCTIONS: readonly string[] = [`records.upsert`, `records.delete`, RECORDS_BATCH_FUNCTION]

/** What a host provides: one package's records, any transport. */
export interface RecordStore {
  /** Every row of the package (any kind) now, then on every change; returns the cancel. */
  subscribe(emit: (rows: readonly RecordRow[]) => void): () => void
  /** Create (no id) or shallow-patch `data` (id); resolves the row's id. The
   *  caller has validated the result against the kind's schema. */
  upsert(input: { kind: string; data: Record<string, unknown>; id?: string }): Promise<{ id: string }>
  delete(id: string): Promise<void>
  /** VAPP-194: the writes as ONE unit, all or none (a package action's);
   *  resolves each write's id, rejects with nothing stored. The caller has
   *  NOT validated a patch (the store merges and checks it). */
  batch(writes: readonly ActionWrite[], records: Record<string, unknown> | undefined): Promise<{ ids: string[] }>
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
    this.rows = [...this.rows, { id, kind: input.kind, data: withoutNulls(input.data), createdBy: this.options.me ?? null, createdAt: now, updatedAt: now }]
    this.changed()
    return { id }
  }

  async delete(id: string): Promise<void> {
    if (!this.get(id)) throw new Error(`record ${id} not found`)
    this.rows = this.rows.filter((r) => r.id !== id)
    this.changed()
  }

  async batch(writes: readonly ActionWrite[], records: Record<string, unknown> | undefined): Promise<{ ids: string[] }> {
    const now = this.options.now?.() ?? new Date().toISOString()
    const result = applyRecordBatch(this.rows, writes, { records, me: this.options.me ?? null, now, newId: () => this.options.newId?.() ?? `r${++this.seq}` })
    if (`error` in result) throw new Error(result.error)
    this.rows = result.rows
    if (writes.length) this.changed()
    return { ids: result.ids }
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
  ctx: RecordsContext = { me: null }
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
        if (!query?.from || !pkg.records?.[query.from]) return void emit(null)
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
        if (issues.length) throw new RecordRefusal(`records: ${issues.map((i) => `${i.path || `/`} ${i.message}`).join(`; `)}`)
        return store.upsert({ kind, data, ...(id ? { id } : {}) })
      },
      "records.delete": async (args) => {
        await store.delete(String(args.id ?? ``))
        return null
      },
      // VAPP-194: a package action's writes, one unit.
      [RECORDS_BATCH_FUNCTION]: async (args) => store.batch(Array.isArray(args.writes) ? (args.writes as ActionWrite[]) : [], pkg.records as Record<string, unknown> | undefined),
    },
  }
}
