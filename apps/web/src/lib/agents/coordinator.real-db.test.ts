// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { COORDINATOR_COPY } from "@exp/domain-contract/agent-settings"
import { db } from "@/db/connection"
import { agents, devices, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1717 (agent-settings.json `coordinator`): the team's owners make one
// team agent its coordinator; at most one per team. Against the REAL
// migrated Postgres.

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
  relayPostNudge: async () => {},
}))

const ready = await probeRealDb(
  `SELECT EXISTS (SELECT 1 FROM pg_enum WHERE enumlabel = 'coordinator') AS ok`
)
if (!ready.ok) console.info(`[coordinator.real-db] skipped: ${ready.reason}`)

const { agentsRouter } = await import(`@/lib/trpc/agents`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const person = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) })

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function user(): Promise<string> {
  const id = `coord-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@coord.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const mate = await user()
  const team = async () => {
    const [row] = await db.insert(teams).values({ name: `Coord`, slug: `coord-${id8()}` }).returning({ id: teams.id })
    made.teams.push(row!.id)
    await db.insert(teamMembers).values([
      { teamId: row!.id, userId: owner, role: `owner` },
      { teamId: row!.id, userId: mate, role: `member` },
    ])
    return row!.id
  }
  const teamId = await team()
  const other = await team()
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `mac`, kind: `desktop` })
  const agent = async (team: string, name: string, role: `agent` | `assistant` = `agent`) => {
    const [row] = await db
      .insert(agents)
      .values({ teamId: team, name, instructions: ``, triggers: [], deviceId, ownerId: owner, role })
      .returning({ id: agents.id })
    return row!.id
  }
  return { owner, mate, teamId, other, agent }
}

const roleOf = async (id: string) => (await db.select({ role: agents.role }).from(agents).where(eq(agents.id, id)))[0]?.role

describe.skipIf(!ready.ok)(`agents.setCoordinator (#EXP-1717)`, () => {
  it(`an owner sets one; a new pick moves the role; none clears it`, async () => {
    const w = await world()
    const a = await w.agent(w.teamId, `Triage`)
    const b = await w.agent(w.teamId, `Release`)
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await asOwner.setCoordinator({ teamId: w.teamId, agentId: a })
    expect(await roleOf(a)).toBe(`coordinator`)
    await asOwner.setCoordinator({ teamId: w.teamId, agentId: b })
    expect([await roleOf(a), await roleOf(b)]).toEqual([`agent`, `coordinator`])
    await asOwner.setCoordinator({ teamId: w.teamId, agentId: null })
    const left = await db.select().from(agents).where(and(eq(agents.teamId, w.teamId), eq(agents.role, `coordinator`)))
    expect(left).toEqual([])
  })

  it(`a member, an Assistant and another team's agent are refused`, async () => {
    const w = await world()
    const a = await w.agent(w.teamId, `Triage`)
    const assistant = await w.agent(w.teamId, `Assistant`, `assistant`)
    const foreign = await w.agent(w.other, `Elsewhere`)
    await expect(
      agentsRouter.createCaller(person(w.mate) as never).setCoordinator({ teamId: w.teamId, agentId: a })
    ).rejects.toThrow(COORDINATOR_COPY.refusedOwner)
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await expect(asOwner.setCoordinator({ teamId: w.teamId, agentId: assistant })).rejects.toThrow(
      COORDINATOR_COPY.refusedAgent
    )
    await expect(asOwner.setCoordinator({ teamId: w.teamId, agentId: foreign })).rejects.toThrow(
      COORDINATOR_COPY.refusedAgent
    )
    expect(await roleOf(a)).toBe(`agent`)
  })
})
