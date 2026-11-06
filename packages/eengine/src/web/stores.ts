// VAPP-269 (E6d): the web runner's storage (runner.json `storage`). Records in
// SQLite-WASM on OPFS (the same tables as the crate's `SqliteRecordStore`),
// IndexedDB when OPFS's sync access handles are missing, files in OPFS, the
// Vault in IndexedDB. The SQLite engine runs in a worker (OPFS sync access
// handles exist only there): `serveSql` is the worker's half over any
// synchronous database, `sqlClient` the page's; the runner app wires
// `@sqlite.org/sqlite-wasm` in (the Engine never imports it).

import type { RecordRow } from "@exponential_app/eui/host"

import type { Vault } from "../broker"
import { LocalRecordStore, type RecordCommit, type RecordEvent, type RecordPersistence } from "../stores"
import type { BlobStore } from "../tools"

// ---------------------------------------------------------------- SQLite

/** The schema (idempotent): the crate's `sqlite::SCHEMA`, statement for statement. */
export const SQLITE_SCHEMA = [
  `CREATE TABLE IF NOT EXISTS app_records (app_id TEXT NOT NULL, id TEXT NOT NULL, kind TEXT NOT NULL, data TEXT NOT NULL, created_by TEXT, author_id TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY (app_id, id))`,
  `CREATE INDEX IF NOT EXISTS app_records_kind ON app_records (app_id, kind)`,
  `CREATE TABLE IF NOT EXISTS app_record_events (seq INTEGER PRIMARY KEY AUTOINCREMENT, app_id TEXT NOT NULL, record_id TEXT NOT NULL, op TEXT NOT NULL, actor TEXT, write_id TEXT NOT NULL, patch TEXT NOT NULL, before TEXT NOT NULL, at TEXT NOT NULL)`,
  `CREATE INDEX IF NOT EXISTS app_record_events_record ON app_record_events (app_id, record_id)`,
  `CREATE TABLE IF NOT EXISTS app_record_writes (app_id TEXT NOT NULL, write_id TEXT NOT NULL, ids TEXT NOT NULL, PRIMARY KEY (app_id, write_id))`,
]

export type SqlValue = string | number | null
export interface SqlStatement {
  sql: string
  bind?: SqlValue[]
}

/** A synchronous SQLite connection (sqlite-wasm's oo1 `DB`, `bun:sqlite` in tests). */
export interface SyncSqlDatabase {
  /** Rows as objects keyed by column name. */
  query(sql: string, bind: SqlValue[]): Record<string, SqlValue>[]
  run(sql: string, bind: SqlValue[]): void
}

/** The page's handle on the worker's database. */
export interface SqlDatabase {
  query(sql: string, bind?: SqlValue[]): Promise<Record<string, SqlValue>[]>
  /** The statements in ONE transaction: all or nothing. */
  transaction(statements: SqlStatement[]): Promise<void>
}

type SqlRequest = { id: number; op: `query`; sql: string; bind: SqlValue[] } | { id: number; op: `transaction`; statements: SqlStatement[] }
type SqlResponse = { id: number; rows?: Record<string, SqlValue>[]; error?: string }

/** The smallest MessagePort a worker and a page share. */
export interface PortLike {
  postMessage(message: unknown): void
  addEventListener(type: `message`, listener: (event: { data: unknown }) => void): void
}

/** Run one request against `db` (the worker's half, a transaction rolls back on any failure). */
export function answerSql(db: SyncSqlDatabase, request: SqlRequest): SqlResponse {
  try {
    if (request.op === `query`) return { id: request.id, rows: db.query(request.sql, request.bind) }
    db.run(`BEGIN IMMEDIATE`, [])
    try {
      for (const s of request.statements) db.run(s.sql, s.bind ?? [])
      db.run(`COMMIT`, [])
    } catch (error) {
      db.run(`ROLLBACK`, [])
      throw error
    }
    return { id: request.id }
  } catch (error) {
    return { id: request.id, error: error instanceof Error ? error.message : String(error) }
  }
}

