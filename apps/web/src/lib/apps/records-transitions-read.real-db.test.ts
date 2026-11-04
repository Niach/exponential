// @vitest-environment node
// Dogfood (spec §20): "every row of such a kind carries `transitions`" held on
// the hosts' local query only; a server read (tRPC `apps.records.query`, MCP
// `records_list`, an exposed tool) dropped it, so an agent could not see the
// next states. Against the REAL migrated Postgres.
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { apps, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'app_record_events') AS ok`)
const { queryRecords, upsertRecord } = await import(`@/lib/apps/records-service`)

const made = { teams: [] as string[], users: [] as string[] }
const PACKAGE = {
  records: {
    request: {
      schema: { type: `object`, required: [`status`], properties: { status: { type: `string`, enum: [`requested`, `approved`, `paid`] } } },
      transitions: { field: `status`, from: { requested: [`approved`], approved: [`paid`] }, who: { "approved->paid": `owners` } },
    },
    plain: { schema: { type: `object`, properties: { name: { type: `string` } } } },
  },
}

async function user(): Promise<string> {
  const id = `tr-${crypto.randomUUID().slice(0, 8)}`
  await db.execute(sql`INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at) VALUES (${id}, 'T', ${`${id}@tr.test`}, true, false, now(), now())`)
  made.users.push(id)
  return id
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`a server read's transitions (real db)`, () => {
  it(`each row carries the next states THIS reader may take; other kinds carry none`, async () => {
    const owner = await user()
    const member = await user()
    const [team] = await db.insert(teams).values({ name: `T`, slug: `tr-${crypto.randomUUID().slice(0, 8)}` }).returning({ id: teams.id })
    made.teams.push(team!.id)
    await db.insert(teamMembers).values([
      { teamId: team!.id, userId: owner, role: `owner` },
      { teamId: team!.id, userId: member, role: `member` },
    ])
    const [app] = await db.insert(apps).values({ teamId: team!.id, slug: `t`, name: `T`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner }).returning({ id: apps.id })
    const ref = { id: app!.id }
    const { id } = await upsertRecord(owner, ref, { kind: `request`, data: { status: `requested` } })
    await upsertRecord(owner, ref, { kind: `plain`, data: { name: `n` } })
    const read = async (who: string, kind = `request`) => (await queryRecords(who, ref, { kind, query: {} })).rows[0] as Record<string, unknown>
    expect((await read(member)).transitions).toEqual([`approved`])
    await upsertRecord(owner, ref, { kind: `request`, recordId: id, data: { status: `approved` } })
    expect((await read(member)).transitions).toEqual([])
    expect((await read(owner)).transitions).toEqual([`paid`])
    expect(await read(owner, `plain`)).not.toHaveProperty(`transitions`)
  })
})
