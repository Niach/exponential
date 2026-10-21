// @vitest-environment node
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { boards, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// NEW-1: the server watchdog against the REAL migrated Postgres. What
// stalls is flagged on the row (`blocked.kind`), lands under Needs you
// (`needs_input`) and is said ONCE in the parent's conversation through the
// bus; each flag has exactly one clearer (a claim, the next device report,
// the device's heartbeat). `runWatchdog(now)` scans the WHOLE shared dev
// database, so every case runs at a `now` in the year 2000 and backdates
// only its own rows (`created_at` / `updated_at` / `last_seen_at`, written
// past the triggers): nobody else's row is ever old enough to match, and no
// case sleeps. Each case builds a fresh team; the suite deletes every team
// (and user) it made (cascade).

const relay = vi.hoisted(() => ({
  on: false,
  nudges: [] as Array<{ userId: string; deviceId: string }>,
}))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
}))
// A relay is "configured" only for the nudge case; nothing leaves the test.
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => (relay.on ? { url: `http://relay.test`, secret: `s` } : null),
  relayPostNudge: async (_config: unknown, userId: string, deviceId: string) => {
    relay.nudges.push({ userId, deviceId })
    return { delivered: true }
  },
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))
// The scheduler's metrics are not under test.
vi.mock(`@/lib/metrics/registry`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/metrics/registry")>()),
  reportSchedulerRun: () => {},
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'wake_on')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[watchdog.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { BUS_COPY } = await import(`@/lib/sessions/bus`)
const { buildQuestionCard, postAskCard } = await import(`@/lib/sessions/plan`)
const { runWatchdog, restoreDeviceSessionsInTx } = await import(`@/lib/sessions/watchdog`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)
const { devicesRouter } = await import(`@/lib/trpc/devices`)

const MIN = 60 * 1000
/** The watchdog's clock: far before any real row in the dev database. */
const NOW = new Date(`2000-01-01T01:00:00.000Z`)
const ago = (ms: number) => new Date(NOW.getTime() - ms)
const watchdog = contract.sessionWatchdog

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `dog-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@watchdog.test`}, true, false, now(), now())
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