/** Serve `db` over `port` (call it inside the worker once the database is open). */
export function serveSql(db: SyncSqlDatabase, port: PortLike): void {
  for (const s of SQLITE_SCHEMA) db.run(s, [])
  port.addEventListener(`message`, (event) => port.postMessage(answerSql(db, event.data as SqlRequest)))
}

/** The page's client of a worker's `serveSql`. */
export function sqlClient(port: PortLike): SqlDatabase {
  let next = 0
  const pending = new Map<number, { resolve: (r: SqlResponse) => void }>()
  port.addEventListener(`message`, (event) => {
    const r = event.data as SqlResponse
    pending.get(r.id)?.resolve(r)
    pending.delete(r.id)
  })
  const send = (request: SqlRequest): Promise<SqlResponse> =>
    new Promise((resolve, reject) => {
      pending.set(request.id, {
        resolve: (r) => (r.error === undefined ? resolve(r) : reject(new Error(r.error))),
      })
      port.postMessage(request)
    })
  return {
    query: async (sql, bind = []) => (await send({ id: ++next, op: `query`, sql, bind })).rows ?? [],
    transaction: async (statements) => void (await send({ id: ++next, op: `transaction`, statements })),
  }
}

const json = (v: unknown): string => JSON.stringify(v ?? null)
const parsed = (v: SqlValue): unknown => {
  try {
    return typeof v === `string` ? JSON.parse(v) : null
  } catch {
    return null
  }
}
const text = (v: SqlValue | undefined): string | null => (typeof v === `string` ? v : null)

/** The crate's `SqliteRecordStore` tables over a `SqlDatabase`. */
export class SqlitePersistence implements RecordPersistence {
  constructor(private readonly db: SqlDatabase) {}

  async load(app: string): Promise<RecordRow[]> {
    const rows = await this.db.query(`SELECT id, kind, data, created_by, author_id, created_at, updated_at FROM app_records WHERE app_id = ? ORDER BY created_at, id`, [app])
    return rows.map((r) => ({
      id: String(r.id),
      kind: String(r.kind),
      data: (parsed(r.data) as Record<string, unknown> | null) ?? {},
      createdBy: text(r.created_by),
      authorId: text(r.author_id),
      createdAt: String(r.created_at),
      updatedAt: String(r.updated_at),
    }))
  }

  async ack(app: string, writeId: string): Promise<string[] | null> {
    const [row] = await this.db.query(`SELECT ids FROM app_record_writes WHERE app_id = ? AND write_id = ?`, [app, writeId])
    return row ? ((parsed(row.ids) as string[] | null) ?? []) : null
  }

