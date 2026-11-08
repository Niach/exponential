// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  agents,
  boards,
  devices,
  events,
  installs,
  messages,
  repositories,
  sessionKeys,
  sessions,
  teamMembers,
  teams,
} from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"
import { inSession, messageWithSession } from "@/lib/sessions/messages"

// #EXP-1409 against the REAL migrated Postgres (harness as in
// bus-private.real-db.test.ts): a PRIVATE session's `session.ended` event
// carries its ids only (the row syncs to every member over the `events`
// shape), a trigger firing quotes its title only where the conversation's
// readers may see the session (`askHidden`), a visible session keeps it
// all; moving an agent to a shared device neutralises what its private
// trees held in full; a private listener of ANOTHER member gets neutral
// bus lines. NEW-8: the 0200 scrub's replay left with
// `messages.session_id` (a one-shot migration, its SQL names the dropped
// column).

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'wake_on')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[event-private.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { privateText, askHiddenText } = await import(`@/lib/sessions/bus`)
const { hidePrivateLinesInTx } = await import(`@/lib/sessions/private-lines`)
const { TRIGGER_COPY } = await import(`@/lib/agents/triggers`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const { getSessionTree } = await import(`@/lib/sessions/tree`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { agentsRouter } = await import(`@/lib/trpc/agents`)
const { buildQuestionCard, postAskCard } = await import(`@/lib/sessions/plan`)
const { PRIVATE_PR_LEAD, PRIVATE_REPLY_LEAD, PRIVATE_REPORT_LEAD } = await import(`@exp/domain-contract/chats`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `evt-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@evt.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
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

const ctx = (userId: string) =>
  ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never

const row = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

const SECRET = `Rotate the prod key xyz-123`
const BRANCH = `exp/xyz-123-rotation`
const ENDED = [{ id: crypto.randomUUID(), enabled: true, kind: `event` as const, name: `session.ended` }]

/** A team of an owner and a member: the owner's private device, the
 *  owner's device shared with the team, the member's private device. */
async function world() {
  const owner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Evt`, slug: `evt-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `EV` })
  const ownerDev = `dev-${id8()}`
  const teamDev = `dev-${id8()}`
  const mateDev = `dev-${id8()}`
  await db.insert(devices).values([
    { userId: owner, deviceId: ownerDev, label: `laptop`, kind: `server` },
    { userId: owner, deviceId: teamDev, label: `team box`, kind: `server`, sharedTeamIds: [teamId] },
    { userId: mate, deviceId: mateDev, label: `mate box`, kind: `server` },
  ])
  const agent = async (ownerId: string, deviceId: string, triggers: typeof ENDED | [] = ENDED) =>
    (
      await db
        .insert(agents)
        .values({ teamId, name: `Agent ${id8()}`, deviceId, ownerId, triggers })
        .returning()
    )[0]!
  /** A chat with `SECRET` as its prompt (so its title) and a branch. */
  const chat = async (userId: string, deviceId: string) => {
    const id = (await sendSession({ userId, input: { teamId, deviceId, text: SECRET } })).sessionId
    await db.update(sessions).set({ branch: BRANCH, title: SECRET }).where(eq(sessions.id, id))
    return id
  }
  return { owner, mate, teamId, ownerDev, teamDev, mateDev, agent, chat }
}

const endedEvents = (sessionId: string) =>
  db
    .select()
    .from(events)
    .where(and(eq(events.name, `session.ended`), sql`${events.payload}->>'sessionId' = ${sessionId}`))

/** The trigger firings posted for `agentId` (into its owner's conversation). */
const firings = (agentId: string) =>
  db
    .select(messageWithSession())
    .from(messages)
    .where(sql`${messages.card}->>'event' = 'trigger_fired' AND ${messages.card}->>'agentId' = ${agentId}`)
    .orderBy(asc(messages.createdAt))

describe.skipIf(!ready.ok)(`#EXP-1409: a private session's session.ended event`, () => {
  it(`the team-synced row carries ids only; a firing quotes the title only where its readers may see the session`, async () => {
    const w = await world()
    const mateAgent = await w.agent(w.mate, w.mateDev)
    const sharedAgent = await w.agent(w.owner, w.teamDev)
    const privateAgent = await w.agent(w.owner, w.ownerDev)
    const chat = await w.chat(w.owner, w.ownerDev)
    expect((await row(chat)).shared).toBe(false)
    await endSession({ userId: w.owner, sessionId: chat })

    const [event] = await endedEvents(chat)
    expect(event!.payload).toEqual({ sessionId: chat, rootId: chat })
    expect(JSON.stringify(event)).not.toContain(`xyz-123`)

    // Another member's agent and a shared conversation: the ids-only row.
    for (const agent of [mateAgent, sharedAgent]) {
      const [fired] = await firings(agent.id)
      expect(fired!.body).toBe(TRIGGER_COPY.event(event!))
      expect(fired!.body).toContain(chat)
      expect(fired!.body).not.toContain(`xyz-123`)
    }
    // The owner's own private conversation may see the chat: the full payload.
    const [own] = await firings(privateAgent.id)
    expect(own!.body).toContain(SECRET)
    expect(own!.body).toContain(BRANCH)
    expect(own!.card).toMatchObject({ eventId: event!.id, name: `session.ended` })
    // The emitter prints the ids in the order a rewrite prints them (rootId
    // first, migration 0200's): a pass over a fresh firing changes nothing.
    for (const agent of [mateAgent, sharedAgent]) {
      const [fired] = await firings(agent.id)
      expect(await db.transaction((tx) => hidePrivateLinesInTx(tx, [fired!.sessionId]))).toBe(0)
    }
  })

  it(`a session visible to the whole team keeps its title and branch, in the row and every firing`, async () => {
    const w = await world()
    const mateAgent = await w.agent(w.mate, w.mateDev)
    const chat = await w.chat(w.owner, w.teamDev)
    expect((await row(chat)).shared).toBe(true)
    await endSession({ userId: w.owner, sessionId: chat })
    const [event] = await endedEvents(chat)
    expect(event!.payload).toEqual({ sessionId: chat, rootId: chat, title: SECRET, branch: BRANCH })
    const [fired] = await firings(mateAgent.id)
    expect(fired!.body).toBe(TRIGGER_COPY.event(event!))
    expect(fired!.body).toContain(BRANCH)
  })

  it(`a private child of a private chat names its ids only too`, async () => {
    const w = await world()
    const chat = await w.chat(w.owner, w.ownerDev)
    const child = (await sendSession({ userId: w.owner, input: { parentId: chat, text: SECRET } })).sessionId
    await endSession({ userId: w.owner, sessionId: chat })
    const [event] = await endedEvents(child)
    expect(event!.payload).toEqual({ sessionId: child, rootId: chat })
  })
})

describe.skipIf(!ready.ok)(`#EXP-1409: a private listener owned by a different member`, () => {
  it(`gets the neutral reply, report and ask lines (the userId side of askHidden)`, async () => {
    const w = await world()
    const chat = await w.chat(w.owner, w.ownerDev)
    const listener = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.mateDev, text: `watch` } }))
      .sessionId
    expect((await row(listener)).shared).toBe(false)
    await db.execute(sql`UPDATE sessions SET wake_on = ${JSON.stringify([chat])}::jsonb WHERE id = ${listener}`)
    const host = sessionsRouter.createCaller(ctx(w.owner))
    await host.claim({ sessionId: chat, deviceId: w.ownerDev })
    await host.reply({ sessionId: chat, body: SECRET })
    const card = buildQuestionCard({ question: SECRET, options: [{ label: `yes` }] })
    await postAskCard({ userId: w.owner, sessionId: chat, card })
    await endSession({ userId: w.owner, sessionId: chat, summary: SECRET })
    const lines = await db
      .select()
      .from(messages)
      .where(and(inSession(listener), eq(messages.authorSessionId, chat)))
    const kinds = lines.map((m) => (m.card as { kind?: string } | null)?.kind ?? null)
    expect(kinds).toEqual(expect.arrayContaining([null, `ask`, `report`]))
    expect(lines.find((m) => m.card === null)!.body).toBe(privateText({ id: chat }, PRIVATE_REPLY_LEAD))
    expect(lines.find((m) => (m.card as { kind?: string } | null)?.kind === `ask`)!.body).toBe(
      askHiddenText({ id: chat }, card)
    )
    expect(lines.find((m) => (m.card as { kind?: string } | null)?.kind === `report`)).toMatchObject({
      body: privateText({ id: chat }, PRIVATE_REPORT_LEAD),
      card: { summary: PRIVATE_REPORT_LEAD },
    })
    expect(JSON.stringify(lines)).not.toContain(`xyz-123`)
  })
})

