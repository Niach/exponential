// VAPP-269: the web runner's stores with fakes for the browser: `bun:sqlite`
// stands in for sqlite-wasm's oo1 DB behind the worker protocol (a real
// MessageChannel), fake-indexeddb for IndexedDB, a Map for OPFS.

import { Database } from "bun:sqlite"
import { describe, expect, test } from "bun:test"
import { IDBFactory } from "fake-indexeddb"

import { LocalRecordStore, type EngineRecordStore } from "../stores"
import { IndexedDbPersistence, IndexedDbVault, OpfsBlobStore, openRunnerIdb, openWebStorage, serveSql, sqlClient, SqlitePersistence, type OpfsDirectory, type SqlValue, type SyncSqlDatabase } from "./stores"

const bunSql = (db: Database): SyncSqlDatabase => ({
  query: (sql, bind) => db.query(sql).all(...(bind as SqlValue[])) as Record<string, SqlValue>[],
  run: (sql, bind) => void db.query(sql).run(...(bind as SqlValue[])),
})

/** A worker's `serveSql` over bun:sqlite, reached through a real MessageChannel. */
const workerDb = (db = new Database(`:memory:`)) => {
  const channel = new MessageChannel()
  serveSql(bunSql(db), channel.port1)
  channel.port1.start()
  channel.port2.start()
  return { client: sqlClient(channel.port2), db, close: () => (channel.port1.close(), channel.port2.close()) }
}

const AT = `2026-11-06T10:00:00.000Z`
const create = (writeId: string, id: string, data: Record<string, unknown> = {}) => ({ writeId, writes: [{ op: `upsert` as const, id, kind: `note`, data, create: true }], at: AT })

/** The store contract every backend keeps (the crate's sqlite test, case for case). */
async function storeContract(store: EngineRecordStore) {
  const first = await store.write(`a`, create(`w1`, `r1`), `d-1`)
  const again = await store.write(`a`, create(`w1`, `r1`), `d-1`)
  expect([first.replayed, again.replayed]).toEqual([false, true])
  expect(again.ids).toEqual([`r1`])
  expect((await store.rows(`a`)).length).toBe(1)
  await store.write(`a`, { writeId: `w2`, writes: [{ op: `upsert`, id: `r1`, kind: `note`, data: { text: `x` }, create: false }], at: `2026-11-06T10:01:00.000Z` }, `d-1`)
  expect((await store.history(`a`, `r1`)).map((e) => e.op)).toEqual([`create`, `update`])
  expect((await store.rows(`a`))[0]!.data.text).toBe(`x`)
  const missing = await store.write(`a`, { writeId: `w3`, writes: [{ op: `delete`, id: `nope` }], at: AT }, null).catch((e) => e)
  expect(missing.code).toBe(`NOT_FOUND`)
  // Apps are apart.
  expect(await store.rows(`b`)).toEqual([])
}

describe(`SQLite over the worker protocol`, () => {
  test(`keeps the store contract`, async () => {
    const { client, close } = workerDb()
    await storeContract(new LocalRecordStore(new SqlitePersistence(client)))
    close()
  })

  test(`rows survive a reopen (the file is the authority)`, async () => {
    const db = new Database(`:memory:`)
    const one = workerDb(db)
    await new LocalRecordStore(new SqlitePersistence(one.client)).write(`a`, create(`w1`, `r1`, { text: `kept` }), `d-1`)
    one.close()
    const two = workerDb(db)
    const store = new LocalRecordStore(new SqlitePersistence(two.client))
    const rows = await store.rows(`a`)
    expect(rows).toEqual([{ id: `r1`, kind: `note`, data: { text: `kept` }, createdBy: `d-1`, authorId: `d-1`, createdAt: AT, updatedAt: AT }])
    expect((await store.write(`a`, create(`w1`, `r1`), `d-1`)).replayed).toBe(true)
    two.close()
  })

  test(`a failing transaction lands nothing`, async () => {
    const { client, db, close } = workerDb()
    const error = await client.transaction([{ sql: `INSERT INTO app_record_writes (app_id, write_id, ids) VALUES ('a', 'w', '[]')` }, { sql: `INSERT INTO nope VALUES (1)` }]).catch((e) => e)
    expect(error).toBeInstanceOf(Error)
    expect(db.query(`SELECT count(*) AS n FROM app_record_writes`).get()).toEqual({ n: 0 })
    close()
  })
})

