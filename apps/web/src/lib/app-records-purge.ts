// VAPP-159 PURGE (fixtures/app-records.json `purge`): hard-delete the app
// records trashed longer than the retention: a record deleted on its own
// (`deleted_at`) or a record of a trashed app (`app_deleted_at`; app trash
// has no restore). Neither row is in any shape any more, so the delete
// reaches no client; it only stops the tombstones growing. One bounded
// batch per watchdog tick, oldest first; never imports a router.

import { and, asc, inArray, sql } from "drizzle-orm"
import { APP_RECORDS_PURGE } from "@exp/domain-contract/app-records"
import { db } from "@/db/connection"
import { appRecords } from "@/db/schema"

type Executor = Pick<typeof db, `select` | `delete`>

/** Deletes at most `APP_RECORDS_PURGE.batch` due records; returns how many. */
export async function purgeTrashedAppRecords(now: Date = new Date(), executor: Executor = db): Promise<number> {
  const cutoff = new Date(now.getTime() - APP_RECORDS_PURGE.retentionDays * 24 * 60 * 60 * 1000)
  // `least` skips NULLs: the older of the two stamps, NULL = live. The
  // partial `idx_app_records_trashed` index walks exactly these rows.
  const trashedAt = sql`least(${appRecords.deletedAt}, ${appRecords.appDeletedAt})`
  const due = sql`${trashedAt} < ${cutoff.toISOString()}::timestamptz`
  const batch = executor
    .select({ id: appRecords.id })
    .from(appRecords)
    .where(due)
    .orderBy(asc(trashedAt))
    .limit(APP_RECORDS_PURGE.batch)
  const gone = await executor
    .delete(appRecords)
    .where(and(inArray(appRecords.id, batch), due))
    .returning({ id: appRecords.id })
  return gone.length
}
