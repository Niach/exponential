// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

// #EXP-1618: a device parked a session's unpushed work on a leftover
// branch: the conversation gets ONE server line (pre-consumed: it never
// wakes the agent) and the session's starter an inbox row. Real Postgres
// (harness copied from escalate.real-db.test.ts).

const notifyLeftover = vi.hoisted(() => vi.fn(async (..._args: unknown[]) => {}))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionLeftover: notifyLeftover,
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors') AS ok
`)
if (!ready.ok) console.info(`[leftover.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { sessionsRouter, leftoverText } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

const caller = (userId: string) =>
  sessionsRouter.createCaller({
    session: { user: { id: userId } },
    db,
    request: new Request(`http://x.test`),
  } as never)

describe.skipIf(!ready.ok)(`#EXP-1618: sessions.leftover`, () => {
  it(`posts one non-waking line and tells the starter`, async () => {
    const owner = `leftover-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${owner}, 'Tester', ${`${owner}@leftover.test`}, true, false, now(), now())
    `)
    made.users.push(owner)
    const [team] = await db
      .insert(teams)
      .values({ name: `Leftover`, slug: `leftover-${id8()}` })
      .returning({ id: teams.id })
    made.teams.push(team!.id)
    await db.insert(teamMembers).values({ teamId: team!.id, userId: owner, role: `owner` })
    await db.insert(boards).values({ teamId: team!.id, name: `Core`, slug: `core-${id8()}`, prefix: `LO` })
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
    const { sessionId } = await sendSession({ userId: owner, input: { teamId: team!.id, deviceId, text: `work` } })

    await caller(owner).leftover({ sessionId, branch: `exp/leftover-1a2b3c4d` })
    const lines = await db.select().from(messages).where(inSession(sessionId))
    const line = lines.find((m) => m.body === leftoverText(`exp/leftover-1a2b3c4d`))
    expect(line).toMatchObject({ authorKind: `system` })
    expect(line?.consumedAt).not.toBeNull()
    expect(notifyLeftover).toHaveBeenCalledWith(sessionId, `exp/leftover-1a2b3c4d`)

    // Only a leftover branch name is accepted.
    await expect(caller(owner).leftover({ sessionId, branch: `main` })).rejects.toThrow()
    await expect(caller(owner).leftover({ sessionId, branch: `exp/leftover-1a2b3c4d; rm` })).rejects.toThrow()
    const after = await db.select().from(messages).where(inSession(sessionId))
    expect(after.filter((m) => m.authorKind === `system` && m.body.startsWith(`Unpushed`))).toHaveLength(1)
  })
})
