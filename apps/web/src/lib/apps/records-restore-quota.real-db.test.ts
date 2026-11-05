// @vitest-environment node
// VAPP-227: a restore or undo that revives a deleted record takes the team's
// quota lock like a create, so a team at its plan's `appRecords` limit cannot
// pass it by restoring. Against the REAL migrated Postgres.
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appRecords, apps, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/bootstrap-cloud`, async (importOriginal) => ({ ...(await importOriginal<typeof import("@/lib/bootstrap-cloud")>()), isCloudInstance: () => true }))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'app_records') AS ok`)
const { deleteRecord, listRecordEvents, restoreRecord, undoEvents, upsertRecord } = await import(`@/lib/apps/records-service`)
const { PLAN_LIMITS } = await import(`@/lib/plan-limits`)

const made: { team?: string; user?: string } = {}
const PACKAGE = { records: { log: { schema: { type: `object`, properties: { n: { type: `number` } } } } } }

afterAll(async () => {
  if (!ready.ok) return
  if (made.team) await db.delete(teams).where(sql`id = ${made.team}`)
  if (made.user) await db.execute(sql`DELETE FROM users WHERE id = ${made.user}`)
})

describe.skipIf(!ready.ok)(`restore and undo against the record quota (real db)`, () => {
  it(`a team at its limit cannot revive a deleted record; below it, it can`, async () => {
    const owner = `restore-${crypto.randomUUID().slice(0, 8)}`
    await db.execute(sql`INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at) VALUES (${owner}, 'R', ${`${owner}@quota.test`}, true, false, now(), now())`)
    made.user = owner
    const [team] = await db.insert(teams).values({ name: `R`, slug: owner }).returning({ id: teams.id })
    made.team = team!.id
    await db.insert(teamMembers).values({ teamId: team!.id, userId: owner, role: `owner` })
    const [app] = await db.insert(apps).values({ teamId: team!.id, slug: `r`, name: `R`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner }).returning({ id: apps.id })
    const ref = { id: app!.id }

    const created = await upsertRecord(owner, ref, { kind: `log`, data: { n: 0 } })
    await deleteRecord(owner, ref, created.id)
    const { events } = await listRecordEvents(owner, ref, created.id)
    const createEvent = events.find((e) => e.op === `create`)!
    const deleteEvent = events.find((e) => e.op === `delete`)!

    // Fill the team to exactly its plan's limit.
    const rows = Array.from({ length: PLAN_LIMITS.free.appRecords }, (_, n) => ({ appId: app!.id, teamId: team!.id, kind: `log`, data: { n }, createdBy: owner, authorId: owner, updatedBy: owner, schemaVersion: 1 }))
    await db.insert(appRecords).values(rows as never)

    await expect(restoreRecord(owner, ref, { recordId: created.id, eventId: createEvent.id })).rejects.toThrow(/Upgrade|limit|plan/i)
    await expect(undoEvents(owner, ref, { eventIds: [deleteEvent.id] })).rejects.toThrow(/Upgrade|limit|plan/i)
    const [still] = await db.select({ deletedAt: appRecords.deletedAt }).from(appRecords).where(sql`id = ${created.id}`)
    expect(still!.deletedAt).not.toBeNull()

    // Delete one to make room: the revive now fits, and takes the last slot.
    const other = await upsertRecord(owner, ref, { kind: `log`, data: { n: -1 } }).catch(() => null)
    expect(other).toBeNull()
    await db.execute(sql`UPDATE app_records SET deleted_at = now() WHERE id = (SELECT id FROM app_records WHERE app_id = ${app!.id} AND deleted_at IS NULL LIMIT 1)`)
    await restoreRecord(owner, ref, { recordId: created.id, eventId: createEvent.id })
    const [back] = await db.select({ deletedAt: appRecords.deletedAt }).from(appRecords).where(sql`id = ${created.id}`)
    expect(back!.deletedAt).toBeNull()

    // A restore of a live row adds nothing, so it passes at the limit.
    await restoreRecord(owner, ref, { recordId: created.id, eventId: createEvent.id })
  }, 30_000)
})
