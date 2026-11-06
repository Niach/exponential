// VAPP-269: the record store an AppInstance writes through, the twin of the
// crate's `providers::RecordStore` + `stores.rs`. One store serves every app
// of a host, keyed by app id: a batch applies all or nothing, a known
// `writeId` answers its first ack (`replayed`), every write leaves one history
// event per record. `LocalRecordStore` keeps each app's rows in memory over a
// `RecordPersistence` (memory here; SQLite-WASM on OPFS and IndexedDB in
// `./web/stores`), so a read never waits on storage.

import type { RecordRow } from "@exponential_app/eui/host"

import type { Failure, Vault } from "./broker"
import type { BlobStore } from "./tools"

/** One write of a batch (the store's spelling of the outbox writes). */
export type RecordWrite = { op: `upsert`; id: string; kind: string; data: Record<string, unknown>; create: boolean } | { op: `delete`; id: string }

/** One call's writes, all or nothing, with its idempotency key. */
export interface WriteBatch {
  writeId: string
  writes: RecordWrite[]
  /** ISO time of the write (createdAt / updatedAt of what it touches). */
  at: string
}

/** The ids in write order; `replayed` = the writeId was applied before. */
export interface WriteAck {
  ids: string[]
  replayed: boolean
}

/** One history event of a record. */
export interface RecordEvent {
  recordId: string
  op: `create` | `update` | `delete`
  actor: string | null
  writeId: string
  patch: unknown
  before: unknown
  at: string
}

export interface RecordsDelta {
  upserts: RecordRow[]
  deletes: string[]
}

/** Where an app's records live. Rejects with a `StoreError`. */
export interface EngineRecordStore {
  rows(app: string): Promise<RecordRow[]>
  write(app: string, batch: WriteBatch, by: string | null): Promise<WriteAck>
  history(app: string, id: string): Promise<RecordEvent[]>
  /** Called with every change to `app`'s rows; returns the cancel. */
  subscribe(app: string, on: (delta: RecordsDelta) => void): () => void
}

/** A store's refusal as an Error carrying the Failure. */
export class StoreError extends Error implements Failure {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message)
    this.name = `StoreError`
  }
}

/** Apply one batch to `rows` (all or nothing: throws with nothing changed).
 *  Answers the next rows, the ack, the delta and the events. */
export function applyBatch(rows: readonly RecordRow[], batch: WriteBatch, by: string | null): { rows: RecordRow[]; ack: WriteAck; delta: RecordsDelta; events: RecordEvent[] } {
  const next = rows.map((r) => ({ ...r }))
  const delta: RecordsDelta = { upserts: [], deletes: [] }
  const events: RecordEvent[] = []
  const ids: string[] = []
  for (const w of batch.writes) {
    const at = next.findIndex((r) => r.id === w.id)
    if (w.op === `upsert`) {
      if (at >= 0) {
        if (w.create) throw new StoreError(`CONFLICT`, `${w.id} exists`)
        const row = next[at]!
        const before = row.data
        next[at] = { ...row, data: { ...(row.data ?? {}), ...w.data }, updatedAt: batch.at }
        delta.upserts.push(next[at]!)
        events.push({ recordId: w.id, op: `update`, actor: by, writeId: batch.writeId, patch: w.data, before, at: batch.at })
      } else {
        if (!w.create) throw new StoreError(`NOT_FOUND`, `no record ${w.id}`)
        const row: RecordRow = { id: w.id, kind: w.kind, data: { ...w.data }, createdBy: by, authorId: by, createdAt: batch.at, updatedAt: batch.at }
        next.push(row)
        delta.upserts.push(row)
        events.push({ recordId: w.id, op: `create`, actor: by, writeId: batch.writeId, patch: w.data, before: null, at: batch.at })
      }
    } else {
      if (at < 0) throw new StoreError(`NOT_FOUND`, `no record ${w.id}`)
      const [gone] = next.splice(at, 1)
      delta.deletes.push(w.id)
      events.push({ recordId: w.id, op: `delete`, actor: by, writeId: batch.writeId, patch: null, before: gone!.data, at: batch.at })
    }
    ids.push(w.id)
  }
  return { rows: next, ack: { ids, replayed: false }, delta, events }
}

/** One committed batch, what a persistence stores in one transaction. */
export interface RecordCommit {
  writeId: string
  ids: string[]
  upserts: RecordRow[]
  deletes: string[]
  events: RecordEvent[]
}

/** Where a `LocalRecordStore` keeps its rows. */
export interface RecordPersistence {
  load(app: string): Promise<RecordRow[]>
  /** The ids of an applied writeId, or null. */
  ack(app: string, writeId: string): Promise<string[] | null>
  /** One batch, all or nothing. */
  commit(app: string, commit: RecordCommit): Promise<void>
  history(app: string, id: string): Promise<RecordEvent[]>
}

