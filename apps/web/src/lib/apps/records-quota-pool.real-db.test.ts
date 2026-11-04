// @vitest-environment node
// Dogfood (spec §20): on a cloud instance the record quota took a second pool
// connection (`getTeamPlan` on the global db) while holding the team's advisory
// lock, so parallel creates past the pool size filled the pool with waiters on
// that lock and the holder never got its connection: the whole server hung. A
// 64-row batch on a kind with a record-create automation was enough. Against
// the REAL migrated Postgres, with the pool's default size.
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { db, pool } from "@/db/connection"
import { apps, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/bootstrap-cloud`, async (importOriginal) => ({ ...(await importOriginal<typeof import("@/lib/bootstrap-cloud")>()), isCloudInstance: () => true }))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'app_records') AS ok`)
const { upsertRecord } = await import(`@/lib/apps/records-service`)

const made: { team?: string; user?: string } = {}
const PACKAGE = { records: { log: { schema: { type: `object`, properties: { n: { type: `number` } } } } } }

afterAll(async () => {
  if (!ready.ok) return
  if (made.team) await db.delete(teams).where(sql`id = ${made.team}`)
  if (made.user) await db.execute(sql`DELETE FROM users WHERE id = ${made.user}`)
})

describe.skipIf(!ready.ok)(`the record quota under parallel creates (real db)`, () => {
  it(`more creates than the pool has connections all settle`, async () => {
    const owner = `quota-${crypto.randomUUID().slice(0, 8)}`
    await db.execute(sql`INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at) VALUES (${owner}, 'Q', ${`${owner}@quota.test`}, true, false, now(), now())`)
    made.user = owner
    const [team] = await db.insert(teams).values({ name: `Q`, slug: owner }).returning({ id: teams.id })
    made.team = team!.id
    await db.insert(teamMembers).values({ teamId: team!.id, userId: owner, role: `owner` })
    const [app] = await db.insert(apps).values({ teamId: team!.id, slug: `q`, name: `Q`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner }).returning({ id: apps.id })

    const creates = Array.from({ length: pool.options.max! * 4 }, (_, n) => upsertRecord(owner, { id: app!.id }, { kind: `log`, data: { n } }))
    const settled = Promise.allSettled(creates)
    const hung = new Promise<`hung`>((resolve) => setTimeout(() => resolve(`hung`), 20_000).unref())
    const out = await Promise.race([settled, hung])
    expect(out).not.toBe(`hung`)
    const results = out as PromiseSettledResult<unknown>[]
    // A refusal by the plan's quota is fine; a hang or a driver error is not.
    for (const r of results) if (r.status === `rejected`) expect(String(r.reason)).toMatch(/plan|limit|record/i)
    expect(results.filter((r) => r.status === `fulfilled`).length).toBeGreaterThan(0)
  }, 30_000)
})
