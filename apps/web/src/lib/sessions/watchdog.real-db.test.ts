// @vitest-environment node
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
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
const {
  BACKGROUND_STUCK_MINUTES,
  runWatchdog,
  restoreDeviceSessionsInTx,
  clearLiveStuckInTx,
  recordSessionActivityInTx,
  touchSessionActivity,
} = await import(`@/lib/sessions/watchdog`)
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
/** EXP-1374: `updated_at` backdates the session's sign of life with it
 *  (`last_activity_at`, which every report stamps at the real now). */
const backdateSession = (id: string, column: `created_at` | `updated_at`, at: Date) =>
  backdate(() =>
    column === `updated_at`
      ? sql`UPDATE sessions SET updated_at = ${at.toISOString()}, last_activity_at = ${at.toISOString()} WHERE id = ${id}`
      : sql`UPDATE sessions SET ${sql.raw(column)} = ${at.toISOString()} WHERE id = ${id}`
  )
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
    await w.host.claim({ sessionId: late, deviceId: w.deviceId })
    expect(await row(late)).toMatchObject({ status: `running`, blocked: null, needsInput: false })
  })

  it(`nudges the host device again for a live session's input pending past nudgeMinutes`, async () => {
    const w = await world({ shared: true })
    // A teammate's start on the owner's shared box: the owner hosts it.
    const rootId = await w.root(`hosted`, w.mate)
    await w.host.claim({ sessionId: rootId, deviceId: w.deviceId })
    const waiting = await sendSession({ userId: w.mate, input: { sessionId: rootId, text: `are you there?` } })
    const fresh = await w.root(`fresh`, w.mate)
    await w.host.claim({ sessionId: fresh, deviceId: w.deviceId })
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
      await w.host.claim({ sessionId: id, deviceId: w.deviceId })
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
    // EXP-1374: how long, and a check, never "stop it".
    expect(await lines(rootId, `session_stuck`, stuck)).toMatchObject([
      {
        authorKind: `system`,
        authorSessionId: stuck,
        consumedAt: null,
        body: BUS_COPY.session_stuck(watchdog.stuckMinutes + 1),
      },
    ])
    expect(BUS_COPY.session_stuck(31)).not.toMatch(/stop|end it/i)
    await runWatchdog(NOW)
    expect(await lines(rootId, `session_stuck`, stuck)).toHaveLength(1)

    // Any report = activity: the flag and Needs you clear.
    await w.host.report({ sessionId: stuck, busy: true })
    expect(await row(stuck)).toMatchObject({ blocked: null, needsInput: false, busy: true })
  })

  it(`a host device silent past the online window flags its BUSY sessions (S-3: idle ones untouched, silent); its heartbeat restores them`, async () => {
    const w = await world({ shared: true })
    const rootId = await w.root()
    const childId = await w.child(rootId, `working`)
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    await w.host.report({ sessionId: childId, busy: true })
    // A teammate's session hosted on the same shared box, waiting on its own card.
    const hosted = await w.root(`hosted`, w.mate)
    await w.host.claim({ sessionId: hosted, deviceId: w.deviceId })
    await postAskCard({
      userId: w.mate,
      sessionId: hosted,
      card: buildQuestionCard({ question: `Which one?`, options: [{ label: `a` }, { label: `b` }] }),
    })
    // A queued session is not live: the offline sweep leaves it alone.
    const queued = await w.child(rootId, `not yet`)
    await backdateDevice(w.deviceId, ago((contract.device.onlineWindowSeconds + 1) * 1000))

    const run = await runWatchdog(NOW)
    expect(run.offline).toBeGreaterThanOrEqual(1)
    // EXP-1374: an interruption, never on a person; the busy one was
    // mid-turn (`resume`). S-3: an idle one (its open card still waits) is
    // never flagged.
    expect(await row(childId)).toMatchObject({
      status: `idle`,
      busy: false,
      needsInput: false,
      blocked: { kind: `device_offline`, since: NOW.toISOString(), resume: true, resumeSince: null },
    })
    expect(await row(hosted)).toMatchObject({ busy: false, needsInput: true, blocked: null })
    expect(await row(queued)).toMatchObject({ status: `queued`, blocked: null })
    // S-3: silent, no bus line.
    await runWatchdog(NOW)
    expect(await lines(rootId, `device_offline`, childId)).toHaveLength(0)

    // The heartbeat (the OWNER's device row) restores the flagged one.
    const restored = await db.transaction((tx) =>
      restoreDeviceSessionsInTx(tx, { deviceId: w.deviceId, userId: w.owner })
    )
    expect(restored).toBe(1)
    expect(await row(childId)).toMatchObject({ status: `idle`, blocked: null, needsInput: false })
    expect(await row(hosted)).toMatchObject({ blocked: null, needsInput: true })
    // Another account's heartbeat for an unrelated device restores nothing.
    await w.host.report({ sessionId: childId, busy: true })
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
    for (const id of [dead, alive, between, mates]) await w.host.claim({ sessionId: id, deviceId: w.deviceId })
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
    // EXP-1374: an interruption (the runner restarted mid-turn): its device
    // continues it, never a person's.
    for (const id of [dead, mates]) {
      expect(await row(id)).toMatchObject({
        status: `idle`,
        busy: false,
        needsInput: false,
        blocked: { kind: `restarted`, resume: true },
      })
    }
    expect(await row(alive)).toMatchObject({ status: `running`, busy: true, blocked: null, needsInput: false })
    expect(await row(between)).toMatchObject({ status: `running`, busy: false, blocked: null })
    // S-3: silent, the parent is not told.
    await beat([alive])
    expect(await lines(rootId, `session_interrupted`, dead)).toHaveLength(0)

    // The device continuing it reports: the flag clears.
    await w.host.report({ sessionId: dead, status: `running`, busy: true })
    expect(await row(dead)).toMatchObject({ blocked: null, needsInput: false, busy: true })
  })

  it(`FEED-145: a long turn its online host lists live is never flagged stuck, so a busy row never reads needs input`, async () => {
    const w = await world()
    const rootId = await w.root()
    const long = await w.child(rootId, `long turn, subagents and pictures`)
    const unlisted = await w.child(rootId, `not in the beat`)
    for (const id of [long, unlisted]) {
      await w.host.claim({ sessionId: id, deviceId: w.deviceId })
      await w.host.report({ sessionId: id, busy: true })
    }
    // Non-asking cards mid-turn are no input to wait on.
    await w.host.reply({ sessionId: long, card: { kind: `subagent`, title: `Explore`, status: `running` } })
    await w.host.reply({
      sessionId: long,
      card: { kind: `picture`, attachmentId: crypto.randomUUID(), width: null, height: null },
    })
    expect(await row(long)).toMatchObject({ busy: true, needsInput: false })
    await devicesRouter
      .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
      .heartbeat({ deviceId: w.deviceId, activeSessions: 2, defaultsSyncedAt: null, liveSessionIds: [long, unlisted] })
    for (const id of [long, unlisted]) {
      await backdateSession(id, `updated_at`, ago((watchdog.stuckMinutes + 1) * MIN))
    }
    await backdateDevice(w.deviceId, NOW)
    // Only `long` stays listed on the device row.
    await db.update(devices).set({ liveSessionIds: [long] }).where(eq(devices.deviceId, w.deviceId))

    await runWatchdog(NOW)
    expect(await row(long)).toMatchObject({ status: `running`, busy: true, needsInput: false, blocked: null })
    expect(await lines(rootId, `session_stuck`, long)).toEqual([])
    // A session the claim does not name keeps the 30-minute rule.
    expect(await row(unlisted)).toMatchObject({ busy: true, needsInput: true, blocked: { kind: `stuck` } })

    // A host past the online window vouches for nothing.
    const later = await w.child(rootId, `host went quiet`)
    await w.host.claim({ sessionId: later, deviceId: w.deviceId })
    await w.host.report({ sessionId: later, busy: true })
    await db.update(devices).set({ liveSessionIds: [later] }).where(eq(devices.deviceId, w.deviceId))
    await backdateSession(later, `updated_at`, ago((watchdog.stuckMinutes + 1) * MIN))
    await backdateDevice(w.deviceId, ago((contract.device.onlineWindowSeconds + 1) * 1000))
    await runWatchdog(NOW)
    expect((await row(later)).blocked).not.toBeNull()
  })

  it(`FEED-145: a heartbeat listing a stuck session clears the flag; needs input re-derives from its open cards`, async () => {
    const w = await world()
    const rootId = await w.root()
    const working = await w.child(rootId, `working`)
    const asking = await w.child(rootId, `asking`)
    for (const id of [working, asking]) {
      await w.host.claim({ sessionId: id, deviceId: w.deviceId })
      await w.host.report({ sessionId: id, busy: true })
      await backdateSession(id, `updated_at`, ago((watchdog.stuckMinutes + 1) * MIN))
    }
    await runWatchdog(NOW)
    for (const id of [working, asking]) {
      expect(await row(id)).toMatchObject({ busy: true, needsInput: true, blocked: { kind: `stuck` } })
    }
    // `asking` has a question open: it still waits once the flag goes.
    await w.host.reply({
      sessionId: asking,
      card: { kind: `question`, question: `Which one?`, options: [`a`, `b`] },
    })

    await devicesRouter
      .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
      .heartbeat({ deviceId: w.deviceId, activeSessions: 2, defaultsSyncedAt: null, liveSessionIds: [working, asking] })
    expect(await row(working)).toMatchObject({ status: `running`, busy: true, needsInput: false, blocked: null })
    expect(await row(asking)).toMatchObject({ busy: true, needsInput: true, blocked: null })
    const [device] = await db.select().from(devices).where(eq(devices.deviceId, w.deviceId))
    expect(device!.liveSessionIds).toEqual([working, asking])
  })

  it(`FEED-147: another account registering the same deviceId moves none of the host's sessions`, async () => {
    const w = await world({ shared: true })
    const rootId = await w.root()
    const busy = await w.child(rootId, `busy`)
    const stuck = await w.child(rootId, `stuck`)
    const offline = await w.child(rootId, `offline`)
    for (const id of [busy, stuck, offline]) {
      await w.host.claim({ sessionId: id, deviceId: w.deviceId })
      await w.host.report({ sessionId: id, busy: true })
    }
    const stuckFlag = { kind: `stuck`, agent: null, window: null, resetsAt: null, since: NOW.toISOString() }
    const offlineFlag = { ...stuckFlag, kind: `device_offline` }
    await db.update(sessions).set({ blocked: stuckFlag, needsInput: true }).where(eq(sessions.id, stuck))
    await db
      .update(sessions)
      .set({ status: `idle`, busy: false, blocked: offlineFlag, needsInput: true })
      .where(eq(sessions.id, offline))
    const before = await Promise.all([busy, stuck, offline].map(row))

    // A stranger, and a teammate sharing the very team, each register the
    // owner's machine id: their rows exist, so their beats are accepted.
    const stranger = await user()
    for (const attacker of [stranger, w.mate]) {
      await db.insert(devices).values({
        userId: attacker,
        deviceId: w.deviceId,
        label: `not yours`,
        kind: `server`,
        sharedTeamIds: attacker === w.mate ? [w.teamId] : [],
      })
      const beat = (liveSessionIds: string[]) =>
        devicesRouter
          .createCaller({ session: { user: { id: attacker } }, db, request: new Request(`http://x.test`) } as never)
          .heartbeat({ deviceId: w.deviceId, activeSessions: 0, defaultsSyncedAt: null, liveSessionIds })
      // Lists nothing (would flag every busy row dead), then lists them
      // all (would clear `stuck`); restore would clear `device_offline`.
      expect(await beat([])).toMatchObject({ ok: true })
      expect(await beat([busy, stuck, offline])).toMatchObject({ ok: true })
      await db.transaction(async (tx) => {
        expect(await restoreDeviceSessionsInTx(tx, { deviceId: w.deviceId, userId: attacker })).toBe(0)
        expect(
          await clearLiveStuckInTx(tx, { deviceId: w.deviceId, userId: attacker, liveSessionIds: [stuck] })
        ).toBe(0)
      })
    }
    expect(await Promise.all([busy, stuck, offline].map(row))).toEqual(before)
    expect(await lines(rootId, `session_stuck`, busy)).toEqual([])

    // The owner's own beat still speaks for them.
    await devicesRouter
      .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
      .heartbeat({ deviceId: w.deviceId, activeSessions: 1, defaultsSyncedAt: null, liveSessionIds: [stuck] })
    expect(await row(busy)).toMatchObject({ busy: false, blocked: { kind: `restarted` } })
    expect(await row(stuck)).toMatchObject({ blocked: null })
    expect(await row(offline)).toMatchObject({ blocked: null })
  })

  it(`FEED-148: a teammate's online decoy row listing the box's sessions keeps none of them from being flagged stuck`, async () => {
    const w = await world({ shared: true })
    const hosted = await w.root(`hosted`, w.mate)
    await w.host.claim({ sessionId: hosted, deviceId: w.deviceId })
    await w.host.report({ sessionId: hosted, busy: true })
    // The host is online but runs nothing; a teammate's later sharing row
    // for the same machine id (from before the one-sharer rule) is online
    // and lists the session.
    await db.update(devices).set({ liveSessionIds: [] }).where(and(eq(devices.deviceId, w.deviceId), eq(devices.userId, w.owner)))
    await backdateDevice(w.deviceId, NOW)
    const decoy = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: decoy, role: `member` })
    await db.insert(devices).values({
      userId: decoy,
      deviceId: w.deviceId,
      label: `decoy`,
      kind: `server`,
      sharedTeamIds: [w.teamId],
      lastSeenAt: NOW,
      liveSessionIds: [hosted],
    })
    await backdateSession(hosted, `updated_at`, ago((watchdog.stuckMinutes + 1) * MIN))

    await runWatchdog(NOW)
    expect(await row(hosted)).toMatchObject({ busy: true, needsInput: true, blocked: { kind: `stuck` } })
  })

  it(`FEED-148: a teammate's online decoy row keeps none of an offline host's sessions live`, async () => {
    const w = await world({ shared: true })
    const hosted = await w.root(`hosted`, w.mate)
    await w.host.claim({ sessionId: hosted, deviceId: w.deviceId })
    await w.host.report({ sessionId: hosted, busy: true })
    await backdateDevice(w.deviceId, ago((contract.device.onlineWindowSeconds + 1) * 1000))
    const decoy = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: decoy, role: `member` })
    await db.insert(devices).values({
      userId: decoy,
      deviceId: w.deviceId,
      label: `decoy`,
      kind: `server`,
      sharedTeamIds: [w.teamId],
      lastSeenAt: NOW,
    })

    await runWatchdog(NOW)
    expect(await row(hosted)).toMatchObject({ status: `idle`, busy: false, blocked: { kind: `device_offline` } })
  })

  it(`FEED-146: malformed liveSessionIds are dropped before the save and the dead-process check; the beat never fails`, async () => {
    const w = await world()
    const rootId = await w.root()
    const listed = await w.child(rootId, `listed in caps`)
    const unlisted = await w.child(rootId, `not listed`)
    for (const id of [listed, unlisted]) {
      await w.host.claim({ sessionId: id, deviceId: w.deviceId })
      await w.host.report({ sessionId: id, busy: true })
    }
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    const result = await devicesRouter
      .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
      .heartbeat({
        deviceId: w.deviceId,
        activeSessions: 1,
        defaultsSyncedAt: null,
        liveSessionIds: [`not-a-uuid`, ``, listed.toUpperCase(), listed, 42, `x`.repeat(5000)] as never,
      })
    expect(result).toMatchObject({ ok: true })
    expect(warn).toHaveBeenCalledWith(expect.stringContaining(`dropped 4 invalid liveSessionIds`))
    warn.mockRestore()
    const [device] = await db.select().from(devices).where(eq(devices.deviceId, w.deviceId))
    expect(device!.liveSessionIds).toEqual([listed])
    expect(await row(listed)).toMatchObject({ busy: true, blocked: null })
    expect(await row(unlisted)).toMatchObject({ busy: false, blocked: { kind: `restarted` } })

    // Past the count cap: a 400, not a 500, and nothing written.
    await expect(
      devicesRouter
        .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
        .heartbeat({
          deviceId: w.deviceId,
          activeSessions: 1,
          defaultsSyncedAt: null,
          liveSessionIds: Array.from({ length: 1001 }, () => crypto.randomUUID()),
        })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`FEED-146: a beat without liveSessionIds stores NULL, so a stale list never vouches for a session`, async () => {
    const w = await world()
    const rootId = await w.root()
    const long = await w.child(rootId, `was listed`)
    await w.host.claim({ sessionId: long, deviceId: w.deviceId })
    await w.host.report({ sessionId: long, busy: true })
    const beat = (liveSessionIds?: string[]) =>
      devicesRouter
        .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
        .heartbeat({ deviceId: w.deviceId, activeSessions: 1, defaultsSyncedAt: null, ...(liveSessionIds ? { liveSessionIds } : {}) })
    await beat([long])
    expect((await db.select().from(devices).where(eq(devices.deviceId, w.deviceId)))[0]!.liveSessionIds).toEqual([long])
    // The device went past its cap: no claim this beat.
    await beat()
    expect((await db.select().from(devices).where(eq(devices.deviceId, w.deviceId)))[0]!.liveSessionIds).toBeNull()

    await backdateSession(long, `updated_at`, ago((watchdog.stuckMinutes + 1) * MIN))
    await backdateDevice(w.deviceId, NOW)
    await runWatchdog(NOW)
    expect(await row(long)).toMatchObject({ blocked: { kind: `stuck` } })
  })

  it(`FEED-146: a usage wall written while a beat clears stuck / device_offline survives`, async () => {
    const w = await world()
    const rootId = await w.root()
    const stuck = await w.child(rootId, `stuck`)
    const offline = await w.child(rootId, `offline`)
    for (const id of [stuck, offline]) await w.host.claim({ sessionId: id, deviceId: w.deviceId })
    const base = { agent: null, window: null, resetsAt: null, since: NOW.toISOString() }
    await db.update(sessions).set({ blocked: { kind: `stuck`, ...base }, needsInput: true }).where(eq(sessions.id, stuck))
    await db
      .update(sessions)
      .set({ blocked: { kind: `device_offline`, ...base }, needsInput: true })
      .where(eq(sessions.id, offline))
    const wall = { kind: `rate_limit`, agent: `claude`, window: `weekly`, resetsAt: null, since: NOW.toISOString() } as const

    // The device's wall lands in a transaction still open while the beat runs.
    await db.transaction(async (wallTx) => {
      await wallTx.update(sessions).set({ blocked: wall }).where(inArray(sessions.id, [stuck, offline]))
      const beat = devicesRouter
        .createCaller({ session: { user: { id: w.owner } }, db, request: new Request(`http://x.test`) } as never)
        .heartbeat({ deviceId: w.deviceId, activeSessions: 2, defaultsSyncedAt: null, liveSessionIds: [stuck, offline] })
      await new Promise((resolve) => setTimeout(resolve, 300))
      return beat
    })
    for (const id of [stuck, offline]) expect((await row(id)).blocked).toEqual(wall)

    // The guarded clear alone never wipes another kind either.
    await db.transaction(async (tx) => {
      expect(await restoreDeviceSessionsInTx(tx, { deviceId: w.deviceId, userId: w.owner })).toBe(0)
      expect(
        await clearLiveStuckInTx(tx, { deviceId: w.deviceId, userId: w.owner, liveSessionIds: [stuck] })
      ).toBe(0)
    })
    expect((await row(stuck)).blocked).toEqual(wall)
  })

  it(`a usage wall keeps the session under Needs you until the device clears it`, async () => {
    const w = await world()
    const rootId = await w.root()
    await w.host.claim({ sessionId: rootId, deviceId: w.deviceId })
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

  // FEED-129: a walled run whose engine went away before the reset (a
  // failed account switch) had nobody to clear the wall; the row read
  // "Usage limit" hours after the window reopened.
  it(`a usage wall whose reset passed is cleared by the watchdog; one still ahead, or naming none, waits for the device`, async () => {
    const w = await world()
    const past = await w.root(`past`)
    const ahead = await w.root(`ahead`)
    const unset = await w.root(`unset`)
    for (const id of [past, ahead, unset]) await w.host.claim({ sessionId: id, deviceId: w.deviceId })
    const wall = (resetsAt: string | null) =>
      ({ kind: `rate_limit`, agent: `claude`, window: `session`, resetsAt, since: ago(2 * MIN).toISOString() }) as const
    await w.host.report({ sessionId: past, blocked: wall(ago(MIN).toISOString()) })
    await w.host.report({ sessionId: ahead, blocked: wall(new Date(NOW.getTime() + MIN).toISOString()) })
    await w.host.report({ sessionId: unset, blocked: wall(null) })
    for (const id of [past, ahead, unset]) expect(await row(id)).toMatchObject({ needsInput: true })
    // The bus said the wall ONCE when the device reported it.
    const said = (await lines(past, `session_blocked`, past)).length
    expect(await runWatchdog(NOW)).toMatchObject({ expired: 1 })
    expect(await row(past)).toMatchObject({ blocked: null, needsInput: false })
    expect((await row(ahead)).blocked).toMatchObject({ kind: `rate_limit` })
    expect((await row(unset)).blocked).toMatchObject({ kind: `rate_limit` })
    // Silent: its passing is no line.
    expect(await lines(past, `session_blocked`, past)).toHaveLength(said)
  })

  // EXP-1374 (FEED-141): only a message counted as a sign of life; a gate
  // run that pushed and ran tools for 13 silent minutes read "stuck, stop it".
  it(`EXP-1374: the stuck rule measures the last sign of life (heartbeat activity, an MCP call), never a background run`, async () => {
    const w = await world()
    const rootId = await w.root()
    const [active, mcp, background, silent] = await Promise.all(
      [`tool calls`, `mcp calls`, `background shell`, `silent`].map((text) => w.child(rootId, text))
    )
    for (const id of [active, mcp, background, silent]) {
      await w.host.claim({ sessionId: id, deviceId: w.deviceId })
      await w.host.report({ sessionId: id, busy: true })
      await backdateSession(id, `updated_at`, ago((watchdog.stuckMinutes + 5) * MIN))
    }
    await w.host.report({ sessionId: background, busy: true, background: true })
    await backdateSession(background, `updated_at`, ago((watchdog.stuckMinutes + 5) * MIN))
    expect(await row(background)).toMatchObject({ busy: true, background: true })
    // An older device: no liveSessionIds claim, but its engine's activity.
    await db.transaction((tx) =>
      recordSessionActivityInTx(
        tx,
        { deviceId: w.deviceId, userId: w.owner, activity: [{ sessionId: active, at: ago(2 * MIN).toISOString() }] },
        NOW
      )
    )
    await touchSessionActivity(eq(sessions.id, mcp), ago(MIN))

    await runWatchdog(NOW)
    for (const id of [active, mcp, background]) {
      expect(await row(id)).toMatchObject({ busy: true, blocked: null, needsInput: false })
    }
    expect(await row(silent)).toMatchObject({ busy: true, needsInput: true, blocked: { kind: `stuck` } })
    const [line] = await lines(rootId, `session_stuck`, silent)
    expect(line!.body).toBe(BUS_COPY.session_stuck(watchdog.stuckMinutes + 5))
    expect(line!.body).toMatch(/^No sign of life for 35 minutes/)

    // Activity never runs ahead of the beat, nor backwards.
    await db.transaction((tx) =>
      recordSessionActivityInTx(
        tx,
        {
          deviceId: w.deviceId,
          userId: w.owner,
          activity: [
            { sessionId: active, at: new Date(NOW.getTime() + 60 * MIN).toISOString() },
            { sessionId: mcp, at: ago(50 * MIN).toISOString() },
            { sessionId: `not-a-uuid`, at: `garbage` },
          ],
        },
        NOW
      )
    )
    expect((await row(active)).lastActivityAt).toEqual(NOW)
    expect((await row(mcp)).lastActivityAt).toEqual(ago(MIN))
    // A not-busy report drops `background` with it.
    await w.host.report({ sessionId: background, busy: false })
    expect(await row(background)).toMatchObject({ busy: false, background: false })
  })

  // The exemption has a cap: the flag is written on a state edge and nothing
  // resets it when the run dies with its tasks, so past BACKGROUND_STUCK_MINUTES
  // a background row is measured like any busy one.
  it(`a background run with no sign of life for BACKGROUND_STUCK_MINUTES is stuck like any other`, async () => {
    const w = await world()
    const rootId = await w.root()
    const [fresh, stale] = await Promise.all(
      [`background fresh`, `background stale`].map((text) => w.child(rootId, text))
    )
    for (const id of [fresh, stale]) {
      await w.host.claim({ sessionId: id, deviceId: w.deviceId })
      await w.host.report({ sessionId: id, busy: true, background: true })
    }
    await backdateSession(fresh, `updated_at`, ago((BACKGROUND_STUCK_MINUTES - 1) * MIN))
    await backdateSession(stale, `updated_at`, ago((BACKGROUND_STUCK_MINUTES + 1) * MIN))
    await runWatchdog(NOW)
    expect(await row(fresh)).toMatchObject({ busy: true, background: true, blocked: null, needsInput: false })
    expect(await row(stale)).toMatchObject({ busy: true, background: true, needsInput: true, blocked: { kind: `stuck` } })
    const [line] = await lines(rootId, `session_stuck`, stale)
    expect(line!.body).toBe(BUS_COPY.session_stuck(BACKGROUND_STUCK_MINUTES + 1))
    // Ending it drops `background` with `busy`.
    await w.host.report({ sessionId: stale, status: `ended` })
    expect(await row(stale)).toMatchObject({ status: `ended`, busy: false, background: false })
  })

  it(`EXP-1374: the device back continues an interrupted run; a listed one is busy again; one left alone is on a person after resumeMinutes`, async () => {
    const w = await world()
    const rootId = await w.root()
    const [survived, interrupted, idle] = await Promise.all(
      [`process survived`, `process died`, `between turns`].map((text) => w.child(rootId, text))
    )
    for (const id of [survived, interrupted, idle]) await w.host.claim({ sessionId: id, deviceId: w.deviceId })
    for (const id of [survived, interrupted]) await w.host.report({ sessionId: id, busy: true })
    await backdateDevice(w.deviceId, ago((contract.device.onlineWindowSeconds + 1) * 1000))
    await runWatchdog(NOW)
    expect(await row(interrupted)).toMatchObject({ blocked: { kind: `device_offline`, resume: true }, needsInput: false })
    // S-3: an idle session is never flagged.
    expect(await row(idle)).toMatchObject({ blocked: null })

    const back = ago(10 * MIN)
    await db.transaction((tx) =>
      restoreDeviceSessionsInTx(tx, { deviceId: w.deviceId, userId: w.owner, liveSessionIds: [survived] }, back)
    )
    expect(await row(survived)).toMatchObject({ status: `running`, busy: true, blocked: null, needsInput: false })
    expect(await row(idle)).toMatchObject({ busy: false, blocked: null })
    // Still interrupted, waiting for its device since it came back.
    expect(await row(interrupted)).toMatchObject({
      status: `idle`,
      needsInput: false,
      blocked: { kind: `device_offline`, resume: true, resumeSince: back.toISOString() },
    })
    // The next beat keeps the first stamp.
    await db.transaction((tx) =>
      restoreDeviceSessionsInTx(tx, { deviceId: w.deviceId, userId: w.owner, liveSessionIds: [survived] }, NOW)
    )
    expect((await row(interrupted)).blocked).toMatchObject({ resumeSince: back.toISOString() })

    // Nobody continued it within resumeMinutes: Needs you, said once (waking).
    const run = await runWatchdog(NOW)
    expect(run.unresumed).toBeGreaterThanOrEqual(1)
    expect(await row(interrupted)).toMatchObject({ needsInput: true, blocked: { kind: `device_offline` } })
    const said = await lines(rootId, `session_interrupted`, interrupted)
    expect(said).toMatchObject([{ body: BUS_COPY.session_unresumed, consumedAt: null }])
    await runWatchdog(NOW)
    expect(await lines(rootId, `session_interrupted`, interrupted)).toHaveLength(1)

    // Its device continuing it reports: the flag and Needs you clear.
    await w.host.report({ sessionId: interrupted, status: `running`, busy: true })
    expect(await row(interrupted)).toMatchObject({ status: `running`, busy: true, blocked: null, needsInput: false })
  })

  it(`NEW-17: a device reporting a run's process LOST mid-turn interrupts it (never an end, never "done"); an older device's end while running reads the same`, async () => {
    const w = await world()
    const rootId = await w.root()
    const killed = await w.child(rootId, `OOM killed`)
    const older = await w.child(rootId, `older device`)
    const refused = await w.child(rootId, `never ran`)
    const finished = await w.child(rootId, `finished`)
    for (const id of [rootId, killed, older, refused, finished]) await w.host.claim({ sessionId: id, deviceId: w.deviceId })
    await w.host.report({ sessionId: killed, busy: true })
    await w.host.report({ sessionId: killed, deviceId: w.deviceId, status: `ended`, lost: `process_lost` })
    expect(await row(killed)).toMatchObject({
      status: `idle`,
      busy: false,
      needsInput: false,
      endedAt: null,
      blocked: { kind: `restarted`, resume: true },
    })
    // S-3: silent, its device continues it.
    expect(await lines(rootId, `session_interrupted`, killed)).toHaveLength(0)

    // An older device sends no `lost`: its end report while a turn is open
    // (running, after a turn ran) is the same interruption.
    await w.host.report({ sessionId: older, busy: true })
    await w.host.report({ sessionId: older, busy: false })
    await w.host.report({ sessionId: older, status: `ended` })
    expect(await row(older)).toMatchObject({ status: `idle`, blocked: { kind: `restarted` } })
    // A launch that never reached a turn still ends; so does a finished one.
    await w.host.report({ sessionId: refused, status: `ended` })
    expect(await row(refused)).toMatchObject({ status: `ended` })
    await w.host.report({ sessionId: finished, status: `idle` })
    await w.host.report({ sessionId: finished, status: `ended` })
    expect(await row(finished)).toMatchObject({ status: `ended` })

    // A daemon restart under an idle session: nothing was interrupted.
    await w.host.report({ sessionId: rootId, status: `idle` })
    await w.host.report({ sessionId: rootId, lost: `daemon_restart` })
    expect(await row(rootId)).toMatchObject({ status: `idle`, blocked: null })
    expect(await lines(rootId, `session_interrupted`, rootId)).toHaveLength(0)

    // A late status tick already read the row idle: the device's own
    // `midTurn` still says a turn was open.
    const late = await w.child(rootId, `late tick`)
    await w.host.claim({ sessionId: late, deviceId: w.deviceId })
    await w.host.report({ sessionId: late, busy: true })
    await w.host.report({ sessionId: late, status: `idle`, busy: false })
    await w.host.report({ sessionId: late, status: `ended`, lost: `process_lost`, midTurn: true })
    expect(await row(late)).toMatchObject({ status: `idle`, endedAt: null, blocked: { kind: `restarted` } })

    // The device's explicit `midTurn: false` wins over a running row, and
    // the lost path clears a Stop's pending settle.
    const between = await w.child(rootId, `between turns`)
    await w.host.claim({ sessionId: between, deviceId: w.deviceId })
    await w.host.report({ sessionId: between, status: `running`, busy: true })
    await backdate(() => sql`UPDATE sessions SET stopped_at = now() WHERE id = ${between}`)
    await w.host.report({ sessionId: between, status: `ended`, lost: `process_lost`, midTurn: false })
    expect(await row(between)).toMatchObject({ status: `idle`, endedAt: null, blocked: null, stoppedAt: null })
    expect(await lines(rootId, `session_interrupted`, between)).toHaveLength(0)

    // FEED-160: a deliberate end (`end_session`: `midTurn: false`, no
    // `lost`) on a running row that ran a turn ENDS: never guessed lost.
    const deliberate = await w.child(rootId, `deliberate end`)
    await w.host.claim({ sessionId: deliberate, deviceId: w.deviceId })
    await w.host.report({ sessionId: deliberate, status: `running`, busy: true })
    await w.host.report({ sessionId: deliberate, busy: false })
    await w.host.report({ sessionId: deliberate, status: `ended`, midTurn: false })
    expect(await row(deliberate)).toMatchObject({ status: `ended`, blocked: null })
    expect(await lines(rootId, `session_interrupted`, deliberate)).toHaveLength(0)

    // Its device continuing it reports: the flag clears.
    await w.host.report({ sessionId: killed, status: `running`, busy: true })
    expect(await row(killed)).toMatchObject({ busy: true, blocked: null })
  })

  it(`FEED-153: a claimed first message stays delivered: a first turn that ended with an empty reply (no agent message) is never re-run by the watchdog`, async () => {
    // The device owns FEED-153's retries (pickup's unstarted reclaim, the
    // seed prompt's one retry): the server cannot tell a turn that never ran
    // from one that did real work and ended silently.
    const w = await world()
    const silent = await w.root(`ended silently`)
    await w.host.claim({ sessionId: silent, deviceId: w.deviceId })
    const brief = async () =>
      (await db.select().from(messages).where(and(eq(messages.sessionId, silent), eq(messages.authorKind, `user`))))[0]!
    await w.host.report({ sessionId: silent, busy: true })
    await w.host.report({ sessionId: silent, status: `idle`, busy: false })
    const at = ago((watchdog.nudgeMinutes + 5) * MIN).toISOString()
    await backdate(
      () => sql`UPDATE sessions SET status = 'idle', busy = false, started_at = ${at}, updated_at = ${at} WHERE id = ${silent}`
    )
    await backdate(() => sql`UPDATE messages SET consumed_at = ${at} WHERE session_id = ${silent} AND author_kind = 'user'`)

    relay.on = true
    await runWatchdog(NOW)
    expect((await brief()).consumedAt).not.toBeNull()
    expect(relay.nudges).not.toContainEqual({ userId: w.owner, deviceId: w.deviceId })
  })
})
