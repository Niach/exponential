// @vitest-environment node
// #EXP-1383 (agent-settings.json `actor` + `assignee`, chats.json
// `agentHint`): against the REAL migrated Postgres. Agents act as
// themselves (issue events, comments, created issues carry the agent of the
// caller's own agent-tree session), issues are assigned to agents (the
// exclusive column, the event payload, `issue.assigned`, the nudge into the
// assigner's conversation, no self-nudge), the instructions stamps, and the
// team boundary of an agent tree's sends.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  agents,
  boards,
  comments,
  devices,
  events,
  issueEvents,
  issues,
  messages,
  sessions,
  teamMembers,
  teams,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

const relay = vi.hoisted(() => ({ nudges: [] as string[] }))
vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => ({ url: `http://relay.test`, secret: `s` }),
    relayPostNudge: async (_config: unknown, userId: string, deviceId: string) => {
      relay.nudges.push(`${userId}@${deviceId}`)
    },
    relayPostKill: async () => {},
    relayPostInterrupt: async () => ({ delivered: true }),
  }
})
const notify = vi.hoisted(() => ({ calls: [] as Array<{ fn: string; args: Record<string, unknown> }> }))
vi.mock(`@/lib/integrations/notifications`, () => {
  const record = (fn: string) => (args: Record<string, unknown>) => {
    notify.calls.push({ fn, args })
  }
  return {
    fireAndForgetSessionMessageNotify: () => {},
    notifySessionBlocked: async () => {},
    fireAndForgetAssignmentNotify: record(`assignment`),
    fireAndForgetIssueMentionNotify: record(`mention`),
    fireAndForgetCommentNotify: record(`comment`),
    fireAndForgetStatusChangeNotify: record(`status`),
    fireAndForgetReporterResolution: () => {},
    fireAndForgetPrNotify: () => {},
  }
})
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'issues' AND column_name = 'assignee_agent_id')
    AND EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'agent_instructions_at')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[agent-actor.real-db] skipped: ${ready.reason}`)

const { issuesRouter } = await import(`@/lib/trpc/issues`)
const { commentsRouter } = await import(`@/lib/trpc/comments`)
const { agentsRouter } = await import(`@/lib/trpc/agents`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { conversationInTx } = await import(`@/lib/agents/conversation`)
const { sendSession, AGENT_TEAM_ONLY_MESSAGE } = await import(`@/lib/sessions/send`)
const { actingSessionForRequest, confineMcpAccessToActingSession } = await import(`@/lib/sessions/acting`)
const { FULL_ACCESS, isTeamVisible } = await import(`@/lib/emcp/scope`)
const { assigneeNudgeLine } = await import(`@/lib/agents/assignment`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `actor-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@actor.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const mate = await user()
  const [team] = await db.insert(teams).values({ name: `Actors`, slug: `actor-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  const [board] = await db
    .insert(boards)
    .values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `AC` })
    .returning({ id: boards.id })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const agent = async (name = `Sloppy`) =>
    (await db.insert(agents).values({ teamId, name, deviceId, ownerId: owner }).returning())[0]!
  const issue = async (title = `An issue`) =>
    (
      await db
        .insert(issues)
        .values({ boardId: board!.id, teamId, title, creatorId: owner })
        .returning({ id: issues.id, identifier: issues.identifier })
    )[0]!
  /** The owner's conversation with the agent (an agent-tree root). */
  const conversation = async (agentRow: typeof agents.$inferSelect, userId = owner) =>
    (await db.transaction((tx) => conversationInTx(tx, agentRow, userId))).session
  return { owner, mate, teamId, boardId: board!.id, deviceId, agent, issue, conversation }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

/** A person's app request. */
const person = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) })
/** An MCP call speaking for `sessionId` (the launcher's header). */
const mcp = (userId: string, sessionId: string) => ({
  session: { user: { id: userId } },
  db,
  viaMcp: true as const,
  request: new Request(`http://x.test/api/mcp`, { headers: { "x-exp-session-id": sessionId } }),
})

const issueRow = async (id: string) => (await db.select().from(issues).where(eq(issues.id, id)).limit(1))[0]!
const assigneeEvents = async (issueId: string) =>
  db
    .select()
    .from(issueEvents)
    .where(and(eq(issueEvents.issueId, issueId), eq(issueEvents.type, `assignee_changed`)))
const assignedEvents = async (teamId: string) =>
  db.select().from(events).where(and(eq(events.teamId, teamId), eq(events.name, `issue.assigned`)))
