// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #FEED-173/#FEED-174 (packages/eagent/fixtures/incidents.json): a send
// naming no session and carrying nothing to start never creates a thread.
// `{wakeOn}` alone registers the wake on the CALLING session; without it
// the send is refused. Against the REAL migrated Postgres.

process.env.BETTER_AUTH_SECRET ||= `watch-only-test-secret-0123456789abcdef`

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
if (!ready.ok) console.info(`[send-watch-only.real-db] skipped: ${ready.reason}`)

const { sendSession, START_NEEDS_WORK } = await import(`@/lib/sessions/send`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function world() {
  const owner = `watch-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@watch.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `Watch`, slug: `watch-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const root = await sendSession({ userId: owner, input: { teamId, deviceId, text: `Coordinate.` } })
  const child = await sendSession({ userId: owner, input: { parentId: root.sessionId, text: `Do part one.` } })
  const other = await sendSession({ userId: owner, input: { teamId, deviceId, text: `Unrelated.` } })
  const roots = async () =>
    (await db.select({ id: sessions.id }).from(sessions).where(eq(sessions.teamId, teamId))).length
  return { owner, teamId, deviceId, root, child, other, roots }
}

describe.skipIf(!ready.ok)(`#FEED-173/#FEED-174 sessions_send with no target`, () => {
  it(`{wakeOn} alone watches from the caller and starts nothing`, async () => {
    const w = await world()
    const count = await w.roots()
    const result = await sendSession({
      userId: w.owner,
      callerSessionId: w.child.sessionId,
      viaMcp: true,
      input: { wakeOn: [w.other.sessionId] },
    })
    expect(result.sessionId).toBe(w.child.sessionId)
    expect(result.messageId).toBeNull()
    expect(await w.roots()).toBe(count)
    const [row] = await db.select({ wakeOn: sessions.wakeOn }).from(sessions).where(eq(sessions.id, w.child.sessionId))
    expect(row!.wakeOn).toContain(w.other.sessionId)
  })

  it(`{wakeOn} with a name or a seal is no watch: refused, the caller untouched`, async () => {
    const w = await world()
    const count = await w.roots()
    await expect(
      sendSession({
        userId: w.owner,
        callerSessionId: w.child.sessionId,
        viaMcp: true,
        input: { wakeOn: [w.other.sessionId], name: `Watch it` },
      })
    ).rejects.toThrow(START_NEEDS_WORK)
    const [row] = await db.select({ wakeOn: sessions.wakeOn }).from(sessions).where(eq(sessions.id, w.child.sessionId))
    expect(row!.wakeOn ?? []).not.toContain(w.other.sessionId)
    expect(await w.roots()).toBe(count)
  })

  it(`no target, no text, no wakeOn: refused`, async () => {
    const w = await world()
    const count = await w.roots()
    await expect(
      sendSession({ userId: w.owner, callerSessionId: w.child.sessionId, viaMcp: true, input: { text: `  ` } })
    ).rejects.toThrow(START_NEEDS_WORK)
    await expect(sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId } })).rejects.toThrow(
      START_NEEDS_WORK
    )
    expect(await w.roots()).toBe(count)
  })

  it(`{wakeOn} with a text still starts the thread (and watches it)`, async () => {
    const w = await world()
    const started = await sendSession({
      userId: w.owner,
      callerSessionId: w.child.sessionId,
      viaMcp: true,
      input: { deviceId: w.deviceId, text: `New work.`, wakeOn: [w.other.sessionId] },
    })
    expect(started.sessionId).not.toBe(w.child.sessionId)
    expect(started.status).toBe(`queued`)
  })
})
