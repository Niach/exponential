// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

// F21: `sessions.send {clientMessageId}` (tRPC only) against the REAL
// migrated Postgres: a retried send naming a key its sender already posted
// returns that message's result and posts nothing (the desktop meeting
// flush retries a committed-but-timed-out send).

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'messages' AND column_name = 'client_message_id') AS ok
`)
if (!ready.ok) console.info(`[send-client-message-id.real-db] skipped: ${ready.reason}`)

const { sessionsRouter, addsMention } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `cmid-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@cmid.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const caller = (userId: string) =>
  sessionsRouter.createCaller({
    session: { user: { id: userId } },
    db,
    request: new Request(`http://x.test`),
  } as never)

async function world() {
  const owner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Cmid`, slug: `cmid-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `CM` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server`, sharedTeamIds: [teamId] })
  return { owner, mate, teamId, deviceId }
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

const keyed = (sessionId: string, clientMessageId: string) =>
  db
    .select({ id: messages.id, body: messages.body })
    .from(messages)
    .where(and(inSession(sessionId), eq(messages.clientMessageId, clientMessageId)))

describe.skipIf(!ready.ok)(`F21 sessions.send clientMessageId`, () => {
  it(`a retry naming the same key returns the first message and posts nothing`, async () => {
    const w = await world()
    const me = caller(w.owner)
    const { sessionId } = await me.send({ teamId: w.teamId, deviceId: w.deviceId, text: `start` })
    const key = crypto.randomUUID()
    const first = await me.send({ sessionId, text: `line one`, clientMessageId: key })
    expect(first.messageId).toBeTruthy()
    const retry = await me.send({ sessionId, text: `line one`, clientMessageId: key })
    expect(retry).toMatchObject({ sessionId, messageId: first.messageId })
    expect(typeof retry.txid).toBe(`number`)
    expect(await keyed(sessionId, key)).toEqual([{ id: first.messageId, body: `line one` }])

    // Concurrent retries: one row, every caller gets it back.
    const racing = crypto.randomUUID()
    const results = await Promise.all(
      [0, 1, 2].map(() => me.send({ sessionId, text: `line two`, clientMessageId: racing }))
    )
    const rows = await keyed(sessionId, racing)
    expect(rows).toHaveLength(1)
    expect(results.map((result) => result.messageId)).toEqual([rows[0]!.id, rows[0]!.id, rows[0]!.id])

    // No key = no dedupe; another key = another message.
    await me.send({ sessionId, text: `line one` })
    const other = await me.send({ sessionId, text: `line one`, clientMessageId: crypto.randomUUID() })
    expect(other.messageId).not.toBe(first.messageId)

    // Another sender's key never returns this sender's message.
    const mates = await caller(w.mate).send({ teamId: w.teamId, deviceId: w.deviceId, text: `mine` })
    const theirs = await caller(w.mate).send({ sessionId: mates.sessionId, text: `x`, clientMessageId: key })
    expect(theirs.messageId).not.toBe(first.messageId)
    expect(theirs.sessionId).toBe(mates.sessionId)
  })

  it(`rejects an over-long key`, async () => {
    const w = await world()
    const me = caller(w.owner)
    const { sessionId } = await me.send({ teamId: w.teamId, deviceId: w.deviceId, text: `start` })
    await expect(me.send({ sessionId, text: `x`, clientMessageId: `k`.repeat(65) })).rejects.toThrow()
  })
})

describe(`F32 addsMention`, () => {
  it(`a new message counts any mention; an edit only an added one`, () => {
    expect(addsMention(null, `hi @a@x.test`)).toBe(true)
    expect(addsMention(`hi @a@x.test`, `hi @a@x.test, more text`)).toBe(false)
    expect(addsMention(`hi @a@x.test`, `hi @A@X.test`)).toBe(false)
    expect(addsMention(`hi @a@x.test`, `hi @a@x.test and @b@x.test`)).toBe(true)
    expect(addsMention(`hi @a@x.test and @b@x.test`, `hi @b@x.test`)).toBe(false)
  })
})
