// VAPP-269: the records' SQLite-WASM on OPFS (runner.json `storage.records`),
// in a worker: OPFS sync access handles exist only there. The page sends
// `{open: appId}`; the worker answers `{ready: true}` and serves the Engine's
// SQL protocol (`serveSql`), or `{failed}` (the page falls back to IndexedDB).

import sqlite3InitModule from "@sqlite.org/sqlite-wasm"
import { serveSql, type SqlValue, type SyncSqlDatabase } from "@exp/eengine/web"
import runner from "@exp/eengine/fixtures/runner.json" with { type: "json" }

const scope = self as unknown as { postMessage(m: unknown): void; addEventListener(t: `message`, l: (e: { data: unknown }) => void): void; removeEventListener(t: `message`, l: (e: { data: unknown }) => void): void }

const opening = async (event: { data: unknown }) => {
  const data = event.data as { open?: unknown }
  if (typeof data?.open !== `string`) return
  scope.removeEventListener(`message`, opening)
  const appId = data.open
  try {
    const sqlite3 = await sqlite3InitModule()
    const pool = await sqlite3.installOpfsSAHPoolVfs({ name: `exp-runner`, directory: `.exp-runner-sqlite`, preserveOnInitFailure: true })
    const db = new pool.OpfsSAHPoolDb(`/${appId}/${runner.storage.records}`)
    const sync: SyncSqlDatabase = {
      query: (sql, bind) => db.exec({ sql, bind: bind as never, rowMode: `object`, returnValue: `resultRows` }) as Record<string, SqlValue>[],
      run: (sql, bind) => void db.exec({ sql, bind: bind.length ? (bind as never) : undefined }),
    }
    serveSql(sync, scope)
    scope.postMessage({ ready: true })
  } catch (error) {
    scope.postMessage({ failed: error instanceof Error ? error.message : String(error) })
  }
}

scope.addEventListener(`message`, opening)
