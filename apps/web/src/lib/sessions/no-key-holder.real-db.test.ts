// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

// #EXP-1959: the device running a session in a KEYED tree holds no key
// holder (its eNet gave up), so its agent's replies cannot be sealed: a
// reply in clear stays refused, and the conversation gets ONE server line
// (in clear, pre-consumed) naming why. Real Postgres (harness copied from
// leftover.real-db.test.ts).

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionLeftover: async () => {},
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
if (!ready.ok) console.info(`[no-key-holder.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { sessionsRouter, noKeyHolderText } = await import(`@/lib/trpc/sessions`)

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

describe.skipIf(!ready.ok)(`#EXP-1959: sessions.noKeyHolder`, () => {
  it(`a keyed tree refuses the reply in clear and gets one server line`, async () => {
    const owner = `nokeyholder-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${owner}, 'Tester', ${`${owner}@nokeyholder.test`}, true, false, now(), now())
    `)
    made.users.push(owner)
    const [team] = await db
      .insert(teams)
      .values({ name: `No key holder`, slug: `nokeyholder-${id8()}` })
      .returning({ id: teams.id })
    made.teams.push(team!.id)
    await db.insert(teamMembers).values({ teamId: team!.id, userId: owner, role: `owner` })
    await db.insert(boards).values({ teamId: team!.id, name: `Core`, slug: `core-${id8()}`, prefix: `NK` })
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
    const { sessionId } = await sendSession({ userId: owner, input: { teamId: team!.id, deviceId, text: `work` } })
    // An existing tree that has its key (`seal_epoch` set on the root).
    await db
      .update(sessions)
      .set({ sealEpoch: 1, sealPublicKey: `A`.repeat(43) })
      .where(eq(sessions.id, sessionId))

    await expect(caller(owner).reply({ sessionId, body: `the answer in clear` })).rejects.toThrow(/encrypted/)
    await caller(owner).noKeyHolder({ sessionId, reason: `node_failed` })
    await caller(owner).noKeyHolder({ sessionId, reason: `node_failed` })
    await expect(caller(owner).noKeyHolder({ sessionId, reason: `anything` as never })).rejects.toThrow()

    const rows = await db.select().from(messages).where(inSession(sessionId))
    expect(rows.some((m) => m.body.includes(`the answer in clear`))).toBe(false)
    const lines = rows.filter((m) => m.body === noKeyHolderText(`node_failed`))
    expect(lines).toHaveLength(1)
    expect(lines[0]).toMatchObject({ authorKind: `system` })
    expect(lines[0]?.consumedAt).not.toBeNull()
    expect(noKeyHolderText(`node_failed`)).toContain(`P2P is off: its node failed to start`)
  })
})
