// @vitest-environment node
// #EXP-1439 (agent-settings.json `assistant.chat`): against the REAL migrated
// Postgres. A member's Assistant conversation stays quiet: one live status
// card edited in place until the member writes, text only in a turn the
// member started, a decision pushes, routine bus lines never land, quiet
// check-ins are skipped.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => null,
    relayPostNudge: async () => {},
    relayPostKill: async () => {},
    relayPostInterrupt: async () => ({ delivered: true }),
  }
})
const pushes = vi.hoisted(() => [] as Array<{ sessionId: string; text: string }>)
vi.mock(`@/lib/integrations/notifications`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/integrations/notifications")>()
  return {
    ...real,
    fireAndForgetSessionMessageNotify: () => {},
    notifyAssistantDecision: async (sessionId: string, text: string) => {
      pushes.push({ sessionId, text })
    },
  }
})

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'uniq_agents_assistant')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[assistant-chat.real-db] skipped: ${ready.reason}`)

const { ensureAssistant } = await import(`@/lib/agents/assistant`)
const { conversationInTx } = await import(`@/lib/agents/conversation`)
const { postAgentInputInTx } = await import(`@/lib/agents/triggers`)
const { postShowCard } = await import(`@/lib/sessions/plan`)
const { insertMessage } = await import(`@/lib/sessions/messages`)
const { busLine } = await import(`@/lib/sessions/bus`)
const { memberChatsChanged } = await import(`@/lib/agents/assistant-chat`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function world() {
  const owner = `assistant-chat-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@assistant.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `Quiet`, slug: `quiet-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `desktop`, lastSeenAt: new Date() })
  const assistant = (await ensureAssistant(owner, teamId))!
  const conversation = (await db.transaction((tx) => conversationInTx(tx, assistant, owner))).session
  const host = sessionsRouter.createCaller({
    session: { user: { id: owner } },
    db,
    request: new Request(`http://x.test`),
  } as never)
  /** A check-in tick the device delivered (the turn's input). */
  const tick = () =>
    db.transaction(async (tx) => {
      await postAgentInputInTx(tx, assistant, owner, `Scheduled check-in`, { triggerId: crypto.randomUUID(), eventId: null, name: null })
      await tx
        .update(messages)
        .set({ consumedAt: new Date() })
        .where(and(inSession(conversation.id), sql`${messages.consumedAt} IS NULL`))
    })
  /** The member writes, and the device delivers it. */
  const write = (body: string) =>
    db.transaction((tx) => insertMessage(tx, conversation.id, { authorKind: `user`, authorId: owner, body, consumedAt: new Date() }))
  const surface = (text: string) => ({ kind: `surface` as const, surface: { root: { id: `t`, component: `Text`, props: { text } } } })
  const show = (text: string) => postShowCard({ userId: owner, sessionId: conversation.id, card: surface(text), replace: false })
  const rows = () => db.select().from(messages).where(inSession(conversation.id))
  return { owner, teamId, deviceId, assistant, conversation, host, tick, write, show, rows }
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

describe.skipIf(!ready.ok)(`the Assistant's quiet chat (#EXP-1439)`, () => {
  it(`keeps ONE status card, edited in place across turns until the member writes`, async () => {
    const w = await world()
    await w.write(`What's going on?`)
    const first = await w.show(`1 chat running`)
    expect(first.replaced).toBe(false)
    await w.tick()
    const second = await w.show(`2 chats running`)
    expect(second).toMatchObject({ messageId: first.messageId, replaced: true })
    await w.tick()
    expect((await w.show(`PR #12 merged`)).messageId).toBe(first.messageId)
    const cards = (await w.rows()).filter((row) => row.card?.kind === `surface`)
    expect(cards).toHaveLength(1)
    expect(JSON.stringify(cards[0]!.card)).toContain(`PR #12 merged`)
    // The member writes: the next status is a new card.
    await w.write(`Thanks`)
    expect((await w.show(`All quiet`)).messageId).not.toBe(first.messageId)
  })

  it(`posts text only in a turn the member started`, async () => {
    const w = await world()
    await w.tick()
    expect((await w.host.reply({ sessionId: w.conversation.id, body: `Nothing new.` })).messageId).toBe(``)
    await w.write(`Status?`)
    const answered = await w.host.reply({ sessionId: w.conversation.id, body: `Two chats are running.` })
    expect(answered.messageId).not.toBe(``)
    const texts = (await w.rows()).filter((row) => row.authorKind === `agent` && row.card === null)
    expect(texts.map((row) => row.body)).toEqual([`Two chats are running.`])
  })

  it(`pushes a decision to its owner`, async () => {
    const w = await world()
    pushes.length = 0
    await w.tick()
    await postShowCard({
      userId: w.owner,
      sessionId: w.conversation.id,
      card: { kind: `question`, question: `Merge PR #12?`, options: [`Merge`, `Wait`] },
      replace: false,
    })
    expect(pushes).toEqual([{ sessionId: w.conversation.id, text: `Merge PR #12?` }])
  })

  it(`never receives a routine bus line; a team agent's conversation still does`, async () => {
    const w = await world()
    const [agent] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Sloppy`, deviceId: w.deviceId, ownerId: w.owner })
      .returning()
    const team = (await db.transaction((tx) => conversationInTx(tx, agent!, w.owner))).session
    const childOf = async (parent: typeof w.conversation) => {
      const [child] = await db
        .insert(sessions)
        .values({ teamId: w.teamId, rootId: parent.id, parentId: parent.id, depth: 1, userId: w.owner, deviceId: w.deviceId })
        .returning()
      return child!
    }
    const quiet = await childOf(w.conversation)
    const loud = await childOf(team)
    for (const child of [quiet, loud]) {
      await db.transaction((tx) => busLine(tx, child, `pr_opened`, `Opened PR #1`, { waking: true }))
    }
    const lines = async (sessionId: string) =>
      (await db.select().from(messages).where(inSession(sessionId))).filter(
        (row) => row.card?.kind === `event` && (row.card as { event?: string }).event === `pr_opened`
      )
    expect(await lines(w.conversation.id)).toHaveLength(0)
    expect(await lines(quiet.id)).toHaveLength(0)
    expect(await lines(team.id)).toHaveLength(1)
  })

  it(`skips a check-in when none of the member's chats moved`, async () => {
    const w = await world()
    const before = new Date(Date.now() - 60_000)
    // Its own conversation's activity is not news.
    await db.update(sessions).set({ lastActivityAt: new Date() }).where(eq(sessions.id, w.conversation.id))
    expect(await memberChatsChanged(db, { ...w.assistant }, before)).toBe(false)
    const thread = crypto.randomUUID()
    await db.insert(sessions).values({
      id: thread,
      teamId: w.teamId,
      rootId: thread,
      userId: w.owner,
      deviceId: w.deviceId,
      lastActivityAt: new Date(),
    })
    expect(await memberChatsChanged(db, { ...w.assistant }, before)).toBe(true)
  })
})