const nudges = async (sessionId: string) =>
  db
    .select({ body: messages.body, card: messages.card, authorId: messages.authorId })
    .from(messages)
    .where(and(inSession(sessionId), sql`${messages.card}->>'name' = 'issue.assigned'`))
const conversationOf = async (agentId: string, userId: string) =>
  (await db.select().from(sessions).where(and(eq(sessions.agentId, agentId), eq(sessions.userId, userId))))[0]

describe.skipIf(!ready.ok)(`assign issues to agents (#EXP-1383)`, () => {
  it(`assigns an agent exclusively, records from/to + agent ids, emits issue.assigned and nudges the assigner's conversation`, async () => {
    const w = await world()
    const sloppy = await w.agent()
    const target = await w.issue(`Fix the login`)
    // A person first, then the agent: the agent clears the person.
    await issuesRouter.createCaller(person(w.owner) as never).update({ id: target.id, assigneeId: w.mate })
    relay.nudges.length = 0
    await issuesRouter.createCaller(person(w.owner) as never).update({ id: target.id, assigneeAgentId: sloppy.id })
    let row = await issueRow(target.id)
    expect(row).toMatchObject({ assigneeId: null, assigneeAgentId: sloppy.id })
    const changes = await assigneeEvents(target.id)
    expect(changes.map((e) => e.payload)).toEqual(
      expect.arrayContaining([{ from: w.mate, to: null, fromAgentId: null, toAgentId: sloppy.id }])
    )
    const emitted = await assignedEvents(w.teamId)
    expect(emitted.map((e) => e.payload)).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ issueId: target.id, identifier: target.identifier, assigneeId: null, assigneeAgentId: sloppy.id }),
        expect.objectContaining({ issueId: target.id, assigneeId: w.mate, assigneeAgentId: null }),
      ])
    )
    // The nudge: the owner's conversation (created), authored by the owner.
    const convo = (await conversationOf(sloppy.id, w.owner))!
    expect(convo).toMatchObject({ status: `queued`, agentId: sloppy.id })
    const posted = await nudges(convo.id)
    expect(posted).toHaveLength(1)
    expect(posted[0]).toMatchObject({
      authorId: w.owner,
      body: assigneeNudgeLine({ identifier: target.identifier, title: `Fix the login` }),
      card: { kind: `event`, event: `trigger_fired`, agentId: sloppy.id, triggerId: null, name: `issue.assigned`, issueId: target.id },
    })
    expect(posted[0]!.body).toBe(`${target.identifier} was assigned to you: Fix the login`)
    expect(relay.nudges).toEqual([`${w.owner}@${w.deviceId}`])

    // Back to a person: the agent column clears in the same write.
    await issuesRouter.createCaller(person(w.owner) as never).update({ id: target.id, assigneeId: w.owner })
    row = await issueRow(target.id)
    expect(row).toMatchObject({ assigneeId: w.owner, assigneeAgentId: null })
    expect((await assigneeEvents(target.id)).map((e) => e.payload)).toEqual(
      expect.arrayContaining([{ from: null, to: w.owner, fromAgentId: sloppy.id, toAgentId: null }])
    )

    // An old client (no agent assignees) picks Unassigned: `assigneeId: null`
    // alone clears the agent too.
    await issuesRouter.createCaller(person(w.owner) as never).update({ id: target.id, assigneeAgentId: sloppy.id })
    await issuesRouter.createCaller(person(w.owner) as never).update({ id: target.id, assigneeId: null })
    row = await issueRow(target.id)
    expect(row).toMatchObject({ assigneeId: null, assigneeAgentId: null })
  })

  it(`refuses another team's agent and both assignees at once; the DB check holds`, async () => {
    const w = await world()
    const other = await world()
    const foreign = await other.agent(`Foreign`)
    const target = await w.issue()
    await expect(
      issuesRouter.createCaller(person(w.owner) as never).update({ id: target.id, assigneeAgentId: foreign.id })
    ).rejects.toThrow(/issue's team/)
    const mine = await w.agent()
    await expect(
      issuesRouter.createCaller(person(w.owner) as never).update({ id: target.id, assigneeId: w.owner, assigneeAgentId: mine.id })
    ).rejects.toThrow(/not both/)
    await expect(
      db.update(issues).set({ assigneeId: w.owner, assigneeAgentId: mine.id }).where(eq(issues.id, target.id))
    ).rejects.toThrow()
  })

  it(`creates with an agent assignee (also from a draft), skipping the solo default, and an agent session never nudges itself`, async () => {
    const w = await world()
    const sloppy = await w.agent()
    const created = await issuesRouter
      .createCaller(person(w.mate) as never)
      .create({ boardId: w.boardId, title: `Filed for the agent`, assigneeAgentId: sloppy.id })
    expect(created.issue).toMatchObject({ assigneeId: null, assigneeAgentId: sloppy.id, creatorAgentId: null })
    // The mate's own conversation cannot run (the device is the owner's,
    // unshared): the issue stands, no nudge, the event still lands.
    expect(await conversationOf(sloppy.id, w.mate)).toBeUndefined()
    expect((await assignedEvents(w.teamId)).map((e) => e.payload)).toEqual([
      expect.objectContaining({ issueId: created.issue.id, assigneeAgentId: sloppy.id }),
    ])

    // The agent's own session assigns an issue to itself: stamped as the
    // agent, no nudge into its own conversation.
    const convo = await w.conversation(sloppy)
    const target = await w.issue(`Self-assigned`)
    await issuesRouter.createCaller(mcp(w.owner, convo.id) as never).update({ id: target.id, assigneeAgentId: sloppy.id })
    expect(await nudges(convo.id)).toHaveLength(0)
    const [change] = await assigneeEvents(target.id)
    expect(change).toMatchObject({ actorUserId: w.owner, actorAgentId: sloppy.id, viaMcp: true, sessionId: convo.id })

    // Another agent's session assigns to Sloppy: nudges as its member.
    const helper = await w.agent(`Helper`)
    const helperConvo = await w.conversation(helper)
    const third = await w.issue(`Handed over`)
    await issuesRouter
      .createCaller(mcp(w.owner, helperConvo.id) as never)
      .update({ id: third.id, assigneeAgentId: sloppy.id })
    expect(await nudges(convo.id)).toHaveLength(1)
  })
})

