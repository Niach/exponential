// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, sessions, teamMembers, teams, devices } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #FEED-172 (packages/eagent/fixtures/incidents.json): a crowded chat that
// asked its team agent in a PLAIN message gets the agent's reply; the
// crowding rule forks only new work. Against the REAL migrated Postgres.

process.env.BETTER_AUTH_SECRET ||= `fresh-chat-test-secret-0123456789abcdef`

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

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'conversations') AS ok`)
if (!ready.ok) console.info(`[agent-fresh-chat.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#FEED-172 agent reuse`, () => {
  it(`a reply to a crowded chat that asked reaches it; new work after it forks`, async () => {
    const owner = `fresh-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${owner}, 'Tester', ${`${owner}@fresh.test`}, true, false, now(), now())
    `)
    made.users.push(owner)
    const [team] = await db.insert(teams).values({ name: `Fresh`, slug: `fresh-${id8()}` }).returning({ id: teams.id })
    const teamId = team!.id
    made.teams.push(teamId)
    await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
    const [agent] = await db.insert(agents).values({ teamId, name: `Helper`, deviceId, ownerId: owner }).returning()

    // The agent's conversation, then a chat it starts.
    const conversation = await sendSession({ userId: owner, input: { agentId: agent!.id, text: `Hi.` } })
    const chat = await sendSession({
      userId: owner,
      callerSessionId: conversation.sessionId,
      viaMcp: true,
      input: { teamId, text: `Write the spec.` },
    })
    const chatRow = (await db.select().from(sessions).where(sql`${sessions.id} = ${chat.sessionId}`))[0]!
    // Eight sub-sessions (ended ones count): crowded.
    await db.insert(sessions).values(
      Array.from({ length: 8 }, () => ({
        id: crypto.randomUUID(),
        teamId,
        parentId: chatRow.id,
        rootId: chatRow.id,
        depth: 1,
        userId: owner,
        deviceId,
        status: `ended` as const,
        endedAt: new Date(),
        shared: chatRow.shared,
      }))
    )
    const fromAgent = (text: string) =>
      sendSession({
        userId: owner,
        callerSessionId: conversation.sessionId,
        viaMcp: true,
        input: { sessionId: chatRow.id, text },
      })

    // Before the chat speaks to the agent, new work forks.
    const before = await fromAgent(`Unrelated new work.`)
    expect(before.sessionId).not.toBe(chatRow.id)
    expect(before.warning).toMatch(/8 sub-sessions/)

    // The chat asks in a plain message; the agent's reply reaches it.
    await sendSession({
      userId: owner,
      callerSessionId: chatRow.id,
      viaMcp: true,
      input: { agentId: agent!.id, text: `Should the spec cover Windows too?` },
    })
    const reply = await fromAgent(`No: Linux and macOS only.`)
    expect(reply.sessionId).toBe(chatRow.id)
    expect(reply.warning).toBeUndefined()

    // The caller spoke last: its next message is new work again.
    const after = await fromAgent(`Also write the migration guide.`)
    expect(after.sessionId).not.toBe(chatRow.id)
    expect(after.warning).toMatch(/8 sub-sessions/)
  })

  it(`#FEED-172 (85ee1eff): a crowded chat whose CHILD asked the agent gets the reply in place`, async () => {
    const owner = `fresh-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${owner}, 'Tester', ${`${owner}@fresh.test`}, true, false, now(), now())
    `)
    made.users.push(owner)
    const [team] = await db.insert(teams).values({ name: `Fresh`, slug: `fresh-${id8()}` }).returning({ id: teams.id })
    const teamId = team!.id
    made.teams.push(teamId)
    await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
    const [agent] = await db.insert(agents).values({ teamId, name: `Helper`, deviceId, ownerId: owner }).returning()
    const conversation = await sendSession({ userId: owner, input: { agentId: agent!.id, text: `Hi.` } })
    const chat = await sendSession({
      userId: owner,
      callerSessionId: conversation.sessionId,
      viaMcp: true,
      input: { teamId, text: `Coordinate the release.` },
    })
    const chatRow = (await db.select().from(sessions).where(sql`${sessions.id} = ${chat.sessionId}`))[0]!
    await db.insert(sessions).values(
      Array.from({ length: 8 }, () => ({
        id: crypto.randomUUID(),
        teamId,
        parentId: chatRow.id,
        rootId: chatRow.id,
        depth: 1,
        userId: owner,
        deviceId,
        status: `ended` as const,
        endedAt: new Date(),
        shared: chatRow.shared,
      }))
    )
    const child = await sendSession({
      userId: owner,
      callerSessionId: chatRow.id,
      viaMcp: true,
      input: { parentId: chatRow.id, text: `Check the locales.` },
    })
    // The CHILD asks the agent; the agent answers the chat it belongs to.
    await sendSession({
      userId: owner,
      callerSessionId: child.sessionId,
      viaMcp: true,
      input: { agentId: agent!.id, text: `Which locale is the default?` },
    })
    const reply = await sendSession({
      userId: owner,
      callerSessionId: conversation.sessionId,
      viaMcp: true,
      input: { sessionId: chatRow.id, text: `German.` },
    })
    expect(reply.sessionId).toBe(chatRow.id)
    expect(reply.warning).toBeUndefined()
  })
})