  async commit(app: string, c: RecordCommit): Promise<void> {
    const statements: SqlStatement[] = []
    for (const id of c.deletes) statements.push({ sql: `DELETE FROM app_records WHERE app_id = ? AND id = ?`, bind: [app, id] })
    for (const r of c.upserts)
      statements.push({
        sql: `INSERT INTO app_records (app_id, id, kind, data, created_by, author_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (app_id, id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at`,
        bind: [app, r.id, r.kind, json(r.data), r.createdBy, r.authorId ?? null, r.createdAt, r.updatedAt],
      })
    for (const e of c.events)
      statements.push({
        sql: `INSERT INTO app_record_events (app_id, record_id, op, actor, write_id, patch, before, at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
        bind: [app, e.recordId, e.op, e.actor, e.writeId, json(e.patch), json(e.before), e.at],
      })
    statements.push({ sql: `INSERT INTO app_record_writes (app_id, write_id, ids) VALUES (?, ?, ?)`, bind: [app, c.writeId, json(c.ids)] })
    await this.db.transaction(statements)
  }

  async history(app: string, id: string): Promise<RecordEvent[]> {
    const rows = await this.db.query(`SELECT record_id, op, actor, write_id, patch, before, at FROM app_record_events WHERE app_id = ? AND record_id = ? ORDER BY seq`, [app, id])
    return rows.map((r) => ({ recordId: String(r.record_id), op: r.op as RecordEvent[`op`], actor: text(r.actor), writeId: String(r.write_id), patch: parsed(r.patch), before: parsed(r.before), at: String(r.at) }))
  }
}

// ---------------------------------------------------------------- IndexedDB

const RECORDS = `records`
const WRITES = `writes`
const EVENTS = `events`
const VAULT = `vault`

const done = <T>(request: IDBRequest<T>): Promise<T> =>
  new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })

const committed = (tx: IDBTransaction): Promise<void> =>
  new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
    tx.onabort = () => reject(tx.error ?? new Error(`aborted`))
  })

/** Open (and create) the runner's IndexedDB database: records, writes, events and the vault. */
export function openRunnerIdb(factory: IDBFactory, name: string): Promise<IDBDatabase> {
  const request = factory.open(name, 1)
  request.onupgradeneeded = () => {
    const db = request.result
    db.createObjectStore(RECORDS, { keyPath: [`appId`, `id`] }).createIndex(`app`, `appId`)
    db.createObjectStore(WRITES, { keyPath: [`appId`, `writeId`] })
    db.createObjectStore(EVENTS, { autoIncrement: true }).createIndex(`record`, [`appId`, `recordId`])
    db.createObjectStore(VAULT)
  }
  return done(request)
}

/** The records' fallback when OPFS has no sync access handles: one IndexedDB transaction per batch. */
export class IndexedDbPersistence implements RecordPersistence {
  constructor(private readonly db: IDBDatabase) {}

  async load(app: string): Promise<RecordRow[]> {
    const all = await done(this.db.transaction(RECORDS).objectStore(RECORDS).index(`app`).getAll(app))
    return (all as (RecordRow & { appId: string })[]).map(({ appId: _, ...row }) => row).sort((a, b) => (a.createdAt < b.createdAt ? -1 : a.createdAt > b.createdAt ? 1 : a.id < b.id ? -1 : 1))
  }

  async ack(app: string, writeId: string): Promise<string[] | null> {
    const row = (await done(this.db.transaction(WRITES).objectStore(WRITES).get([app, writeId]))) as { ids: string[] } | undefined
    return row ? row.ids : null
  }

  async commit(app: string, c: RecordCommit): Promise<void> {
    const tx = this.db.transaction([RECORDS, WRITES, EVENTS], `readwrite`)
    const records = tx.objectStore(RECORDS)
    for (const id of c.deletes) records.delete([app, id])
    for (const r of c.upserts) records.put({ ...r, appId: app })
    for (const e of c.events) tx.objectStore(EVENTS).add({ ...e, appId: app })
    tx.objectStore(WRITES).put({ appId: app, writeId: c.writeId, ids: c.ids })
    await committed(tx)
  }

  async history(app: string, id: string): Promise<RecordEvent[]> {
    const all = await done(this.db.transaction(EVENTS).objectStore(EVENTS).index(`record`).getAll([app, id]))
    return (all as (RecordEvent & { appId: string })[]).map(({ appId: _, ...e }) => e)
  }
}

/** The Vault in IndexedDB (runner.json `storage`: the web's keystore). */
export class IndexedDbVault implements Vault {
  constructor(private readonly db: IDBDatabase) {}

  async read(key: string): Promise<string | null> {
    const v = await done(this.db.transaction(VAULT).objectStore(VAULT).get(key))
    return typeof v === `string` ? v : null
  }

  async write(key: string, value: string): Promise<void> {
    const tx = this.db.transaction(VAULT, `readwrite`)
    tx.objectStore(VAULT).put(value, key)
    await committed(tx)
  }

  async delete(key: string): Promise<void> {
    const tx = this.db.transaction(VAULT, `readwrite`)
    tx.objectStore(VAULT).delete(key)
    await committed(tx)
  }
}

// ---------------------------------------------------------------- OPFS files

/** An OPFS directory (the parts the BlobStore uses). */
export interface OpfsDirectory {
  getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<OpfsDirectory>
  getFileHandle(name: string, options?: { create?: boolean }): Promise<{ getFile(): Promise<{ arrayBuffer(): Promise<ArrayBuffer> }>; createWritable(): Promise<{ write(data: Uint8Array | string): Promise<void>; close(): Promise<void> }> }>
  removeEntry(name: string): Promise<void>
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const notFound = (e: unknown) => e instanceof Error && (e.name === `NotFoundError` || e.name === `TypeMismatchError`)

/** Record files in OPFS: `<root>/<app>/<id>` (+ `<id>.json` with name and mime), the crate's `FsBlobStore` layout. */
export class OpfsBlobStore implements BlobStore {
  constructor(private readonly root: () => Promise<OpfsDirectory>) {}

  private async dir(app: string, create: boolean): Promise<OpfsDirectory> {
    if (!app || /[/\\]/.test(app) || app.startsWith(`.`)) throw new Error(`not an app id`)
    return (await this.root()).getDirectoryHandle(app, { create })
  }

  private async writeFile(dir: OpfsDirectory, name: string, data: Uint8Array | string): Promise<void> {
    const writable = await (await dir.getFileHandle(name, { create: true })).createWritable()
    await writable.write(data)
    await writable.close()
  }

  async put(app: string, name: string, mimeType: string, bytes: Uint8Array): Promise<string> {
    const dir = await this.dir(app, true)
    const id = crypto.randomUUID()
    await this.writeFile(dir, id, bytes)
    await this.writeFile(dir, `${id}.json`, JSON.stringify({ name, mime: mimeType }))
    return id
  }

  async get(app: string, id: string): Promise<Uint8Array | null> {
    if (!UUID.test(id)) return null
    try {
      const file = await (await (await this.dir(app, false)).getFileHandle(id)).getFile()
      return new Uint8Array(await file.arrayBuffer())
    } catch (e) {
      if (notFound(e)) return null
      throw e
    }
  }

  async delete(app: string, id: string): Promise<void> {
    if (!UUID.test(id)) return
    let dir: OpfsDirectory
    try {
      dir = await this.dir(app, false)
    } catch (e) {
      if (notFound(e)) return
      throw e
    }
    for (const name of [id, `${id}.json`]) await dir.removeEntry(name).catch((e) => (notFound(e) ? undefined : Promise.reject(e)))
  }
}

// ---------------------------------------------------------------- choosing

export interface WebStorage {
  records: LocalRecordStore
  /** Which records backend opened: `sqlite` (OPFS) or the `indexeddb` fallback. */
  backend: `sqlite` | `indexeddb`
  vault: Vault
  blobs: BlobStore
}

/** The runner's stores: SQLite on OPFS when `openSqlite` opens, else IndexedDB. */
export async function openWebStorage(options: {
  indexedDB: IDBFactory
  /** The IndexedDB database name (per app id). */
  idbName: string
  /** Opens the worker's SQLite on OPFS; rejects when OPFS's sync access handles are missing. */
  openSqlite?: () => Promise<SqlDatabase>
  /** The OPFS directory `storage.files` lives in. */
  files: () => Promise<OpfsDirectory>
}): Promise<WebStorage> {
  const idb = await openRunnerIdb(options.indexedDB, options.idbName)
  const vault = new IndexedDbVault(idb)
  const blobs = new OpfsBlobStore(options.files)
  if (options.openSqlite) {
    try {
      const db = await options.openSqlite()
      return { records: new LocalRecordStore(new SqlitePersistence(db)), backend: `sqlite`, vault, blobs }
    } catch {
      // No OPFS sync access handles (an older Safari, a private window): IndexedDB.
    }
  }
  return { records: new LocalRecordStore(new IndexedDbPersistence(idb)), backend: `indexeddb`, vault, blobs }
}
