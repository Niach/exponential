// eApps R8 (#VAPP-233, record-migrations.json, docs/spec/apps.md §3.6): the
// publish sweep. A publish of a version whose package declares `migrations`
// marks its `app_versions` row `running` (publishVersionInTx); this drives
// it: batch after batch through the records service (`sweepRecordsBatch`:
// the `eapps` role, all or nothing per batch, history events via
// `migration`), the row's `migration_done` / `migration_total` the progress
// the Versions list shows. Resumable: the state lives in the database (the
// row + each record's `schema_version`), so the watchdog picks every
// running sweep up again each minute and a restart loses nothing; two
// replicas never take the same rows (SKIP LOCKED) and a sweep is done only
// once no record is left (rows another run holds count). A record that cannot
// move fails the sweep (`failed`, the error on the row): the records of the
// batch stay as they were and a new publish starts over.

import { and, eq, sql } from "drizzle-orm"
import { db as defaultDb } from "@/db/connection"
import { appVersions } from "@/db/schema"
import { sweepRecordsBatch } from "@/lib/apps/records-service"

type Executor = typeof defaultDb

/** How long one call keeps sweeping one app before it yields (the next tick resumes). */
export const RECORD_SWEEP_BUDGET_MS = 20_000

/** Advance every running sweep (or one app's) within the budget; the
 *  number of records moved. */
export async function runRecordSweeps(opts: { appId?: string; budgetMs?: number; limit?: number } = {}, executor: Executor = defaultDb): Promise<number> {
  const running = await executor
    .select({ appId: appVersions.appId, number: appVersions.number, from: appVersions.migrationFrom, by: appVersions.migrationBy, total: appVersions.migrationTotal, done: appVersions.migrationDone })
    .from(appVersions)
    .where(and(eq(appVersions.migrationState, `running`), opts.appId ? eq(appVersions.appId, opts.appId) : undefined))
    .limit(20)
  let moved = 0
  for (const sweep of running) {
    const until = Date.now() + (opts.budgetMs ?? RECORD_SWEEP_BUDGET_MS)
    const row = and(eq(appVersions.appId, sweep.appId), eq(appVersions.number, sweep.number), eq(appVersions.migrationState, `running`))
    for (;;) {
      const batch = await sweepRecordsBatch(sweep.appId, sweep.number, { from: sweep.from, actor: sweep.by, limit: opts.limit }, executor)
      if (`superseded` in batch) {
        await executor.update(appVersions).set({ migrationState: `superseded` }).where(row)
        break
      }
      if (`error` in batch) {
        await executor.update(appVersions).set({ migrationState: `failed`, migrationError: batch.error }).where(row)
        break
      }
      moved += batch.moved
      // total = done + pending: exact once nothing is pending, and two runs
      // at once (a publish's kick and a tick) only ever add their own moves.
      await executor
        .update(appVersions)
        .set({
          migrationDone: sql`${appVersions.migrationDone} + ${batch.moved}`,
          migrationTotal: sql`${appVersions.migrationDone} + ${batch.moved + batch.pending}`,
          ...(batch.pending === 0 ? { migrationState: `done` as const } : {}),
        })
        .where(row)
      if (batch.pending === 0 || batch.moved === 0 || Date.now() >= until) break
    }
  }
  return moved
}