describe.skipIf(!ready.ok)(`agents act as themselves (#EXP-1383)`, () => {
  it(`stamps the acting agent on events, comments and created issues, and names it in notifications`, async () => {
    const w = await world()
    const sloppy = await w.agent()
    const convo = await w.conversation(sloppy)
    const ctx = mcp(w.owner, convo.id)
    const created = await issuesRouter.createCaller(ctx as never).create({ boardId: w.boardId, title: `By the agent` })
    expect(created.issue).toMatchObject({ creatorId: w.owner, creatorAgentId: sloppy.id })
    const [createdEvent] = await db.select().from(issueEvents).where(eq(issueEvents.issueId, created.issue.id))
    expect(createdEvent).toMatchObject({ type: `created`, actorUserId: w.owner, actorAgentId: sloppy.id })

    notify.calls.length = 0
    const comment = await commentsRouter.createCaller(ctx as never).create({ issueId: created.issue.id, body: `On it` })
    const [stored] = await db.select().from(comments).where(eq(comments.id, comment.comment.id))
    expect(stored).toMatchObject({ authorId: w.owner, authorAgentId: sloppy.id, source: `mcp` })
    expect(notify.calls.find((c) => c.fn === `comment`)?.args).toMatchObject({ actorAgentId: sloppy.id })

    await issuesRouter.createCaller(ctx as never).update({ id: created.issue.id, status: `in_progress` })
    expect(notify.calls.find((c) => c.fn === `status`)?.args).toMatchObject({ actorAgentId: sloppy.id })

    // A person's request (no MCP) and a session that is NOT the caller's
    // act as nobody.
    const plain = await commentsRouter.createCaller(person(w.owner) as never).create({ issueId: created.issue.id, body: `Me` })
    expect((await db.select().from(comments).where(eq(comments.id, plain.comment.id)))[0]!.authorAgentId).toBeNull()
    const spoofed = await commentsRouter
      .createCaller(mcp(w.mate, convo.id) as never)
      .create({ issueId: created.issue.id, body: `Not me` })
    expect((await db.select().from(comments).where(eq(comments.id, spoofed.comment.id)))[0]!.authorAgentId).toBeNull()
  })

  it(`resolves the acting session once per request and only for its principal`, async () => {
    const w = await world()
    const sloppy = await w.agent()
    const convo = await w.conversation(sloppy)
    const request = new Request(`http://x.test`)
    const first = actingSessionForRequest(request, w.owner, convo.id)
    expect(actingSessionForRequest(request, w.owner, convo.id)).toBe(first)
    expect(await first).toMatchObject({ id: convo.id, teamId: w.teamId, rootAgentId: sloppy.id, userId: w.owner })
    expect(await actingSessionForRequest(request, w.mate, convo.id)).toBeNull()
  })

  it(`confines /api/mcp to the team of a caller's agent-tree session, nothing else`, async () => {
    const w = await world()
    const sloppy = await w.agent()
    const convo = await w.conversation(sloppy)
    const clamped = await confineMcpAccessToActingSession(FULL_ACCESS, new Request(`http://x.test`), w.owner, convo.id)
    expect(clamped).toMatchObject({ full: false, origin: `agent` })
    expect(isTeamVisible(clamped, w.teamId)).toBe(true)
    expect(isTeamVisible(clamped, crypto.randomUUID())).toBe(false)
    // A sub-session of the agent's tree is confined the same way.
    const child = await sendSession({
      userId: w.owner,
      callerSessionId: convo.id,
      viaMcp: true,
      input: { parentId: convo.id, text: `Sub-task` } as never,
    })
    expect(
      (await confineMcpAccessToActingSession(FULL_ACCESS, new Request(`http://x.test`), w.owner, child.sessionId)).full
    ).toBe(false)
    // Somebody else's agent session (the header grants nothing) and no
    // session keep the credential's access.
    expect(await confineMcpAccessToActingSession(FULL_ACCESS, new Request(`http://x.test`), w.mate, convo.id)).toBe(FULL_ACCESS)
    expect(await confineMcpAccessToActingSession(FULL_ACCESS, new Request(`http://x.test`), w.owner, null)).toBe(FULL_ACCESS)
    // A normal thread keeps full access.
    const thread = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `Thread` } as never })
    expect(await confineMcpAccessToActingSession(FULL_ACCESS, new Request(`http://x.test`), w.owner, thread.sessionId)).toBe(FULL_ACCESS)
  })
})

