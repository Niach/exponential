// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { digestMessages, type ConversationDigest } from "@exp/domain-contract/chats"
import { needsYou, type NeedsYouMessageInput } from "@exp/domain-contract/needs-you"
import { db } from "@/db/connection"
import { agents, devices, messages, sessions, teamMembers, teams, userVoiceSettings } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { McpUser } from "@/lib/emcp/server"

// #FEED-298 (needs-you.json DISMISS, waiting-cards.json MEMBER DISMISS), the
// reported case end to end against the REAL migrated Postgres: a 25-day-old
// question in the member's own `assistant` conversation, which is persistent
// (no `end`, so FEED-299's expiry never reaches it), under hourly schedule
// ticks, as a voice conversation and as a plain one. The Needs you list the
// clients draw (the SQL digest through `needsYou()`) holds it; the row's
// Dismiss and an MCP `sessions_send` outside any session each clear it.

process.env.BETTER_AUTH_SECRET ||= `needs-you-dismiss-secret-0123456789abcdef`

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
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(
  `SELECT (EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'sessions' AND column_name = 'digest')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'conversation_digest')) AS ok`
)
if (!ready.ok) console.info(`[needs-you-dismiss.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { postAgentInputInTx } = await import(`@/lib/agents/triggers`)
const { FULL_ACCESS } = await import(`@/lib/emcp/scope`)
const { registerNounTools } = await import(`@/lib/emcp/nouns`)
const { invalidateMembershipCaches } = await import(`@/lib/auth/membership-cache`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const DAY = 24 * 60 * 60_000
const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function user(): Promise<McpUser> {
  const id = `ny-dismiss-${id8()}`
  const email = `${id}@needs.test`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${email}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

const trpc = (userId: string) =>
  sessionsRouter.createCaller({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) } as never)

/** The member's own MCP key, no session header: an MCP client of theirs. */
function mcp(u: McpUser) {
  const handlers = new Map<string, (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>>()
  const server = {
    registerTool: (name: string, _def: unknown, handler: (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>) => {
      handlers.set(name, handler)
    },
  }
  registerNounTools(server as never, u, new Request(`https://x.test/api/mcp`), FULL_ACCESS, null)
  return async (name: string, args: Record<string, unknown>): Promise<ToolResult> => {
    try {
      return await handlers.get(`exponential_${name}`)!(args, {})
    } catch (e) {
      return { isError: true, content: [{ type: `text`, text: e instanceof Error ? e.message : String(e) }] }
    }
  }
}

const text = (result: ToolResult) => result.content.map((part) => part.text ?? ``).join(``)
const row = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)))[0]!

/** The member's Needs you rows, as the clients fold them from the digest. */
async function rows(me: string, sessionId: string) {
  const session = await row(sessionId)
  return needsYou({
    now: Date.now(),
    me,
    sessions: [
      { id: session.id, rootId: session.rootId, name: `Assistant`, status: session.status, agentId: session.agentId, userId: session.userId, busy: session.busy },
    ],
    messages: digestMessages(session.id, session.digest as ConversationDigest) as unknown as NeedsYouMessageInput[],
    pullRequests: [],
    mergeMode: `auto`,
  })
}

/** The stale card: posted by the Assistant, 25 days old, with hourly ticks
 *  and the conversation's newer messages after it. */
