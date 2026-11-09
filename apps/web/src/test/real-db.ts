// Real-Postgres tests (EXP-1284 review T16). Most suites mock the db; the
// ones that lock DATABASE rules (triggers, row locks, migrations' backfills,
// the chat helpers' SQL) run against the migrated dev/CI Postgres named by
// DATABASE_URL instead, each case inside ONE transaction that is always
// rolled back, so they leave nothing behind and can run beside a dev server.
//
// They SKIP (never fail) when that database is not reachable or not ready:
// CI's unit job sets an unreachable dummy URL (db/connection.ts throws at
// import without one), and a fresh checkout may not have applied the custom
// triggers yet. `realDbReady` names what is missing so a skip is never silent
// about why. #NEW-90: with REAL_DB_REQUIRED set (the release suite) a database
// that is not ready FAILS the suite instead, so a green run means they ran.
import { Pool } from "pg"
import { sql, type SQL } from "drizzle-orm"
import { db } from "@/db/connection"

// eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
export type RealDbTx = Parameters<Parameters<typeof import("@/db/connection").db.transaction>[0]>[0]

/** Probes DATABASE_URL once, with a short timeout. `requires` = SQL that must
 *  answer one row with a true `ok` (the schema/trigger state a suite needs). */
export async function probeRealDb(requires: string): Promise<{ ok: boolean; reason: string }> {
  const probe = await probeOnce(requires)
  if (!probe.ok && process.env.REAL_DB_REQUIRED) throw new Error(`real-DB suite required but not ready: ${probe.reason}`)
  return probe
}

async function probeOnce(requires: string): Promise<{ ok: boolean; reason: string }> {
  const url = process.env.DATABASE_URL
  if (!url) return { ok: false, reason: `DATABASE_URL is not set` }
  const pool = new Pool({ connectionString: url, connectionTimeoutMillis: 1500, max: 1 })
  try {
    const { rows } = await pool.query<{ ok: boolean }>(requires)
    return rows[0]?.ok
      ? { ok: true, reason: `` }
      : { ok: false, reason: `the database is not migrated or its custom triggers are not applied` }
  } catch (e) {
    return { ok: false, reason: e instanceof Error ? e.message : String(e) }
  } finally {
    await pool.end().catch(() => {})
  }
}

class Rollback extends Error {}

/** Runs `fn` in a transaction and ROLLS IT BACK, whatever happens. */
export async function inRollback(fn: (tx: RealDbTx) => Promise<void>): Promise<void> {
  const rollback = new Rollback(`rollback`)
  try {
    await db.transaction(async (tx) => {
      await fn(tx)
      throw rollback
    })
  } catch (e) {
    if (e !== rollback) throw e
  }
}

/** Runs `fn` again when Postgres rolled it back for a concurrent writer: a
 *  deadlock victim (40P01) or a REPEATABLE READ serialization failure
 *  (40001), up to 5 times. The real-DB files run side by side on ONE
 *  database and some replay table-wide migrations (host-spoofing's 0206/0213
 *  lock every `device_first_seen` row and FK-check their users), so a suite
 *  can lose to a neighbour it did nothing wrong to. Only for work that is
 *  safe to repeat: a rolled-back replay, an idempotent
 *  `DELETE … WHERE id IN (own ids)`. */
export async function retryingConflict<T>(run: () => Promise<T>): Promise<T> {
  for (let attempt = 1; ; attempt++) {
    try {
      return await run()
    } catch (error) {
      const e = error as { code?: string; cause?: { code?: string } }
      const code = e.code ?? e.cause?.code
      if ((code !== `40P01` && code !== `40001`) || attempt >= 5) throw error
    }
  }
}

/** Shadows each of `tables`, for the rest of `tx`, with a TEMP copy of only
 *  the rows `where` selects. Postgres resolves an unqualified name in
 *  `pg_temp` first, so a table-wide migration replayed verbatim afterwards
 *  reads and rewrites the case's own rows only: it never row-locks what the
 *  real-DB files beside it commit (the rollback drops the copies). */
export async function shadowTables(tx: RealDbTx, tables: readonly string[], where: SQL): Promise<void> {
  for (const table of tables) {
    const name = sql.identifier(table)
    await tx.execute(sql`CREATE TEMP TABLE ${name} (LIKE public.${name} INCLUDING DEFAULTS INCLUDING CONSTRAINTS INCLUDING INDEXES) ON COMMIT DROP`)
    await tx.execute(sql`INSERT INTO pg_temp.${name} SELECT * FROM public.${name} WHERE ${where}`)
  }
}
