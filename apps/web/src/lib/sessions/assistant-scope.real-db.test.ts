// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1720 (packages/eagent/fixtures/tree.json `treeTeams`, emcp
// access.json `assistantScope`): the member's Assistant in their hidden
// personal team works across their teams. Against the REAL migrated Postgres.

process.env.BETTER_AUTH_SECRET ||= `assistant-scope-test-secret-0123456789abcdef`

vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(
  `SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'sessions' AND column_name = 'scope_team_ids') AS ok`
)
if (!ready.ok) console.info(`[assistant-scope.real-db] skipped: ${ready.reason}`)

const { sendSession, SCOPE_ASSISTANT_ONLY_MESSAGE, TEAM_OUT_OF_SCOPE_MESSAGE, AGENT_TEAM_ONLY_MESSAGE } = await import(
  `@/lib/sessions/send`
)
const { treeTeams } = await import(`@/lib/sessions/tree-teams`)
const { watchSessions } = await import(`@/lib/sessions/bus`)
const { getMemberTeamIds, getTeamMember, getUserTeamIds } = await import(`@/lib/auth/membership`)
const { invalidateMembershipCaches } = await import(`@/lib/auth/membership-cache`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function user(): Promise<string> {
  const id = `scope-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@scope.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(owner: string, kind: `team` | `personal` = `team`): Promise<string> {
  const [row] = await db
    .insert(teams)
    .values({ name: kind === `personal` ? `Personal` : `Team`, slug: `scope-${id8()}`, kind, ownerId: kind === `personal` ? owner : null })
    .returning({ id: teams.id })
  made.teams.push(row!.id)
  if (kind === `team`) await db.insert(teamMembers).values({ teamId: row!.id, userId: owner, role: `owner` })
  return row!.id
}

async function world() {
  const me = await user()
  const a = await team(me)
  const b = await team(me)
  const other = await user()
  const foreign = await team(other)
  const personal = await team(me, `personal`)
  invalidateMembershipCaches()
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: me, deviceId, label: `mac`, kind: `desktop` })
  const [assistant] = await db
    .insert(agents)
    .values({ teamId: personal, name: `Assistant`, instructions: ``, triggers: [], deviceId, ownerId: me, role: `assistant` })
    .returning()
  const [teamAgent] = await db
    .insert(agents)
    .values({ teamId: a, name: `Sloppy`, instructions: ``, triggers: [], deviceId, ownerId: me, role: `agent` })
    .returning()
  const conv = await sendSession({ userId: me, input: { agentId: assistant!.id, text: `Hi.` } })
  return { me, a, b, foreign, personal, deviceId, assistant: assistant!, teamAgent: teamAgent!, conv: conv.sessionId }
}

describe.skipIf(!ready.ok)(`#EXP-1720 the Assistant's teams`, () => {
  it(`the personal team: synced and counted as membership, never a joined team`, async () => {
    const w = await world()
    expect(await getUserTeamIds(w.me)).toEqual(expect.arrayContaining([w.a, w.b, w.personal]))
    expect(await getMemberTeamIds(w.me)).toEqual(expect.arrayContaining([w.a, w.b]))
    expect(await getMemberTeamIds(w.me)).not.toContain(w.personal)
    expect((await getTeamMember(w.me, w.personal))?.role).toBe(`member`)
    expect(await getTeamMember(`nobody`, w.personal)).toBeUndefined()
  })

  it(`a desktop shared with one team still runs its owner's Assistant`, async () => {
    const w = await world()
    const shared = `dev-${id8()}`
    await db.insert(devices).values({ userId: w.me, deviceId: shared, label: `box`, kind: `desktop`, sharedTeamIds: [w.a] })
    await db.update(agents).set({ deviceId: shared }).where(eq(agents.id, w.assistant.id))
    await db.delete(sessions).where(eq(sessions.id, w.conv))
    const conv = await sendSession({ userId: w.me, input: { agentId: w.assistant.id, text: `Hi.` } })
    const [root] = await db.select().from(sessions).where(eq(sessions.id, conv.sessionId))
    expect(root).toMatchObject({ teamId: w.personal, deviceId: shared })
  })

  it(`its conversation lives in the personal team; nothing picked = every team of the member`, async () => {
    const w = await world()
    const [root] = await db.select().from(sessions).where(eq(sessions.id, w.conv))
    expect(root!.teamId).toBe(w.personal)
    expect((await treeTeams(w.conv)).sort()).toEqual([w.personal, w.a, w.b].sort())
  })

  it(`a turn's Teams pick becomes the root's scope, ∩ the member's teams`, async () => {
    const w = await world()
    const sent = await sendSession({
      userId: w.me,
      input: { agentId: w.assistant.id, text: `Only B.`, scopeTeamIds: [w.b, w.foreign, w.personal, w.b] },
    })
    const [root] = await db.select({ scope: sessions.scopeTeamIds }).from(sessions).where(eq(sessions.id, w.conv))
    expect(root!.scope).toEqual([w.b])
    const [message] = await db.select({ scope: messages.scopeTeamIds }).from(messages).where(eq(messages.id, sent.messageId!))
    expect(message!.scope).toEqual([w.b])
    expect(await treeTeams(w.conv)).toEqual([w.personal, w.b])
    await sendSession({ userId: w.me, input: { sessionId: w.conv, text: `All again.`, scopeTeamIds: null } })
    expect((await treeTeams(w.conv)).sort()).toEqual([w.personal, w.a, w.b].sort())
  })

  it(`a Teams pick into anything but the Assistant's conversation is refused`, async () => {
    const w = await world()
    const thread = await sendSession({ userId: w.me, input: { teamId: w.a, deviceId: w.deviceId, text: `Work.` } })
    await expect(
      sendSession({ userId: w.me, input: { sessionId: thread.sessionId, text: `Hi.`, scopeTeamIds: [w.a] } })
    ).rejects.toThrow(SCOPE_ASSISTANT_ONLY_MESSAGE)
    await expect(
      sendSession({ userId: w.me, input: { agentId: w.teamAgent.id, text: `Hi.`, scopeTeamIds: [w.a] } })
    ).rejects.toThrow(SCOPE_ASSISTANT_ONLY_MESSAGE)
  })

  it(`its children and threads start in a team of its scope, never outside it`, async () => {
    const w = await world()
    await sendSession({ userId: w.me, input: { agentId: w.assistant.id, text: `B only.`, scopeTeamIds: [w.b] } })
    const as = { userId: w.me, callerSessionId: w.conv, viaMcp: true }
    const child = await sendSession({ ...as, input: { parentId: w.conv, teamId: w.b, text: `Fix it in B.` } })
    const [row] = await db.select().from(sessions).where(eq(sessions.id, child.sessionId))
    expect(row).toMatchObject({ teamId: w.b, rootId: w.conv, parentId: w.conv })
    const thread = await sendSession({ ...as, input: { teamId: w.b, text: `A thread in B.` } })
    const [threadRow] = await db.select().from(sessions).where(eq(sessions.id, thread.sessionId))
    expect(threadRow).toMatchObject({ teamId: w.b, deviceId: w.deviceId, startedByAgentId: w.assistant.id })
    await expect(sendSession({ ...as, input: { parentId: w.conv, teamId: w.a, text: `A is out.` } })).rejects.toThrow(
      TEAM_OUT_OF_SCOPE_MESSAGE
    )
    await expect(sendSession({ ...as, input: { teamId: w.foreign, text: `Not mine.` } })).rejects.toThrow(
      TEAM_OUT_OF_SCOPE_MESSAGE
    )
  })

  it(`a team agent's tree stays in its team`, async () => {
    const w = await world()
    const conv = await sendSession({ userId: w.me, input: { agentId: w.teamAgent.id, text: `Hi.` } })
    await expect(
      sendSession({
        userId: w.me,
        callerSessionId: conv.sessionId,
        viaMcp: true,
        input: { parentId: conv.sessionId, teamId: w.b, text: `Elsewhere.` },
      })
    ).rejects.toThrow(AGENT_TEAM_ONLY_MESSAGE)
  })

  it(`it watches sessions of its scope teams only`, async () => {
    const w = await world()
    await sendSession({ userId: w.me, input: { agentId: w.assistant.id, text: `B only.`, scopeTeamIds: [w.b] } })
    const inB = await sendSession({ userId: w.me, input: { teamId: w.b, deviceId: w.deviceId, text: `B work.` } })
    const inA = await sendSession({ userId: w.me, input: { teamId: w.a, deviceId: w.deviceId, text: `A work.` } })
    const watched = await db.transaction((tx) => watchSessions(tx, w.conv, [inB.sessionId, inA.sessionId]))
    expect(watched).toEqual([inB.sessionId])
  })
})