async function openPr(teamId: string, rootSessionId: string, actorUserId: string) {
  const fullName = `o/evt-${id8()}`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId, fullName, defaultBranch: `main` })
    .returning({ id: repositories.id })
  return recordPullRequestOpened({
    teamId,
    rootSessionId,
    sessionId: rootSessionId,
    repositoryId: repo!.id,
    repoFullName: fullName,
    number: 7,
    url: `https://github.com/${fullName}/pull/7`,
    title: `Key rotation`,
    head: `exp/rotation`,
    base: `main`,
    issueIds: [],
    actorUserId,
  })
}

describe.skipIf(!ready.ok)(`#FEED-194: turn replies and team agents`, () => {
  it(`the agent that started a chat wakes on each turn; another agent that only watches it does not`, async () => {
    const w = await world()
    const starter = await w.agent(w.owner, w.ownerDev)
    const other = await w.agent(w.owner, w.ownerDev)
    const conversationOf = async (agentId: string) =>
      (await sendSession({ userId: w.owner, input: { agentId, text: `hello` } })).sessionId
    const starting = await conversationOf(starter.id)
    const watching = await conversationOf(other.id)
    const chat = (
      await sendSession({
        userId: w.owner,
        callerSessionId: starting,
        viaMcp: true,
        input: { teamId: w.teamId, deviceId: w.ownerDev, text: `Make the PDF` },
      })
    ).sessionId
    expect((await row(chat)).startedByAgentId).toBe(starter.id)
    await sendSession({ userId: w.owner, callerSessionId: watching, viaMcp: true, input: { wakeOn: [chat] } })
    expect((await row(watching)).wakeOn).toContain(chat)

    const host = sessionsRouter.createCaller(ctx(w.owner))
    await host.claim({ sessionId: chat, deviceId: w.ownerDev })
    await host.reply({ sessionId: chat, body: `The PDF is done.` })

    const turnsIn = (sessionId: string) =>
      db
        .select()
        .from(messages)
        .where(and(eq(messages.rootId, sessionId), eq(messages.authorSessionId, chat), sql`${messages.card} IS NULL`))
    expect(await turnsIn(starting)).toMatchObject([{ authorKind: `agent`, body: `The PDF is done.`, consumedAt: null }])
    expect(await turnsIn(watching)).toEqual([])
  })
})

