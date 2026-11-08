// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { LEAD_COPY } from "@exp/domain-contract/agent-settings"
import { db } from "@/db/connection"
import { agents, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1735 (agent-settings.json `lead`, eagent bus.json `leadOf`): the
// team's lead Assistant hears the asks and reports of the work another
// member's Assistant runs in the team. Against the REAL migrated Postgres.

vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(
  `SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'teams' AND column_name = 'lead_assistant_id') AS ok`
)
if (!ready.ok) console.info(`[lead.real-db] skipped: ${ready.reason}`)

const { agentsRouter } = await import(`@/lib/trpc/agents`)
const { busReport, leadOf } = await import(`@/lib/sessions/bus`)
const { inSession } = await import(`@/lib/sessions/messages`)
const { memberChatsChanged } = await import(`@/lib/agents/assistant-chat`)
const { invalidateMembershipCaches } = await import(`@/lib/auth/membership-cache`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const person = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) })

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function user(): Promise<string> {
  const id = `lead-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@lead.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const mate = await user()
  const outsider = await user()
  const [team] = await db.insert(teams).values({ name: `Lead`, slug: `lead-${id8()}` }).returning({ id: teams.id })
  made.teams.push(team!.id)
  await db.insert(teamMembers).values([
    { teamId: team!.id, userId: owner, role: `owner` },
    { teamId: team!.id, userId: mate, role: `member` },
  ])
  invalidateMembershipCaches()
  const assistant = async (userId: string) => {
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId, deviceId, label: `mac`, kind: `desktop` })
    const [row] = await db
      .insert(agents)
      .values({ teamId: team!.id, name: `Assistant`, instructions: ``, triggers: [], deviceId, ownerId: userId, role: `assistant` })
      .returning()
    return row!
  }
  const mine = await assistant(owner)
  const lead = await assistant(mate)
  return { owner, mate, outsider, teamId: team!.id, mine, lead }
}

async function session(values: Partial<typeof sessions.$inferInsert> & { teamId: string; userId: string; deviceId: string }) {
  const id = crypto.randomUUID()
  const parent = values.parentId
    ? (await db.select({ rootId: sessions.rootId, depth: sessions.depth }).from(sessions).where(eq(sessions.id, values.parentId)))[0]!
    : null
  await db.insert(sessions).values({
    id,
    rootId: parent?.rootId ?? id,
    depth: parent ? parent.depth + 1 : 0,
    status: `running`,
    shared: false,
    title: `t`,
    ...values,
  })
  return id
}

const linesFrom = (listenerId: string, childId: string) =>
  db
    .select({ body: messages.body, card: messages.card, consumedAt: messages.consumedAt })
    .from(messages)
    .where(and(inSession(listenerId), eq(messages.authorSessionId, childId)))

describe.skipIf(!ready.ok)(`#EXP-1735 the team's lead Assistant`, () => {
  it(`owners pick a member; their Assistant leads and its conversation opens; None clears it`, async () => {
    const w = await world()
    const [coordinator] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Triage`, instructions: ``, triggers: [], deviceId: w.mine.deviceId, ownerId: w.owner, role: `coordinator` })
      .returning({ id: agents.id })
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await asOwner.setLeadAssistant({ teamId: w.teamId, userId: w.mate })
    // The lead replaces the coordinator.
    expect((await db.select({ role: agents.role }).from(agents).where(eq(agents.id, coordinator!.id)))[0]!.role).toBe(`agent`)
    const [team] = await db.select({ lead: teams.leadAssistantId }).from(teams).where(eq(teams.id, w.teamId))
    expect(team!.lead).toBe(w.lead.id)
    const [conversation] = await db
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.agentId, w.lead.id), eq(sessions.userId, w.mate)))
    expect(conversation).toBeDefined()
    expect(await asOwner.lead({ teamId: w.teamId })).toEqual({ lead: { agentId: w.lead.id, userId: w.mate } })
    // Every member's list names the lead (the mate's Assistant is never listed otherwise).
    const listed = await asOwner.list({ teamId: w.teamId })
    expect(listed.agents.find((agent) => agent.id === w.lead.id)).toMatchObject({ lead: true, ownerId: w.mate })
    expect(listed.agents.find((agent) => agent.id === w.mine.id)).toMatchObject({ lead: false })
    await asOwner.setLeadAssistant({ teamId: w.teamId, userId: null })
    expect(await asOwner.lead({ teamId: w.teamId })).toEqual({ lead: null })
  })

  it(`a member and a non-member pick are refused`, async () => {
    const w = await world()
    const asMate = agentsRouter.createCaller(person(w.mate) as never)
    await expect(asMate.setLeadAssistant({ teamId: w.teamId, userId: w.mate })).rejects.toThrow(LEAD_COPY.refusedOwner)
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await expect(asOwner.setLeadAssistant({ teamId: w.teamId, userId: w.outsider })).rejects.toThrow(LEAD_COPY.refusedMember)
  })

  it(`a member's Assistant's work reports its end to the lead; private = the neutral line; the lead's own work once`, async () => {
    const w = await world()
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await asOwner.setLeadAssistant({ teamId: w.teamId, userId: w.mate })
    const [leadConv] = await db
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.agentId, w.lead.id), eq(sessions.userId, w.mate)))
    const mine = await session({ teamId: w.teamId, userId: w.owner, deviceId: w.mine.deviceId, agentId: w.mine.id, status: `idle` })
    const privateChild = await session({ teamId: w.teamId, userId: w.owner, deviceId: w.mine.deviceId, parentId: mine })
    const thread = await session({ teamId: w.teamId, userId: w.owner, deviceId: w.mine.deviceId, startedByAgentId: w.mine.id, shared: true })
    const leadsOwn = await session({ teamId: w.teamId, userId: w.mate, deviceId: w.lead.deviceId, parentId: leadConv!.id })

    expect(await leadOf(db, { id: mine, rootId: mine, teamId: w.teamId })).toBeNull()
    expect(await leadOf(db, { id: privateChild, rootId: mine, teamId: w.teamId })).toEqual({ id: w.lead.id, ownerId: w.mate })
    expect(await leadOf(db, { id: leadsOwn, rootId: leadConv!.id, teamId: w.teamId })).toBeNull()

    await db.transaction(async (tx) => {
      await busReport(tx, { id: privateChild, parentId: mine }, `Split the PR.`)
      await busReport(tx, { id: thread, parentId: null }, `Merged #7.`)
      await busReport(tx, { id: leadsOwn, parentId: leadConv!.id }, `Board picked.`)
    })
    expect((await linesFrom(mine, privateChild)).map((line) => line.body)).toEqual([`Split the PR.`])
    const neutral = await linesFrom(leadConv!.id, privateChild)
    expect(neutral).toHaveLength(1)
    expect(neutral[0]!.body).toContain(`A private chat finished.`)
    expect(neutral[0]!.consumedAt).toBeNull()
    expect((await linesFrom(leadConv!.id, thread)).map((line) => line.body)).toEqual([`Merged #7.`])
    expect((await linesFrom(leadConv!.id, leadsOwn)).map((line) => line.body)).toEqual([`Board picked.`])
  })

  it(`the lead's check-in counts the work that reports to it; a lead whose member left counts as none`, async () => {
    const w = await world()
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await asOwner.setLeadAssistant({ teamId: w.teamId, userId: w.mate })
    const since = new Date(Date.now() - 60_000)
    const thread = await session({ teamId: w.teamId, userId: w.owner, deviceId: w.mine.deviceId, startedByAgentId: w.mine.id })
    expect(await memberChatsChanged(db, w.lead, since)).toBe(false)
    await db.update(sessions).set({ lastActivityAt: new Date() }).where(eq(sessions.id, thread))
    expect(await memberChatsChanged(db, w.lead, since)).toBe(true)
    await db.delete(teamMembers).where(and(eq(teamMembers.teamId, w.teamId), eq(teamMembers.userId, w.mate)))
    invalidateMembershipCaches()
    expect(await memberChatsChanged(db, w.lead, since)).toBe(false)
    expect(await leadOf(db, { id: thread, rootId: thread, teamId: w.teamId })).toBeNull()
    expect(await asOwner.lead({ teamId: w.teamId })).toEqual({ lead: null })
  })
})
