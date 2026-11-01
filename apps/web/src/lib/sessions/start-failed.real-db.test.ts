// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"

// #EXP-1536 / #EXP-1542: `sessions.report` with a failed start
// (`blocked: start_failed`, `handled`) against the REAL migrated Postgres
// (triggers applied): the row reads idle, not busy, blocked `start_failed`
// and under Needs you; it is NEVER said as a wall (no `session_blocked`
// bus line, no push); a stale device's report flags nothing; the
// relaunch's report (no `blocked`) clears it. Each case builds a fresh
// team; the suite deletes every team (and user) it made (cascade).

const pushes = vi.hoisted(() => ({ blocked: [] as string[] }))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async (sessionId: string) => {
    pushes.blocked.push(sessionId)
  },
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
if (!ready.ok) console.info(`[start-failed.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `startfail-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@start-failed.test`}, true, false, now(), now())
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

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

async function world() {
  const owner = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Start failed`, slug: `start-failed-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `SF` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `DESKTOP-2K0CVDE`, kind: `desktop` })
  const rootId = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `root` } })).sessionId
  const childId = (await sendSession({ userId: owner, input: { parentId: rootId, text: `part` } })).sessionId
  return { owner, deviceId, rootId, childId, host: caller(owner) }
}

const row = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

/** The device's failed-start report (crates/coding `start_failed_report`). */
const startFailed = (sessionId: string, deviceId: string, since: string) => ({
  sessionId,
  deviceId,
  status: `idle` as const,
  busy: false,
  blocked: { kind: `start_failed` as const, agent: ``, window: ``, resetsAt: null, since, handled: true },
})

const blockedLines = async (sessionId: string, about: string) =>
  (await db.select().from(messages).where(inSession(sessionId))).filter((m) => {
    const card = m.card as { event?: string; sessionId?: string } | null
    return card?.event === `session_blocked` && card.sessionId === about
  })

describe.skipIf(!ready.ok)(`a failed start's report`, () => {
  it(`flags the row start_failed under Needs you, never said as a wall; the relaunch's report clears it`, async () => {
    const w = await world()
    await w.host.claim({ sessionId: w.childId, deviceId: w.deviceId })
    const since = new Date().toISOString()
    pushes.blocked.length = 0

    await w.host.report(startFailed(w.childId, w.deviceId, since))
    expect(await row(w.childId)).toMatchObject({
      status: `idle`,
      busy: false,
      needsInput: true,
      blocked: { kind: `start_failed`, since },
    })
    // `handled` is never stored.
    expect((await row(w.childId)).blocked).not.toHaveProperty(`handled`)
    // No wall line in the parent, no push.
    expect(await blockedLines(w.rootId, w.childId)).toEqual([])
    expect(pushes.blocked).toEqual([])

    // The relaunch runs: its report (no `blocked`) clears the flag.
    await w.host.report({ sessionId: w.childId, deviceId: w.deviceId, status: `running`, busy: true })
    expect(await row(w.childId)).toMatchObject({ status: `running`, busy: true, blocked: null, needsInput: false })
  })

  it(`a stale device's failed start flags nothing`, async () => {
    const w = await world()
    await w.host.claim({ sessionId: w.childId, deviceId: w.deviceId })
    await w.host.report(startFailed(w.childId, `dev-elsewhere`, new Date().toISOString()))
    expect(await row(w.childId)).toMatchObject({ status: `running`, blocked: null, needsInput: false })
  })
})