async function world(opts: { shared?: boolean } = {}) {
  const owner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Watchdog`, slug: `watchdog-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `WD` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({
    userId: owner,
    deviceId,
    label: `box`,
    kind: `server`,
    sharedTeamIds: opts.shared ? [teamId] : [],
  })
  const root = async (text = `root`, userId = owner) =>
    (await sendSession({ userId, input: { teamId, deviceId, text } })).sessionId
  const child = async (parentId: string, text = `child`) =>
    (await sendSession({ userId: owner, input: { parentId, text } })).sessionId
  return { owner, mate, teamId, deviceId, root, child, host: caller(owner) }
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

beforeEach(() => {
  relay.on = false
  relay.nudges.length = 0
})

const row = async (id: string) =>
  (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

/** Writes past the triggers (the updated_at trigger would re-stamp it). */
async function backdate(write: () => ReturnType<typeof sql>) {
  await db.transaction(async (tx) => {
    await tx.execute(sql`SET LOCAL session_replication_role = replica`)
    await tx.execute(write())
  })
}
const backdateSession = (id: string, column: `created_at` | `updated_at`, at: Date) =>
  backdate(() => sql`UPDATE sessions SET ${sql.raw(column)} = ${at.toISOString()} WHERE id = ${id}`)
const backdateDevice = (deviceId: string, at: Date) =>
  backdate(() => sql`UPDATE devices SET last_seen_at = ${at.toISOString()} WHERE device_id = ${deviceId}`)

/** The bus's watchdog lines about `about` in `sessionId`. */
const lines = async (sessionId: string, event: string, about: string) =>
  (await db.select().from(messages).where(eq(messages.sessionId, sessionId))).filter((m) => {
    const card = m.card as { event?: string; sessionId?: string } | null
    return card?.event === event && card.sessionId === about
  })

describe.skipIf(!ready.ok)(`the watchdog`, () => {
  it(`flags a queued session nobody claimed for unclaimedMinutes, ONCE; the claim clears it`, async () => {
    const w = await world()
    const rootId = await w.root()
    const late = await w.child(rootId, `never picked up`)
    const fresh = await w.child(rootId, `just queued`)
    await backdateSession(rootId, `created_at`, ago((watchdog.unclaimedMinutes + 1) * MIN))
    await backdateSession(late, `created_at`, ago((watchdog.unclaimedMinutes + 1) * MIN))
    await backdateSession(fresh, `created_at`, ago((watchdog.unclaimedMinutes - 1) * MIN))

    const run = await runWatchdog(NOW)
    expect(run.unclaimed).toBeGreaterThanOrEqual(2)
    expect(await row(late)).toMatchObject({
      status: `queued`,
      needsInput: true,
      blocked: { kind: `unclaimed`, since: NOW.toISOString() },
    })
    expect(await row(fresh)).toMatchObject({ blocked: null, needsInput: false })
    // The child's line wakes its parent.
    expect(await lines(rootId, `session_unclaimed`, late)).toMatchObject([
      { authorKind: `system`, authorSessionId: late, consumedAt: null, body: BUS_COPY.session_unclaimed },
    ])
    // The root has nobody listening: its line lands on itself, pre-consumed.
    const own = await lines(rootId, `session_unclaimed`, rootId)
    expect(own).toMatchObject([{ authorSessionId: null }])
    expect(own[0]!.consumedAt).not.toBeNull()

    // Said once.
    await runWatchdog(NOW)
    expect(await lines(rootId, `session_unclaimed`, late)).toHaveLength(1)

    // The claim clears the flag.
    await w.host.claim({ sessionId: late })
    expect(await row(late)).toMatchObject({ status: `running`, blocked: null, needsInput: false })
  })

  it(`nudges the host device again for a live session's input pending past nudgeMinutes`, async () => {
    const w = await world({ shared: true })
    // A teammate's start on the owner's shared box: the owner hosts it.
    const rootId = await w.root(`hosted`, w.mate)
    await w.host.claim({ sessionId: rootId })
    const waiting = await sendSession({ userId: w.mate, input: { sessionId: rootId, text: `are you there?` } })
    const fresh = await w.root(`fresh`, w.mate)
    await w.host.claim({ sessionId: fresh })
    await sendSession({ userId: w.mate, input: { sessionId: fresh, text: `just now` } })
    await backdate(
      () =>
        sql`UPDATE messages SET created_at = ${ago((watchdog.nudgeMinutes + 1) * MIN).toISOString()} WHERE id = ${waiting.messageId}`
    )

    // No relay configured: nothing to nudge.
    relay.nudges.length = 0
    expect((await runWatchdog(NOW)).nudged).toBe(0)
    expect(relay.nudges).toEqual([])

    relay.on = true
    const run = await runWatchdog(NOW)
    expect(relay.nudges).toContainEqual({ userId: w.owner, deviceId: w.deviceId })
    expect(run.nudged).toBeGreaterThanOrEqual(1)
    // A nudge writes no row and says nothing.
    expect(await row(rootId)).toMatchObject({ status: `running`, blocked: null })
    const nudgedRows = await db
      .select()
      .from(messages)
      .where(and(eq(messages.sessionId, rootId), eq(messages.authorKind, `system`)))
    expect(nudgedRows.filter((m) => (m.card as { event?: string } | null)?.event?.startsWith(`session_un`))).toEqual([])

    // Consumed: nothing pending, no nudge.
    await w.host.consume({ sessionId: rootId, messageIds: [waiting.messageId!] })
    relay.nudges.length = 0
    await runWatchdog(NOW)
    expect(relay.nudges.filter((n) => n.deviceId === w.deviceId)).toEqual([])
  })

  it(`flags a busy session with no report for stuckMinutes, ONCE; the next device report clears it`, async () => {
    const w = await world()
    const rootId = await w.root()
    const stuck = await w.child(rootId, `long turn`)
    const working = await w.child(rootId, `short turn`)
    for (const id of [stuck, working]) {
      await w.host.claim({ sessionId: id })
      await w.host.report({ sessionId: id, busy: true })
    }
    await backdateSession(stuck, `updated_at`, ago((watchdog.stuckMinutes + 1) * MIN))
    await backdateSession(working, `updated_at`, ago((watchdog.stuckMinutes - 1) * MIN))

    const run = await runWatchdog(NOW)
    expect(run.stuck).toBeGreaterThanOrEqual(1)
    expect(await row(stuck)).toMatchObject({
      status: `running`,
      busy: true,
      needsInput: true,
      blocked: { kind: `stuck`, since: NOW.toISOString() },
    })
    expect(await row(working)).toMatchObject({ blocked: null, needsInput: false })
    expect(await lines(rootId, `session_stuck`, stuck)).toMatchObject([
      { authorKind: `system`, authorSessionId: stuck, consumedAt: null, body: BUS_COPY.session_stuck },
    ])
    await runWatchdog(NOW)
    expect(await lines(rootId, `session_stuck`, stuck)).toHaveLength(1)

    // Any report = activity: the flag and Needs you clear.
    await w.host.report({ sessionId: stuck, busy: true })
    expect(await row(stuck)).toMatchObject({ blocked: null, needsInput: false, busy: true })
  })

  it(`a host device silent past the online window pauses its live sessions; its heartbeat restores them`, async () => {
    const w = await world({ shared: true })
    const rootId = await w.root()
    const childId = await w.child(rootId, `working`)
    await w.host.claim({ sessionId: childId })
    await w.host.report({ sessionId: childId, busy: true })
    // A teammate's session hosted on the same shared box, waiting on its own card.
    const hosted = await w.root(`hosted`, w.mate)
    await w.host.claim({ sessionId: hosted })
    await postAskCard({
      userId: w.mate,
      sessionId: hosted,
      card: buildQuestionCard({ question: `Which one?`, options: [{ label: `a` }, { label: `b` }] }),
    })
    // A queued session is not live: the offline sweep leaves it alone.
    const queued = await w.child(rootId, `not yet`)
    await backdateDevice(w.deviceId, ago((contract.device.onlineWindowSeconds + 1) * 1000))

    const run = await runWatchdog(NOW)
    expect(run.offline).toBeGreaterThanOrEqual(2)
    for (const id of [childId, hosted]) {
      expect(await row(id)).toMatchObject({
        status: `idle`,
        busy: false,
        needsInput: true,
        blocked: { kind: `device_offline`, since: NOW.toISOString() },
      })
    }
    expect(await row(queued)).toMatchObject({ status: `queued`, blocked: null })
    // Informational: pre-consumed in the parent.
    const [line] = await lines(rootId, `device_offline`, childId)
    expect(line).toMatchObject({ authorSessionId: childId, body: BUS_COPY.device_offline })
    expect(line!.consumedAt).not.toBeNull()
    // Said once.
    await runWatchdog(NOW)
    expect(await lines(rootId, `device_offline`, childId)).toHaveLength(1)

    // The heartbeat (the OWNER's device row) restores both, Needs you
    // re-derived from the open cards.
    const restored = await db.transaction((tx) =>
      restoreDeviceSessionsInTx(tx, { deviceId: w.deviceId, userId: w.owner })
    )
    expect(restored).toBe(2)
    expect(await row(childId)).toMatchObject({ status: `idle`, blocked: null, needsInput: false })
    expect(await row(hosted)).toMatchObject({ status: `idle`, blocked: null, needsInput: true })
    // Another account's heartbeat for an unrelated device restores nothing.
    await backdateDevice(w.deviceId, ago((contract.device.onlineWindowSeconds + 1) * 1000))
    await runWatchdog(NOW)
    expect(
      await db.transaction((tx) => restoreDeviceSessionsInTx(tx, { deviceId: `dev-${id8()}`, userId: w.owner }))
    ).toBe(0)
    expect((await row(childId)).blocked).toMatchObject({ kind: `device_offline` })
  })

  it(`a heartbeat's liveSessionIds flags a busy session its host no longer runs; listed ones and a beat without the field change nothing`, async () => {
    const w = await world({ shared: true })
    const rootId = await w.root()
    const dead = await w.child(rootId, `its process died`)
    const alive = await w.child(rootId, `still running`)
    const between = await w.child(rootId, `between turns`)
    // A teammate's session on the same shared box: the heartbeat speaks for
    // every session the device hosts, whoever started it.
    const mates = await w.root(`hosted`, w.mate)
    for (const id of [dead, alive, between, mates]) await w.host.claim({ sessionId: id })
    for (const id of [dead, alive, mates]) await w.host.report({ sessionId: id, busy: true })
    const beat = (liveSessionIds?: string[]) =>
      devicesRouter
        .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
        .heartbeat({ deviceId: w.deviceId, activeSessions: 1, defaultsSyncedAt: null, ...(liveSessionIds ? { liveSessionIds } : {}) })

    // No field: no claim either way.
    await beat()
    for (const id of [dead, alive, mates]) {
      expect(await row(id)).toMatchObject({ status: `running`, busy: true, blocked: null })
    }

    // Another account's beat naming this device owns no row: nothing moves.
    const stranger = await user()
    expect(
      await devicesRouter
        .createCaller({ session: { user: { id: stranger } }, db, request: new Request(`http://x.test`) } as never)
        .heartbeat({ deviceId: w.deviceId, activeSessions: 0, defaultsSyncedAt: null, liveSessionIds: [] })
    ).toMatchObject({ ok: false })
    expect(await row(dead)).toMatchObject({ busy: true, blocked: null })

    await beat([alive])
    for (const id of [dead, mates]) {
      expect(await row(id)).toMatchObject({
        status: `idle`,
        busy: false,
        needsInput: true,
        blocked: { kind: `stuck` },
      })
    }
    expect(await row(alive)).toMatchObject({ status: `running`, busy: true, blocked: null, needsInput: false })
    expect(await row(between)).toMatchObject({ status: `running`, busy: false, blocked: null })
    // The parent is woken once.
    expect(await lines(rootId, `session_stuck`, dead)).toMatchObject([
      { authorKind: `system`, authorSessionId: dead, consumedAt: null, body: BUS_COPY.session_stuck },
    ])
    await beat([alive])
    expect(await lines(rootId, `session_stuck`, dead)).toHaveLength(1)

    // The next report clears it, as for the watchdog's own `stuck`.
    await w.host.report({ sessionId: dead, status: `running`, busy: true })
    expect(await row(dead)).toMatchObject({ blocked: null, needsInput: false, busy: true })
  })

  it(`a usage wall keeps the session under Needs you until the device clears it`, async () => {
    const w = await world()
    const rootId = await w.root()
    await w.host.claim({ sessionId: rootId })
    const wall = {
      kind: `rate_limit`,
      agent: `claude`,
      window: `weekly`,
      resetsAt: null,
      since: `2026-10-21T10:00:00.000Z`,
    } as const
    await w.host.report({ sessionId: rootId, blocked: wall })
    expect(await row(rootId)).toMatchObject({ blocked: wall, needsInput: true })
    // A walled row is no watchdog business: never re-flagged as stuck.
    await w.host.report({ sessionId: rootId, busy: true })
    await backdateSession(rootId, `updated_at`, ago((watchdog.stuckMinutes + 1) * MIN))
    await runWatchdog(NOW)
    expect((await row(rootId)).blocked).toEqual(wall)
    await w.host.report({ sessionId: rootId, blocked: null })
    expect(await row(rootId)).toMatchObject({ blocked: null, needsInput: false })
  })
})