describe.skipIf(!ready.ok)(`instructions stamps (#EXP-1383 agentHint)`, () => {
  it(`create stamps, only a changed text re-stamps, and the brief stamps the fetching session`, async () => {
    const w = await world()
    const owner = agentsRouter.createCaller(person(w.owner) as never)
    const { agent } = await owner.create({ teamId: w.teamId, name: `Writer`, deviceId: w.deviceId, instructions: `Be brief.`, triggers: [] })
    const born = agent.instructionsUpdatedAt!
    expect(born).toBeInstanceOf(Date)
    const renamed = (await owner.update({ id: agent.id, name: `Writer 2`, instructions: `Be brief.` })).agent
    expect(renamed.instructionsUpdatedAt!.getTime()).toBe(born.getTime())
    await new Promise((resolve) => setTimeout(resolve, 5))
    const edited = (await owner.update({ id: agent.id, instructions: `Be thorough.` })).agent
    expect(edited.instructionsUpdatedAt!.getTime()).toBeGreaterThan(born.getTime())

    const row = (await db.select().from(agents).where(eq(agents.id, agent.id)))[0]!
    const convo = await w.conversation(row)
    expect(convo.agentInstructionsAt).toBeNull()
    await new Promise((resolve) => setTimeout(resolve, 5))
    const brief = await sessionsRouter.createCaller(person(w.owner) as never).brief({ sessionId: convo.id })
    expect(brief.agentInstructions).toBe(`Be thorough.`)
    expect(brief.rootAgentId).toBe(agent.id)
    const stamped = (await db.select().from(sessions).where(eq(sessions.id, convo.id)))[0]!
    expect(stamped.agentInstructionsAt!.getTime()).toBeGreaterThanOrEqual(edited.instructionsUpdatedAt!.getTime())
    // A launch is no edit: the chat list (sorted by updated_at) keeps its order.
    expect(stamped.updatedAt.getTime()).toBe(convo.updatedAt.getTime())
  })
})

describe.skipIf(!ready.ok)(`an agent tree stays in its team (#EXP-1383)`, () => {
  it(`refuses a send or start into another team from an agent session, allows its own team`, async () => {
    const w = await world()
    const other = await world()
    // The owner is a member of the other team too (a real cross-team reach).
    await db.insert(teamMembers).values({ teamId: other.teamId, userId: w.owner, role: `member` })
    const sloppy = await w.agent()
    const convo = await w.conversation(sloppy)
    await expect(
      sendSession({
        userId: w.owner,
        callerSessionId: convo.id,
        viaMcp: true,
        input: { teamId: other.teamId, deviceId: w.deviceId, text: `Leak` } as never,
      })
    ).rejects.toThrow(AGENT_TEAM_ONLY_MESSAGE)
    const otherAgent = await other.agent(`Theirs`)
    await expect(
      sendSession({ userId: w.owner, callerSessionId: convo.id, viaMcp: true, input: { agentId: otherAgent.id, text: `Hi` } as never })
    ).rejects.toThrow(AGENT_TEAM_ONLY_MESSAGE)
    // Its own team: a fresh thread starts.
    const started = await sendSession({
      userId: w.owner,
      callerSessionId: convo.id,
      viaMcp: true,
      input: { text: `Do the thing`, deviceId: w.deviceId } as never,
    })
    expect(started.sessionId).toBeTruthy()
    // Without the agent session the member reaches the other team as before.
    await expect(
      sendSession({ userId: w.owner, input: { teamId: other.teamId, deviceId: w.deviceId, text: `Fine` } as never })
    ).resolves.toMatchObject({ sessionId: expect.any(String) })
  })
})