describe(`the IndexedDB fallback`, () => {
  test(`keeps the store contract`, async () => {
    const idb = await openRunnerIdb(new IDBFactory(), `runner`)
    await storeContract(new LocalRecordStore(new IndexedDbPersistence(idb)))
  })

  test(`the vault reads, writes and deletes`, async () => {
    const vault = new IndexedDbVault(await openRunnerIdb(new IDBFactory(), `runner`))
    expect(await vault.read(`k`)).toBeNull()
    await vault.write(`k`, `v`)
    expect(await vault.read(`k`)).toBe(`v`)
    await vault.delete(`k`)
    expect(await vault.read(`k`)).toBeNull()
  })

  test(`openWebStorage falls back when SQLite cannot open`, async () => {
    const root = memoryOpfs()
    const opened = await openWebStorage({ indexedDB: new IDBFactory(), idbName: `x`, openSqlite: () => Promise.reject(new Error(`no SAH`)), files: async () => root })
    expect(opened.backend).toBe(`indexeddb`)
    const sqlite = await openWebStorage({ indexedDB: new IDBFactory(), idbName: `x`, openSqlite: async () => workerDb().client, files: async () => root })
    expect(sqlite.backend).toBe(`sqlite`)
  })
})

const notFound = () => Object.assign(new Error(`not found`), { name: `NotFoundError` })

/** An in-memory OPFS directory. */
function memoryOpfs(): OpfsDirectory & { files: Map<string, Uint8Array>; dirs: Map<string, ReturnType<typeof memoryOpfs>> } {
  const files = new Map<string, Uint8Array>()
  const dirs = new Map<string, ReturnType<typeof memoryOpfs>>()
  return {
    files,
    dirs,
    async getDirectoryHandle(name, options) {
      let d = dirs.get(name)
      if (!d && options?.create) dirs.set(name, (d = memoryOpfs()))
      if (!d) throw notFound()
      return d
    },
    async getFileHandle(name, options) {
      if (!files.has(name)) {
        if (!options?.create) throw notFound()
        files.set(name, new Uint8Array())
      }
      return {
        getFile: async () => ({ arrayBuffer: async () => files.get(name)!.slice().buffer }),
        createWritable: async () => {
          let data = new Uint8Array()
          return { write: async (d: Uint8Array | string) => void (data = typeof d === `string` ? new TextEncoder().encode(d) : d.slice()), close: async () => void files.set(name, data) }
        },
      }
    },
    async removeEntry(name) {
      if (!files.delete(name)) throw notFound()
    },
  }
}

describe(`the OPFS blob store`, () => {
  test(`puts, gets and deletes an app's files`, async () => {
    const root = memoryOpfs()
    const blobs = new OpfsBlobStore(async () => root)
    const id = await blobs.put(`acme.album`, `a.png`, `image/png`, new Uint8Array([1, 2, 3]))
    expect(await blobs.get(`acme.album`, id)).toEqual(new Uint8Array([1, 2, 3]))
    expect(JSON.parse(new TextDecoder().decode(root.dirs.get(`acme.album`)!.files.get(`${id}.json`)))).toEqual({ name: `a.png`, mime: `image/png` })
    expect(await blobs.get(`other.app`, id)).toBeNull()
    expect(await blobs.get(`acme.album`, `../etc`)).toBeNull()
    await blobs.delete(`acme.album`, id)
    expect(await blobs.get(`acme.album`, id)).toBeNull()
    await blobs.delete(`acme.album`, id) // idempotent
    await expect(blobs.put(`../x`, `a`, `b`, new Uint8Array())).rejects.toThrow(`not an app id`)
  })
})
