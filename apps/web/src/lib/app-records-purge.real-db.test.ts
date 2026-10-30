// @vitest-environment node
import { describe, expect, it } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { APP_RECORDS_PURGE } from "@exp/domain-contract/app-records"
import { appRecords, apps, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"
import { purgeTrashedAppRecords } from "./app-records-purge"

// VAPP-159 PURGE (fixtures/app-records.json `purge`) against the REAL
// migrated Postgres, inside a rolled-back transaction: a record deleted (or
// whose app was trashed) longer than the retention is hard-deleted, a
// younger tombstone and a live row stay, one call deletes at most a batch.
// The purge scans the whole table, so it runs at a `now` in 2000 and only
// this case's backdated rows are that old.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_app_records_trashed') AS ok
`)
if (!ready.ok) console.info(`[app-records-purge.real-db] skipped: ${ready.reason}`)

const NOW = new Date(`2000-01-01T00:00:00.000Z`)
const DAY = 24 * 60 * 60 * 1000
const ago = (days: number) => new Date(NOW.getTime() - days * DAY)

describe.skipIf(!ready.ok)(`app records purge (real db)`, () => {
  it(`hard-deletes only the records trashed past the retention, in batches`, async () => {
    await inRollback(async (tx) => {
      const [team] = await tx.insert(teams).values({ name: `Purge`, slug: `purge-${crypto.randomUUID().slice(0, 8)}` }).returning({ id: teams.id })
      const teamId = team!.id
      const [app] = await tx.insert(apps).values({ teamId, slug: `notes`, name: `Notes` }).returning({ id: apps.id })
      const [gone] = await tx.insert(apps).values({ teamId, slug: `old`, name: `Old` }).returning({ id: apps.id })
      const record = async (appId: string) =>
        (await tx.insert(appRecords).values({ appId, teamId, kind: `note`, data: {} }).returning({ id: appRecords.id }))[0]!.id
      const live = await record(app!.id)
      const fresh = await record(app!.id)
      const stale = await record(app!.id)
      const ofTrashedApp = await record(gone!.id)
      const days = APP_RECORDS_PURGE.retentionDays
      await tx.update(appRecords).set({ deletedAt: ago(days - 1) }).where(eq(appRecords.id, fresh))
      await tx.update(appRecords).set({ deletedAt: ago(days + 1) }).where(eq(appRecords.id, stale))
      // The trigger mirrors the app's trash onto its records.
      await tx.update(apps).set({ deletedAt: ago(days + 2) }).where(eq(apps.id, gone!.id))

      expect(await purgeTrashedAppRecords(NOW, tx)).toBe(2)
      const left = await tx.select({ id: appRecords.id }).from(appRecords).where(inArray(appRecords.id, [live, fresh, stale, ofTrashedApp]))
      expect(left.map((r) => r.id).sort()).toEqual([live, fresh].sort())
      expect(await purgeTrashedAppRecords(NOW, tx)).toBe(0)

      // A backlog larger than one batch drains over several ticks.
      await tx.execute(sql`
        INSERT INTO app_records (app_id, team_id, kind, data, deleted_at)
        SELECT ${app!.id}, ${teamId}, 'note', '{}'::jsonb, ${ago(days + 3).toISOString()}::timestamptz
        FROM generate_series(1, ${APP_RECORDS_PURGE.batch + 5})
      `)
      expect(await purgeTrashedAppRecords(NOW, tx)).toBe(APP_RECORDS_PURGE.batch)
      expect(await purgeTrashedAppRecords(NOW, tx)).toBe(5)
    })
  })
})
