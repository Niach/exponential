// #EXP-1793: an agent's MCP servers (`agents.mcp_server_ids`, agent-settings.json
// `mcpRow`) reach every run of it: its conversation (created with them, a
// definition change carries over), a send to the agent with `mcpServerIds`
// sets the conversation's pick (a foreign id is refused, never dropped), and
// a root its tree starts without a pick takes the agent's.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, devices, mcpServers, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: () => null,
  relayPostNudge: async () => {},
  relayPostKill: async () => {},
  relayPostInterrupt: async () => {},
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: () => {},
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
}))

const ready = await probeRealDb(`
  SELECT EXISTS (
    SELECT 1 FROM information_schema.columns WHERE table_name = 'agents' AND column_name = 'mcp_server_ids'
  ) AS ok
`)
if (!ready.ok) console.info(`[agent-mcp-servers.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { agentsRouter } = await import(`@/lib/trpc/agents`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `amcp-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@agent-mcp.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(owner: string) {
  const [row] = await db.insert(teams).values({ name: `Agent MCP`, slug: `amcp-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values([{ teamId: row!.id, userId: owner, role: `owner` }])
  return row!.id
}

const server = async (teamId: string, name: string) =>
  (
    await db
      .insert(mcpServers)
      .values({ teamId, name, transport: `http`, url: `https://${name}.example.com/mcp`, auth: `none` })
      .returning({ id: mcpServers.id })
  )[0]!.id

const caller = (userId: string) =>
  agentsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

const conversation = async (agentId: string, userId: string) =>
  (
    await db
      .select()
      .from(sessions)
      .where(and(eq(sessions.agentId, agentId), eq(sessions.userId, userId), isNull(sessions.parentId)))
  )[0]

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`agent MCP servers (#EXP-1793)`, () => {
  it(`the definition, its conversation, a send to it and the roots its tree starts`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const otherTeam = await team(owner)
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
    const glitchtip = await server(teamId, `glitchtip`)
    const linear = await server(teamId, `linear`)
    const foreign = await server(otherTeam, `foreign`)

    // create: in-team ids only.
    await expect(
      caller(owner).create({ teamId, name: `Bad`, deviceId, instructions: ``, triggers: [], mcpServerIds: [foreign] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const { agent } = await caller(owner).create({
      teamId,
      name: `Triage`,
      deviceId,
      instructions: ``,
      triggers: [],
      mcpServerIds: [glitchtip, glitchtip],
    })
    expect(agent.mcpServerIds).toEqual([glitchtip])

    // A send to the agent creates the conversation WITH them.
    await sendSession({ userId: owner, input: { agentId: agent.id, text: `hi` } as never })
    expect((await conversation(agent.id, owner))!.mcpServerIds).toEqual([glitchtip])

    // update carries a change onto the conversations; [] clears.
    await caller(owner).update({ id: agent.id, mcpServerIds: [linear, glitchtip] })
    expect((await db.select().from(agents).where(eq(agents.id, agent.id)))[0]!.mcpServerIds).toEqual([linear, glitchtip])
    expect((await conversation(agent.id, owner))!.mcpServerIds).toEqual([linear, glitchtip])

    // The bug: `sessions.send {agentId, mcpServerIds}` now SETS the
    // conversation's pick; a foreign id is an error, never silently dropped.
    await sendSession({ userId: owner, input: { agentId: agent.id, text: `use linear`, mcpServerIds: [linear] } as never })
    expect((await conversation(agent.id, owner))!.mcpServerIds).toEqual([linear])
    await expect(
      sendSession({ userId: owner, input: { agentId: agent.id, text: `x`, mcpServerIds: [foreign] } as never })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect((await conversation(agent.id, owner))!.mcpServerIds).toEqual([linear])
    await sendSession({ userId: owner, input: { agentId: agent.id, text: `none`, mcpServerIds: [] } as never })
    expect((await conversation(agent.id, owner))!.mcpServerIds).toBeNull()

    // A root the agent's tree starts without a pick takes the agent's list.
    const convo = (await conversation(agent.id, owner))!
    const started = await sendSession({
      userId: owner,
      callerSessionId: convo.id,
      viaMcp: true,
      input: { teamId, deviceId, text: `go fix it` } as never,
    })
    const root = (await db.select().from(sessions).where(eq(sessions.id, started.sessionId)))[0]!
    expect(root.startedByAgentId).toBe(agent.id)
    expect(root.mcpServerIds).toEqual([linear, glitchtip])
  })
})