async function staleCard(voice: boolean) {
  const me = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Personal`, slug: `ny-${id8()}`, kind: `personal`, ownerId: me.id })
    .returning({ id: teams.id })
  made.teams.push(team!.id)
  const [shared] = await db.insert(teams).values({ name: `Team`, slug: `ny-${id8()}` }).returning({ id: teams.id })
  made.teams.push(shared!.id)
  await db.insert(teamMembers).values([
    { teamId: shared!.id, userId: me.id, role: `owner` },
    { teamId: shared!.id, userId: mate.id, role: `member` },
  ])
  invalidateMembershipCaches()
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: me.id, deviceId, label: `mac`, kind: `desktop` })
  const [assistant] = await db
    .insert(agents)
    .values({ teamId: team!.id, name: `Assistant`, instructions: ``, triggers: [], deviceId, ownerId: me.id, role: `assistant` })
    .returning()
  const { sessionId } = await sendSession({ userId: me.id, input: { agentId: assistant!.id, text: `Hi.` } })
  const host = trpc(me.id)
  await host.claim({ sessionId, deviceId })
  if (voice) await db.insert(userVoiceSettings).values({ userId: me.id, agentId: assistant!.id })
  const { messageId } = await host.reply({
    sessionId,
    body: ``,
    card: { kind: `question`, question: `Message a session 6e223?`, options: [`Yes`, `No`] },
  })
  await db.execute(sql`
    UPDATE messages SET created_at = now() - interval '25 days', updated_at = now() - interval '25 days'
    WHERE session_id = ${sessionId}`)
  await host.reply({ sessionId, body: `Looked again, nothing new.` })
  await db.transaction((tx) =>
    postAgentInputInTx(tx, assistant!, me.id, `Schedule \`0 * * * *\` fired.`, {
      triggerId: crypto.randomUUID(),
      eventId: null,
      name: null,
    })
  )
  return { me, mate, sessionId, messageId }
}

describe.skipIf(!ready.ok)(`#FEED-298 a 25-day-old card of a persistent assistant conversation`, () => {
  for (const voice of [true, false]) {
    const kind = voice ? `voice` : `plain`

    it(`${kind}: the tick answers nothing, the row lists it, the conversation never ends`, async () => {
      const w = await staleCard(voice)
      const listed = await rows(w.me.id, w.sessionId)
      expect(listed).toMatchObject([{ messageId: w.messageId, sessionId: w.sessionId, kind: `question` }])
      expect(Date.now() - Date.parse(listed[0]!.createdAt)).toBeGreaterThan(24 * DAY)
      expect((await row(w.sessionId)).needsInput).toBe(true)
      // The reported way out that is none: FEED-299's expiry needs an END.
      const end = await mcp(w.me)(`sessions_write`, { op: `end`, id: w.sessionId })
      expect(end.isError).toBe(true)
      expect(text(end)).toContain(`never ends`)
      expect(await rows(w.me.id, w.sessionId)).toHaveLength(1)
    })

    it(`${kind}: the Needs you row's Dismiss clears it`, async () => {
      const w = await staleCard(voice)
      const [listed] = await rows(w.me.id, w.sessionId)
      await expect(trpc(w.mate.id).answer({ messageId: listed!.messageId, dismissed: true })).rejects.toThrow()
      await trpc(w.me.id).answer({ messageId: listed!.messageId, dismissed: true })
      await cleared(w)
    })

    it(`${kind}: MCP sessions_send {answer: {dismissed}} with no session clears it`, async () => {
      const w = await staleCard(voice)
      const refused = await mcp(w.mate)(`sessions_send`, { sessionId: w.sessionId, answer: { dismissed: true } })
      expect(refused.isError).toBe(true)
      const sent = await mcp(w.me)(`sessions_send`, { sessionId: w.sessionId, answer: { dismissed: true } })
      expect(sent.isError, text(sent)).toBeFalsy()
      await cleared(w)
    })
  }

  it(`voice: the member's MCP key still never ANSWERS it`, async () => {
    const w = await staleCard(true)
    const answered = await mcp(w.me)(`sessions_send`, { sessionId: w.sessionId, answer: { answer: `Yes` } })
    expect(answered.isError).toBe(true)
    expect(text(answered)).toContain(`its member may withdraw a card`)
    expect(await rows(w.me.id, w.sessionId)).toHaveLength(1)
  })
})

async function cleared(w: { me: McpUser; sessionId: string; messageId: string }) {
  const [card] = await db.select().from(messages).where(eq(messages.id, w.messageId))
  expect(card!.answeredAt).not.toBeNull()
  expect(card!.card).toMatchObject({ kind: `question`, dismissed: true })
  expect(await rows(w.me.id, w.sessionId)).toEqual([])
  const session = await row(w.sessionId)
  expect(session.needsInput).toBe(false)
  expect(session.status).not.toBe(`ended`)
}