describe.skipIf(!ready.ok)(`a thread an agent starts is the agent's tree (#EXP-1383)`, () => {
  it(`stamps started_by_agent_id, confines it, acts as the agent and briefs rootAgentId`, async () => {
    const w = await world()
    const other = await world()
    await db.insert(teamMembers).values({ teamId: other.teamId, userId: w.owner, role: `member` })
    const sloppy = await w.agent()
    const convo = await w.conversation(sloppy)
    const started = await sendSession({
      userId: w.owner,
      callerSessionId: convo.id,
      viaMcp: true,
      input: { text: `Spawned work`, deviceId: w.deviceId } as never,
    })
    const thread = (await db.select().from(sessions).where(eq(sessions.id, started.sessionId)))[0]!
    expect(thread).toMatchObject({ agentId: null, startedByAgentId: sloppy.id, parentId: null })

    // The brief names the agent: the device gives the tree the team key.
    const brief = await sessionsRouter.createCaller(person(w.owner) as never).brief({ sessionId: thread.id })
    expect(brief.rootAgentId).toBe(sloppy.id)

    // /api/mcp is clamped to the team, for the thread and its children.
    const clamped = await confineMcpAccessToActingSession(FULL_ACCESS, new Request(`http://x.test`), w.owner, thread.id)
    expect(clamped).toMatchObject({ full: false, origin: `agent` })
    expect(isTeamVisible(clamped, other.teamId)).toBe(false)
    const child = await sendSession({
      userId: w.owner,
      callerSessionId: thread.id,
      viaMcp: true,
      input: { parentId: thread.id, text: `Sub-task` } as never,
    })
    expect(
      (await confineMcpAccessToActingSession(FULL_ACCESS, new Request(`http://x.test`), w.owner, child.sessionId)).full
    ).toBe(false)

    // Its sends stay in the team, and a root it starts inherits the agent.
    await expect(
      sendSession({
        userId: w.owner,
        callerSessionId: thread.id,
        viaMcp: true,
        input: { teamId: other.teamId, deviceId: w.deviceId, text: `Leak` } as never,
      })
    ).rejects.toThrow(AGENT_TEAM_ONLY_MESSAGE)
    const grand = await sendSession({
      userId: w.owner,
      callerSessionId: child.sessionId,
      viaMcp: true,
      input: { text: `Again`, deviceId: w.deviceId } as never,
    })
    expect((await db.select().from(sessions).where(eq(sessions.id, grand.sessionId)))[0]!.startedByAgentId).toBe(sloppy.id)

    // Writes through it are stamped as the agent; assigning to that agent
    // from inside its own tree nudges nobody.
    const ctx = mcp(w.owner, child.sessionId)
    const created = await issuesRouter.createCaller(ctx as never).create({ boardId: w.boardId, title: `From the thread` })
    expect(created.issue.creatorAgentId).toBe(sloppy.id)
    const comment = await commentsRouter.createCaller(ctx as never).create({ issueId: created.issue.id, body: `Done` })
    expect((await db.select().from(comments).where(eq(comments.id, comment.comment.id)))[0]!.authorAgentId).toBe(sloppy.id)
    await issuesRouter.createCaller(ctx as never).update({ id: created.issue.id, assigneeAgentId: sloppy.id })
    expect(await nudges(convo.id)).toHaveLength(0)
    const [change] = await assigneeEvents(created.issue.id)
    expect(change).toMatchObject({ actorAgentId: sloppy.id, sessionId: child.sessionId })

    // A person's own thread is no agent tree.
    const plain = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `Mine` } as never })
    expect((await db.select().from(sessions).where(eq(sessions.id, plain.sessionId)))[0]!.startedByAgentId).toBeNull()
    expect((await sessionsRouter.createCaller(person(w.owner) as never).brief({ sessionId: plain.sessionId })).rootAgentId).toBeNull()
  })
})