/** Every app's rows in memory over a persistence; writes run one at a time. */
export class LocalRecordStore implements EngineRecordStore {
  private cache = new Map<string, Promise<RecordRow[]>>()
  private listeners = new Map<string, Set<(delta: RecordsDelta) => void>>()
  private queue: Promise<unknown> = Promise.resolve()

  constructor(readonly persistence: RecordPersistence) {}

  private loaded(app: string): Promise<RecordRow[]> {
    let rows = this.cache.get(app)
    if (!rows) {
      rows = this.persistence.load(app)
      this.cache.set(app, rows)
      rows.catch(() => this.cache.delete(app))
    }
    return rows
  }

  async rows(app: string): Promise<RecordRow[]> {
    return [...(await this.loaded(app))]
  }

  write(app: string, batch: WriteBatch, by: string | null): Promise<WriteAck> {
    const run = async (): Promise<WriteAck> => {
      const known = await this.persistence.ack(app, batch.writeId)
      if (known) return { ids: known, replayed: true }
      const applied = applyBatch(await this.loaded(app), batch, by)
      await this.persistence.commit(app, { writeId: batch.writeId, ids: applied.ack.ids, upserts: applied.delta.upserts, deletes: applied.delta.deletes, events: applied.events })
      this.cache.set(app, Promise.resolve(applied.rows))
      for (const on of this.listeners.get(app) ?? []) on(applied.delta)
      return applied.ack
    }
    const result = this.queue.then(run, run)
    this.queue = result.catch(() => {})
    return result
  }

  history(app: string, id: string): Promise<RecordEvent[]> {
    return this.persistence.history(app, id)
  }

  subscribe(app: string, on: (delta: RecordsDelta) => void): () => void {
    const set = this.listeners.get(app) ?? new Set()
    set.add(on)
    this.listeners.set(app, set)
    return () => void set.delete(on)
  }
}

/** The persistence of tests and previews. */
export class MemoryPersistence implements RecordPersistence {
  private rows = new Map<string, RecordRow[]>()
  private acks = new Map<string, string[]>()
  private events = new Map<string, RecordEvent[]>()

  async load(app: string): Promise<RecordRow[]> {
    return [...(this.rows.get(app) ?? [])]
  }

  async ack(app: string, writeId: string): Promise<string[] | null> {
    return this.acks.get(`${app}\n${writeId}`) ?? null
  }

  async commit(app: string, commit: RecordCommit): Promise<void> {
    let rows = (this.rows.get(app) ?? []).filter((r) => !commit.deletes.includes(r.id))
    for (const row of commit.upserts) {
      const i = rows.findIndex((r) => r.id === row.id)
      rows = i >= 0 ? rows.map((r, j) => (j === i ? row : r)) : [...rows, row]
    }
    this.rows.set(app, rows)
    this.acks.set(`${app}\n${commit.writeId}`, commit.ids)
    for (const e of commit.events) this.events.set(`${app}\n${e.recordId}`, [...(this.events.get(`${app}\n${e.recordId}`) ?? []), e])
  }

  async history(app: string, id: string): Promise<RecordEvent[]> {
    return [...(this.events.get(`${app}\n${id}`) ?? [])]
  }
}

/** Every app's rows in memory (tests, previews). */
export class MemoryRecordStore extends LocalRecordStore {
  constructor() {
    super(new MemoryPersistence())
  }
}

/** A process-local vault (tests, previews). */
export class MemoryVault implements Vault {
  readonly values = new Map<string, string>()

  async read(key: string): Promise<string | null> {
    return this.values.get(key) ?? null
  }

  async write(key: string, value: string): Promise<void> {
    this.values.set(key, value)
  }

  async delete(key: string): Promise<void> {
    this.values.delete(key)
  }
}

/** Record files in memory (tests, previews). */
export class MemoryBlobStore implements BlobStore {
  readonly files = new Map<string, { name: string; mimeType: string; bytes: Uint8Array }>()

  async put(app: string, name: string, mimeType: string, bytes: Uint8Array): Promise<string> {
    const id = crypto.randomUUID()
    this.files.set(`${app}/${id}`, { name, mimeType, bytes: bytes.slice() })
    return id
  }

  async get(app: string, id: string): Promise<Uint8Array | null> {
    return this.files.get(`${app}/${id}`)?.bytes.slice() ?? null
  }

  async delete(app: string, id: string): Promise<void> {
    this.files.delete(`${app}/${id}`)
  }
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i

/** The `local` identity's device uuid under `key` in the Vault, created once. */
export async function localDeviceId(vault: Vault, key: string): Promise<string> {
  const stored = (await vault.read(key))?.trim()
  if (stored && UUID.test(stored)) return stored
  const id = crypto.randomUUID()
  await vault.write(key, id)
  return id
}
