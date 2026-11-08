// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

// #EXP-1731 (packages/eagent/fixtures/subagents.json): CLI subagents as
// child rows the device writes. Real Postgres (harness copied from
// leftover.real-db.test.ts).

const kills = vi.hoisted(() => vi.fn(async (..._args: unknown[]) => ({ delivered: false })))
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
  relayPostKill: kills,
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors') AS ok
`)
if (!ready.ok) console.info(`[subagent.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { subagentRedirectText } = await import(`@/lib/sessions/subagent`)

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

async function setup() {
  const owner = `subagent-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@subagent.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db
    .insert(teams)
    .values({ name: `Subagents`, slug: `subagents-${id8()}` })
    .returning({ id: teams.id })
  made.teams.push(team!.id)
  await db.insert(teamMembers).values({ teamId: team!.id, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId: team!.id, name: `Core`, slug: `core-${id8()}`, prefix: `SA` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const { sessionId } = await sendSession({ userId: owner, input: { teamId: team!.id, deviceId, text: `work` } })
  await caller(owner).report({ sessionId, deviceId, status: `running`, busy: true })
  return { owner, deviceId, rootId: sessionId }
}

const row = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!
const lines = (id: string) => db.select().from(messages).where(inSession(id))

describe.skipIf(!ready.ok)(`#EXP-1731: sessions.subagent`, () => {
  it(`writes one row per key, ends it on itself with no report into the parent, never kills it`, async () => {
    const { owner, deviceId, rootId } = await setup()
    const before = (await lines(rootId)).length
    const [a, b] = await Promise.all([
      caller(owner).subagent({ sessionId: rootId, deviceId, key: `toolu_1`, status: `running`, title: `Find call sites` }),
      caller(owner).subagent({ sessionId: rootId, deviceId, key: `toolu_1`, status: `running` }),
    ])
    expect(a.sessionId).toBeTruthy()
    expect(b.sessionId).toBe(a.sessionId)
    const sub = await row(a.sessionId!)
    expect(sub).toMatchObject({
      parentId: rootId,
      rootId,
      depth: 1,
      deviceId,
      subagentKey: `toolu_1`,
      status: `running`,
      busy: true,
      title: `Find call sites`,
    })
    expect(sub.startedAt).not.toBeNull()

    kills.mockClear()
    await caller(owner).subagent({ sessionId: rootId, deviceId, key: `toolu_1`, status: `ended`, summary: `Found 3.` })
    expect(await row(sub.id)).toMatchObject({ status: `ended`, busy: false })
    const own = await lines(sub.id)
    expect(own.find((m) => m.authorKind === `agent`)).toMatchObject({ body: `Found 3.` })
    // Nothing reached the parent: no card, no report, no wake-up.
    expect((await lines(rootId)).length).toBe(before)
    expect(kills).not.toHaveBeenCalled()

    // A running edge for the ended key revives it in place.
    const again = await caller(owner).subagent({ sessionId: rootId, deviceId, key: `toolu_1`, status: `running` })
    expect(again.sessionId).toBe(sub.id)
    expect(await row(sub.id)).toMatchObject({ status: `running`, busy: true, endedAt: null })
  })

  it(`ignores another device's edge, refuses one under a subagent, redirects a send, and sweeps on the parent's turn end`, async () => {
    const { owner, deviceId, rootId } = await setup()
    expect((await caller(owner).subagent({ sessionId: rootId, deviceId: `other`, key: `k`, status: `running` })).sessionId).toBeNull()
    const { sessionId: subId } = await caller(owner).subagent({ sessionId: rootId, deviceId, key: `toolu_s`, status: `running` })
    await expect(caller(owner).subagent({ sessionId: subId!, deviceId, key: `x`, status: `running` })).rejects.toThrow(
      /A subagent starts no sessions/
    )
    await expect(sendSession({ userId: owner, input: { parentId: subId!, text: `Split.` } })).rejects.toThrow(
      /A subagent starts no sessions/
    )

    const sent = await sendSession({ userId: owner, input: { sessionId: subId!, text: `Also check the tests.` } })
    expect(sent.sessionId).toBe(rootId)
    expect(sent.warning).toContain(subagentRedirectText(subId!, rootId))
    expect((await lines(rootId)).some((m) => m.body === `Also check the tests.`)).toBe(true)

    // A mid-turn report keeps it; the turn end sweeps it.
    await caller(owner).report({ sessionId: rootId, deviceId, status: `running`, busy: true })
    expect((await row(subId!)).status).toBe(`running`)
    await caller(owner).report({ sessionId: rootId, deviceId, status: `idle`, busy: false })
    expect((await row(subId!)).status).toBe(`ended`)
    const live = await db
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.parentId, rootId), sql`status <> 'ended'`))
    expect(live).toHaveLength(0)
  })
})