describe.skipIf(!ready.ok)(`#EXP-1409: moving an agent from a private device to a shared one`, () => {
  it(`its conversations' private lines go neutral inside the move; the owner's own words stay`, async () => {
    const w = await world()
    const agent = await w.agent(w.owner, w.ownerDev)
    const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent.id, text: `watch it` } }))
      .sessionId
    const chat = (
      await sendSession({
        userId: w.owner,
        callerSessionId: conversation,
        viaMcp: true,
        input: { teamId: w.teamId, deviceId: w.ownerDev, text: SECRET },
      })
    ).sessionId
    await db.update(sessions).set({ branch: BRANCH }).where(eq(sessions.id, chat))
    expect((await row(conversation)).shared).toBe(false)
    const host = sessionsRouter.createCaller(ctx(w.owner))
    await host.claim({ sessionId: chat, deviceId: w.ownerDev })
    await host.reply({ sessionId: chat, body: SECRET })
    await postAskCard({ userId: w.owner, sessionId: chat, card: buildQuestionCard({ question: SECRET, options: [{ label: `yes` }] }) })
    await openPr(w.teamId, chat, w.owner)
    await endSession({ userId: w.owner, sessionId: chat, summary: SECRET })
    const inConversation = () =>
      db.select().from(messages).where(eq(messages.rootId, conversation)).orderBy(asc(messages.createdAt))
    // Same member, private conversation: everything arrived in full.
    const before = JSON.stringify(await inConversation())
    expect(before).toContain(`xyz-123`)
    expect(before).toContain(`pullRequestId`)

    await agentsRouter.createCaller(ctx(w.owner)).update({ id: agent.id, deviceId: w.teamDev })
    expect((await row(conversation)).shared).toBe(true)
    const after = await inConversation()
    expect(JSON.stringify(after)).not.toContain(`xyz-123`)
    const from = after.filter((m) => m.authorSessionId === chat)
    // #FEED-194: the agent that started the chat heard its turn; the move
    // made that line neutral too.
    expect(from.find((m) => m.card === null && m.authorKind === `agent`)!.body).toBe(
      privateText({ id: chat }, PRIVATE_REPLY_LEAD)
    )
    expect(from.find((m) => (m.card as { kind?: string } | null)?.kind === `report`)!.body).toBe(
      privateText({ id: chat }, PRIVATE_REPORT_LEAD)
    )
    const prLine = from.find((m) => (m.card as { event?: string } | null)?.event === `pr_opened`)!
    expect(prLine.body).toBe(privateText({ id: chat }, PRIVATE_PR_LEAD))
    expect(prLine.card).not.toHaveProperty(`pullRequestId`)
    const [event] = await endedEvents(chat)
    expect(after.find((m) => (m.card as { event?: string } | null)?.event === `trigger_fired`)!.body).toBe(
      TRIGGER_COPY.event(event!)
    )
    expect(after.find((m) => m.body === `watch it`)).toBeTruthy()
    // A teammate reading the now-shared conversation over MCP.
    expect(JSON.stringify(await getSessionTree(w.mate, conversation, { messages: 100 }))).not.toContain(`xyz-123`)
    // Idempotent, and moving back changes nothing more.
    await inRollback(async (tx) => expect(await hidePrivateLinesInTx(tx, [conversation])).toBe(0))
  })

  it(`#EXP-1410: a firing older than the event retention is still scrubbed, its ids read from its own body`, async () => {
    const w = await world()
    const agent = await w.agent(w.owner, w.ownerDev)
    const chat = await w.chat(w.owner, w.ownerDev)
    await endSession({ userId: w.owner, sessionId: chat })
    const [event] = await endedEvents(chat)
    const [fired] = await firings(agent.id)
    expect(fired!.body).toContain(`xyz-123`)
    // The retention pruned the row; the firing stays.
    await db.delete(events).where(eq(events.id, event!.id))

    await agentsRouter.createCaller(ctx(w.owner)).update({ id: agent.id, deviceId: w.teamDev })

    const [after] = await firings(agent.id)
    expect(after!.body).toBe(TRIGGER_COPY.event(event!))
    expect(after!.body).not.toContain(`xyz-123`)
    expect(after!.card).toEqual(fired!.card)
    await inRollback(async (tx) => expect(await hidePrivateLinesInTx(tx, [after!.sessionId])).toBe(0))
  })

  it(`#EXP-1410: the owner's explicit send into the private conversation stays in full after the move`, async () => {
    const w = await world()
    const agent = await w.agent(w.owner, w.ownerDev)
    const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent.id, text: `watch it` } }))
      .sessionId
    const chat = (
      await sendSession({
        userId: w.owner,
        callerSessionId: conversation,
        viaMcp: true,
        input: { teamId: w.teamId, deviceId: w.ownerDev, text: `go` },
      })
    ).sessionId
    await sendSession({ userId: w.owner, callerSessionId: chat, viaMcp: true, input: { sessionId: conversation, text: SECRET } })

    await agentsRouter.createCaller(ctx(w.owner)).update({ id: agent.id, deviceId: w.teamDev })

    const kept = await db.select().from(messages).where(eq(messages.rootId, conversation))
    expect(kept.some((m) => m.body === SECRET && m.authorSessionId === chat)).toBe(true)
  })

  it(`#FEED-177: a move from a shared device to a private one drops the teammates' tree-key wraps`, async () => {
    const w = await world()
    const agent = await w.agent(w.owner, w.teamDev, [])
    const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent.id, text: `hi` } })).sessionId
    expect((await row(conversation)).shared).toBe(true)
    const install = async (userId: string) =>
      (
        await db
          .insert(installs)
          .values({ userId, kind: `desktop`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: `x`.repeat(43) })
          .returning({ id: installs.id })
      )[0]!.id
    const ownerInstall = await install(w.owner)
    const mateInstall = await install(w.mate)
    await db.update(sessions).set({ sealEpoch: 1, sealPublicKey: `k`.repeat(43) }).where(eq(sessions.id, conversation))
    await db
      .insert(sessionKeys)
      .values([ownerInstall, mateInstall].map((installId) => ({ rootId: conversation, epoch: 1, installId, wrapped: `w` })))
    await agentsRouter.createCaller(ctx(w.owner)).update({ id: agent.id, deviceId: w.ownerDev })
    expect((await row(conversation)).shared).toBe(false)
    expect((await db.select().from(sessionKeys).where(eq(sessionKeys.rootId, conversation))).map((k) => k.installId)).toEqual([
      ownerInstall,
    ])
  })

  it(`a move between private devices of the owner rewrites nothing`, async () => {
    const w = await world()
    const otherDev = `dev-${id8()}`
    await db.insert(devices).values({ userId: w.owner, deviceId: otherDev, label: `desk`, kind: `server` })
    const agent = await w.agent(w.owner, w.ownerDev, [])
    const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent.id, text: `watch it` } }))
      .sessionId
    const chat = (
      await sendSession({
        userId: w.owner,
        callerSessionId: conversation,
        viaMcp: true,
        input: { teamId: w.teamId, deviceId: w.ownerDev, text: SECRET },
      })
    ).sessionId
    await endSession({ userId: w.owner, sessionId: chat, summary: SECRET })
    await agentsRouter.createCaller(ctx(w.owner)).update({ id: agent.id, deviceId: otherDev })
    expect((await row(conversation)).shared).toBe(false)
    const report = (await db.select().from(messages).where(inSession(conversation))).find(
      (m) => (m.card as { kind?: string } | null)?.kind === `report`
    )
    expect(report).toMatchObject({ body: SECRET })
  })
})
