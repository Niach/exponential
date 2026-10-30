// @vitest-environment node
import { generateKeyPairSync, sign } from "node:crypto"
import { readFileSync } from "node:fs"
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, deviceFirstSeen, deviceKeys, deviceRemovals, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"
import { deviceKeyMessage } from "@/lib/device-keys"
import { inSession } from "@/lib/sessions/messages"

// FEED-148: `devices.device_id` is unique per ACCOUNT only, so anyone can
// register a shared box's machine id. Every path that acts for "the host"
// (account deletion, unshare, leave, the agent disarm, the start, the
// device procedures) must mean the RESOLVED host (`lib/sessions/host.ts`),
// never "an account with a row naming the machine". Each case is an
// attacker against the REAL migrated Postgres, then the legitimate flow on
// the same world. The watchdog's decoy cases live in watchdog.real-db.
// These helpers open their own transactions, so each case builds a fresh
// team and the suite deletes every team (and user) it made (cascade).

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
// FEED-154: the device owner's notice, as the router hands it over.
const notices = vi.hoisted(() => ({ calls: [] as Array<Record<string, unknown>> }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
  notifyDeviceShareRemoved: async (args: Record<string, unknown>) => {
    notices.calls.push(args)
  },
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
}))
// FEED-149: a hook that runs between an endpoint's commit and its
// `endHostedSessions` (the race window), then the real end.
const endHook = vi.hoisted(() => ({ before: null as null | ((ids: string[]) => Promise<void>) }))
vi.mock(`@/lib/sessions/lifecycle`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/sessions/lifecycle")>()
  return {
    ...actual,
    endHostedSessions: async (ids: string[], reason?: Parameters<typeof actual.endHostedSessions>[1]) => {
      const before = endHook.before
      endHook.before = null
      if (before) await before(ids)
      return actual.endHostedSessions(ids, reason)
    },
  }
})

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'root_id')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[host-spoofing.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { assertDeviceHost, lockDeviceShares, resolveSessionHost } = await import(`@/lib/sessions/host`)
const { guardAndCleanupTeamsForUserDeletion } = await import(`@/lib/account-deletion`)
const { devicesRouter } = await import(`@/lib/trpc/devices`)
const { teamMembersRouter } = await import(`@/lib/trpc/team-members`)
const { adminRouter } = await import(`@/lib/trpc/admin`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[], devices: [] as string[] }

/** `prefix` orders the id: `0-` sorts below every `host-test-` account. */
async function user(prefix = `host-test-`): Promise<string> {
  const id = `${prefix}${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@host.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const ctx = (userId: string) =>
  ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const devicesAs = (userId: string) => devicesRouter.createCaller(ctx(userId))
const adminAs = (userId: string) => adminRouter.createCaller(ctx(userId))

/** FEED-154: a machine's ed25519 key, as the CLI daemon keeps it. */
function machineKey() {
  const { publicKey, privateKey } = generateKeyPairSync(`ed25519`)
  const hex = Buffer.from(publicKey.export({ format: `jwk` }).x!, `base64url`).toString(`hex`)
  return {
    hex,
    prove: (userId: string, deviceId: string) => {
      const signedAt = new Date().toISOString()
      const message = Buffer.from(deviceKeyMessage(userId, deviceId, hex, signedAt))
      return { publicKey: hex, signedAt, signature: sign(null, message, privateKey).toString(`hex`) }
    },
  }
}
const membersAs = (userId: string) => teamMembersRouter.createCaller(ctx(userId))

/** A team led by `lead` (its owner); `boxOwner`, a member, shares the box
 *  `deviceId` with it; `mate` starts a session there (hosted by boxOwner). */
async function world() {
  const lead = await user()
  const boxOwner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Host`, slug: `host-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: lead, role: `owner` },
    { teamId, userId: boxOwner, role: `member` },
    { teamId, userId: mate, role: `member` },
  ])
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `HS` })
  const deviceId = `dev-${id8()}`
  // FEED-154: the box owner's daemon proved the machine key and pinned it.
  const box = machineKey()
  made.devices.push(deviceId)
  await db
    .insert(devices)
    .values({ userId: boxOwner, deviceId, label: `box`, kind: `server`, sharedTeamIds: [teamId], publicKey: box.hex })
  await db.insert(deviceKeys).values({ deviceId, publicKey: box.hex, userId: boxOwner })
  await db.insert(deviceFirstSeen).values({ deviceId, userId: boxOwner })
  const hosted = (await sendSession({ userId: mate, input: { teamId, deviceId, text: `on the shared box` } })).sessionId
  /** A teammate's devices row for the box. `shares` = a row from before
   *  the one-sharer rule (a legacy duplicate): written straight to the
   *  table, later than the box owner's. */
  const intruder = async (opts: { shares: boolean; prefix?: string; team?: boolean }) => {
    const attacker = await user(opts.prefix)
    if (opts.team !== false) await db.insert(teamMembers).values({ teamId, userId: attacker, role: `member` })
    await db.insert(devices).values({
      userId: attacker,
      deviceId,
      label: `not yours`,
      kind: `server`,
      sharedTeamIds: opts.shares ? [teamId] : [],
    })
    return attacker
  }
  return { lead, boxOwner, mate, teamId, deviceId, hosted, intruder, box }
}

beforeEach(() => {
  notices.calls = []
})

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
  if (made.devices.length) {
    await db.execute(sql`DELETE FROM device_keys WHERE device_id IN ${sql.raw(`('${made.devices.join(`','`)}')`)}`)
    await db.execute(sql`DELETE FROM device_first_seen WHERE device_id IN ${sql.raw(`('${made.devices.join(`','`)}')`)}`)
  }
})

/** The server lines in a session's Conversation (FEED-154: the end reason). */
const lines = async (id: string) =>
  (
    await db
      .select({ body: messages.body })
      .from(messages)
      .where(and(inSession(id), eq(messages.authorKind, `system`)))
  ).map((row) => row.body)

const status = async (id: string) =>
  (await db.select({ status: sessions.status }).from(sessions).where(eq(sessions.id, id)).limit(1))[0]?.status

const memberRow = async (teamId: string, userId: string) =>
  (
    await db
      .select({ id: teamMembers.id })
      .from(teamMembers)
      .where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, userId)))
      .limit(1)
  )[0]!.id

describe.skipIf(!ready.ok)(`FEED-148: a registered machine id is not the host`, () => {
  it(`a stranger deleting their account ends none of the box's sessions; the box owner's deletion does`, async () => {
    const w = await world()
    const stranger = await w.intruder({ shares: false, team: false })
    await inRollback(async (tx) => {
      const { endedSessionIds } = await guardAndCleanupTeamsForUserDeletion(tx as never, stranger, `self`)
      expect(endedSessionIds).not.toContain(w.hosted)
      const [row] = await tx.select({ status: sessions.status }).from(sessions).where(eq(sessions.id, w.hosted))
      expect(row!.status).not.toBe(`ended`)
    })
    // The host's account going away ends what it ran.
    await inRollback(async (tx) => {
      const { endedSessionIds } = await guardAndCleanupTeamsForUserDeletion(tx as never, w.boxOwner, `self`)
      expect(endedSessionIds).toContain(w.hosted)
    })
  })

  it(`a teammate's duplicate share revoked ends none of the box's sessions; the box owner's unshare does`, async () => {
    const w = await world()
    const attacker = await w.intruder({ shares: true })
    await devicesAs(attacker).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    expect(await status(w.hosted)).not.toBe(`ended`)
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    expect(await status(w.hosted)).toBe(`ended`)
  })

  it(`a teammate with a duplicate share leaving the team ends none of the box's sessions; the box owner leaving does`, async () => {
    const w = await world()
    const attacker = await w.intruder({ shares: true })
    await membersAs(attacker).remove({ memberId: await memberRow(w.teamId, attacker) })
    expect(await status(w.hosted)).not.toBe(`ended`)
    await membersAs(w.boxOwner).remove({ memberId: await memberRow(w.teamId, w.boxOwner) })
    expect(await status(w.hosted)).toBe(`ended`)
  })

  it(`a teammate's duplicate share revoked disarms none of the team's agents on the box; the box owner's unshare does`, async () => {
    const w = await world()
    const [agent] = await db
      .insert(agents)
      .values({
        teamId: w.teamId,
        name: `Nightly`,
        deviceId: w.deviceId,
        ownerId: w.lead,
        triggers: [{ id: `t1`, enabled: true, kind: `schedule`, cron: `0 3 * * *` }] as never,
      })
      .returning({ id: agents.id })
    const triggers = async () =>
      (await db.select({ triggers: agents.triggers }).from(agents).where(eq(agents.id, agent!.id)))[0]!.triggers
    const attacker = await w.intruder({ shares: true })
    await devicesAs(attacker).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    expect(await triggers()).toMatchObject([{ id: `t1`, enabled: true }])
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    expect(await triggers()).toMatchObject([{ id: `t1`, enabled: false }])
  })

  it(`a lower-id account cannot share the box into the team, and its duplicate row never becomes the host`, async () => {
    const w = await world()
    // A fresh account (its id sorts below the box owner's) registers the
    // box's id and tries to share it: refused, the host stays.
    const lowId = await w.intruder({ shares: false, prefix: `0-` })
    expect(lowId < w.boxOwner).toBe(true)
    // FEED-154: it never proved the box's key, so the share is refused
    // before it could even compete (the FEED-148 CONFLICT).
    await expect(
      devicesAs(lowId).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    const [own] = await db
      .select({ sharedTeamIds: devices.sharedTeamIds })
      .from(devices)
      .where(and(eq(devices.userId, lowId), eq(devices.deviceId, w.deviceId)))
    expect(own!.sharedTeamIds).toEqual([])
    // A legacy duplicate (a lower id, shared before the rule): the first
    // sharing row hosts, for the old sessions and for new starts alike.
    const legacy = await w.intruder({ shares: true, prefix: `0-` })
    expect(legacy < w.boxOwner).toBe(true)
    const [hosted] = await db.select().from(sessions).where(eq(sessions.id, w.hosted))
    expect(await resolveSessionHost(hosted!)).toBe(w.boxOwner)
    await expect(assertDeviceHost(legacy, hosted!)).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(assertDeviceHost(w.boxOwner, hosted!)).resolves.toBeUndefined()
    const next = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `again` } })).sessionId
    const [started] = await db.select().from(sessions).where(eq(sessions.id, next))
    expect(await resolveSessionHost(started!)).toBe(w.boxOwner)
  })
})

// #EXP-1259: a start reads the box INSIDE its transaction under the machine's
// share lock (shared), so it queues behind a withdrawal holding the lock
// (`devices.setShared`, the roster leave) and then reads the box without its
// share: never a run committed onto a box whose consent is gone.
describe.skipIf(!ready.ok)(`#EXP-1259: a start waits for the share lock`, () => {
  it(`a start reaching the box under a withdrawal reads it unshared`, async () => {
    const w = await world()
    let settled = false
    let start: Promise<{ ok: boolean; code?: string }> = Promise.resolve({ ok: true })
    await db.transaction(async (tx) => {
      await lockDeviceShares(tx, w.deviceId)
      start = sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `late start` } }).then(
        () => ({ ok: true }),
        (e: { code?: string }) => ({ ok: false, code: e.code })
      )
      void start.finally(() => {
        settled = true
      })
      await new Promise((resolve) => setTimeout(resolve, 300))
      // Parked on the lock, not committed.
      expect(settled).toBe(false)
      await tx
        .update(devices)
        .set({ sharedTeamIds: [] })
        .where(and(eq(devices.userId, w.boxOwner), eq(devices.deviceId, w.deviceId)))
    })
    expect(await start).toEqual({ ok: false, code: `NOT_FOUND` })
    const started = await db
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.teamId, w.teamId), eq(sessions.userId, w.mate), eq(sessions.deviceId, w.deviceId)))
    expect(started.map((r) => r.id)).toEqual([w.hosted])
  })
})

// FEED-149: the follow-ups. A machine's id shows in team-visible sessions and
// the box owner's Remove re-registers the box YOUNGER, so no share displaces
// another, whatever the rows' ages: a contested share is CONFLICT and only a
// team owner's drop settles it; the owner reads who holds one.
describe.skipIf(!ready.ok)(`FEED-149: squats, the owner's drop, the holder's name`, () => {
  const shareOf = async (userId: string, deviceId: string) =>
    (
      await db
        .select({ sharedTeamIds: devices.sharedTeamIds })
        .from(devices)
        .where(and(eq(devices.userId, userId), eq(devices.deviceId, deviceId)))
    )[0]!.sharedTeamIds
  const doctor = { checkedAt: new Date().toISOString(), items: [] }
  const registerAs = (
    userId: string,
    deviceId: string,
    extra: { launch?: boolean; key?: ReturnType<ReturnType<typeof machineKey>[`prove`]> } = {}
  ) => devicesAs(userId).register({ deviceId, label: `box`, kind: `server`, doctor, ...extra })

  it(`a teammate's OLDER row (the box owner removed and re-registered the box) cannot take the box over`, async () => {
    const w = await world()
    // 1. A teammate reads the box's id from a team-visible session and
    //    registers it (younger than the box owner's row, for now).
    const attacker = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: attacker, role: `member` })
    await registerAs(attacker, w.deviceId)
    // 2. The box owner presses Remove (FEED-155: it ends the box's runs like
    //    an unshare); the daemon's next launch registers again with a NEW
    //    created_at, so the attacker's row is now the OLDER one. The owner
    //    shares the box again and a teammate runs on it.
    await devicesAs(w.boxOwner).remove({ deviceId: w.deviceId })
    expect(await status(w.hosted)).toBe(`ended`)
    await registerAs(w.boxOwner, w.deviceId, { launch: true, key: w.box.prove(w.boxOwner, w.deviceId) })
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    const ages = await db
      .select({ userId: devices.userId })
      .from(devices)
      .where(eq(devices.deviceId, w.deviceId))
      .orderBy(devices.createdAt, devices.id)
    expect(ages.map((row) => row.userId)).toEqual([attacker, w.boxOwner])
    const run = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `owner's run` } })).sessionId
    // 3. The attacker shares: refused (FEED-154: no key), nothing displaced.
    await expect(
      devicesAs(attacker).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    expect(await shareOf(attacker, w.deviceId)).toEqual([])
    expect(await shareOf(w.boxOwner, w.deviceId)).toEqual([w.teamId])
    // The owner's run is untouched and still theirs to claim and report.
    expect(await status(run)).not.toBe(`ended`)
    const [row] = await db.select().from(sessions).where(eq(sessions.id, run))
    expect(await resolveSessionHost(row!)).toBe(w.boxOwner)
    await expect(assertDeviceHost(w.boxOwner, row!)).resolves.toBeUndefined()
    await expect(assertDeviceHost(attacker, row!)).rejects.toMatchObject({ code: `FORBIDDEN` })
    // New starts still route to the owner's daemon.
    const next = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `again` } })).sessionId
    const [started] = await db.select().from(sessions).where(eq(sessions.id, next))
    expect(await resolveSessionHost(started!)).toBe(w.boxOwner)
  })

  it(`a legacy squat blocks the box owner with CONFLICT until a team owner drops it`, async () => {
    const w = await world()
    // The box owner stops sharing; a share a teammate made of the id
    // before proofs existed (written straight to the table) hosts the box.
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    const squatter = await w.intruder({ shares: true })
    const squatted = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `squatted` } })).sessionId
    const [row] = await db.select().from(sessions).where(eq(sessions.id, squatted))
    expect(await resolveSessionHost(row!)).toBe(squatter)
    // The box owner's share is refused, and ends nothing.
    await expect(
      devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    expect(await shareOf(squatter, w.deviceId)).toEqual([w.teamId])
    expect(await status(squatted)).not.toBe(`ended`)
    // A team owner settles it: the squat's runs end with it, with the reason,
    // and the squat's owner is told.
    const [squatRow] = await db
      .select({ id: devices.id })
      .from(devices)
      .where(and(eq(devices.userId, squatter), eq(devices.deviceId, w.deviceId)))
    await devicesAs(w.lead).dropShare({ id: squatRow!.id, teamId: w.teamId })
    expect(await status(squatted)).toBe(`ended`)
    expect(await lines(squatted)).toContain(`Ended: Tester removed not yours's share with this team.`)
    expect(notices.calls).toEqual([
      expect.objectContaining({ ownerId: squatter, actorId: w.lead, teamId: w.teamId, endedCount: 1, cause: `dropped` }),
    ])
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    expect(await shareOf(w.boxOwner, w.deviceId)).toEqual([w.teamId])
    // FEED-154: the squat never shares again (it lacks the box's key).
    await expect(
      devicesAs(squatter).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
  })

  it(`a CONFLICT names the holder for a team owner only`, async () => {
    const w = await world()
    // The box owner removed the box and its daemon registered again: their
    // row is now YOUNGER than a duplicate that shares the box.
    const holder = await w.intruder({ shares: false })
    await db.execute(sql`UPDATE users SET name = 'Holder Person' WHERE id = ${holder}`)
    await db
      .update(devices)
      .set({ sharedTeamIds: [w.teamId], createdAt: new Date(Date.now() - 86_400_000) })
      .where(and(eq(devices.userId, holder), eq(devices.deviceId, w.deviceId)))
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    await expect(
      devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `CONFLICT`, message: `Another account already shares this machine with that team` })
    await db
      .update(teamMembers)
      .set({ role: `owner` })
      .where(and(eq(teamMembers.teamId, w.teamId), eq(teamMembers.userId, w.boxOwner)))
    await expect(
      devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `CONFLICT`, message: `Holder Person already shares this machine with that team` })
  })

  it(`a team owner drops a member's share: its runs end, its agents disarm, the box owner can share again`, async () => {
    const w = await world()
    const [agent] = await db
      .insert(agents)
      .values({
        teamId: w.teamId,
        name: `Nightly`,
        deviceId: w.deviceId,
        ownerId: w.lead,
        triggers: [{ id: `t1`, enabled: true, kind: `schedule`, cron: `0 3 * * *` }] as never,
      })
      .returning({ id: agents.id })
    const [boxRow] = await db
      .select({ id: devices.id })
      .from(devices)
      .where(and(eq(devices.userId, w.boxOwner), eq(devices.deviceId, w.deviceId)))
    // Only a team owner.
    await expect(devicesAs(w.mate).dropShare({ id: boxRow!.id, teamId: w.teamId })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    expect(await status(w.hosted)).not.toBe(`ended`)
    await devicesAs(w.lead).dropShare({ id: boxRow!.id, teamId: w.teamId })
    expect(await shareOf(w.boxOwner, w.deviceId)).toEqual([])
    expect(await status(w.hosted)).toBe(`ended`)
    const [after] = await db.select({ triggers: agents.triggers }).from(agents).where(eq(agents.id, agent!.id))
    expect(after!.triggers).toMatchObject([{ id: `t1`, enabled: false }])
    // A share the team does not have is not found.
    await expect(devicesAs(w.lead).dropShare({ id: boxRow!.id, teamId: w.teamId })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    expect(await shareOf(w.boxOwner, w.deviceId)).toEqual([w.teamId])
  })

  it(`a share landing between the unshare's commit and its end cannot save the runs it hosted`, async () => {
    const w = await world()
    const rival = await w.intruder({ shares: false })
    // A second account on the box itself (one data dir, so the same key).
    await db
      .update(devices)
      .set({ publicKey: w.box.hex })
      .where(and(eq(devices.userId, rival), eq(devices.deviceId, w.deviceId)))
    // The rival's share lands after the box owner's unshare commits, before
    // its end runs: the rival is then the host of w.hosted.
    endHook.before = async (ids) => {
      expect(ids).toEqual([w.hosted])
      await devicesAs(rival).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
      const [row] = await db.select().from(sessions).where(eq(sessions.id, w.hosted))
      expect(await resolveSessionHost(row!)).toBe(rival)
    }
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    expect(endHook.before).toBeNull()
    // The end still covers it: the set was fixed before anyone else could share.
    expect(await status(w.hosted)).toBe(`ended`)
  })
})

const doctor = { checkedAt: new Date().toISOString(), items: [] }
const shareOfRow = async (userId: string, deviceId: string) =>
  (
    await db
      .select({ sharedTeamIds: devices.sharedTeamIds })
      .from(devices)
      .where(and(eq(devices.userId, userId), eq(devices.deviceId, deviceId)))
  )[0]?.sharedTeamIds
const triggersOf = async (agentId: string) =>
  (await db.select({ triggers: agents.triggers }).from(agents).where(eq(agents.id, agentId)))[0]!.triggers
async function nightly(w: Awaited<ReturnType<typeof world>>) {
  const [agent] = await db
    .insert(agents)
    .values({
      teamId: w.teamId,
      name: `Nightly`,
      deviceId: w.deviceId,
      ownerId: w.lead,
      triggers: [{ id: `t1`, enabled: true, kind: `schedule`, cron: `0 3 * * *` }] as never,
    })
    .returning({ id: agents.id })
  return agent!.id
}

// FEED-155: Remove is an unshare. Before, it deleted the row and left the
// teammates' runs and the team agents on the box, so a teammate who had
// registered the box's id and shared it BEFORE the owner re-shared became
// the host of all of it.
describe.skipIf(!ready.ok)(`FEED-155: Remove acts like an unshare`, () => {
  it(`ends the hosted runs and disarms the agents; an attacker sharing between Remove and re-share inherits nothing`, async () => {
    const w = await world()
    const agentId = await nightly(w)
    const attacker = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: attacker, role: `member` })
    await devicesAs(attacker).register({ deviceId: w.deviceId, label: `box`, kind: `server`, doctor })
    await devicesAs(w.boxOwner).remove({ deviceId: w.deviceId })
    expect(await status(w.hosted)).toBe(`ended`)
    expect(await lines(w.hosted)).toContain(`Ended: Tester removed box, the machine this session ran on.`)
    expect(await triggersOf(agentId)).toMatchObject([{ id: `t1`, enabled: false }])
    // The attacker shares before the owner does: refused (no key), even
    // with a key of its own (the box's is pinned).
    await expect(
      devicesAs(attacker).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    const own = machineKey()
    const proven = await devicesAs(attacker).register({
      deviceId: w.deviceId,
      label: `box`,
      kind: `server`,
      doctor,
      key: own.prove(attacker, w.deviceId),
    })
    expect(proven).toMatchObject({ ok: true, keyAccepted: false })
    await expect(
      devicesAs(attacker).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    // Nobody shares the box: a teammate's start finds no machine.
    await expect(
      sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `orphan` } })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    // The owner's daemon launches again and re-shares: it hosts again.
    await devicesAs(w.boxOwner).register({
      deviceId: w.deviceId,
      label: `box`,
      kind: `server`,
      doctor,
      launch: true,
      key: w.box.prove(w.boxOwner, w.deviceId),
    })
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    const next = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `back` } })).sessionId
    const [started] = await db.select().from(sessions).where(eq(sessions.id, next))
    expect(await resolveSessionHost(started!)).toBe(w.boxOwner)
  })

  it(`a team the box owner owns keeps its agents armed`, async () => {
    const w = await world()
    const agentId = await nightly(w)
    await db
      .update(teamMembers)
      .set({ role: `owner` })
      .where(and(eq(teamMembers.teamId, w.teamId), eq(teamMembers.userId, w.boxOwner)))
    await devicesAs(w.boxOwner).remove({ deviceId: w.deviceId })
    expect(await triggersOf(agentId)).toMatchObject([{ id: `t1`, enabled: true }])
    expect(await status(w.hosted)).toBe(`ended`)
  })
})

// EXP-1417: a removed row came back on the running build's next heartbeat
// miss or re-dial; a removal now stands until that build's next launch.
describe.skipIf(!ready.ok)(`EXP-1417: Remove sticks`, () => {
  it(`a removed machine stays removed until its build launches again`, async () => {
    const w = await world()
    await devicesAs(w.boxOwner).remove({ deviceId: w.deviceId })
    const beat = await devicesAs(w.boxOwner).heartbeat({ deviceId: w.deviceId, activeSessions: 0, defaultsSyncedAt: null })
    expect(beat).toEqual({ ok: false, updateRequested: false, removed: true })
    const again = await devicesAs(w.boxOwner).register({ deviceId: w.deviceId, label: `box`, kind: `server`, doctor })
    expect(again).toMatchObject({ ok: false, removed: true })
    expect(await shareOfRow(w.boxOwner, w.deviceId)).toBeUndefined()
    // Removing it again is NOT_FOUND, never a silent success.
    await expect(devicesAs(w.boxOwner).remove({ deviceId: w.deviceId })).rejects.toMatchObject({ code: `NOT_FOUND` })
    const launched = await devicesAs(w.boxOwner).register({
      deviceId: w.deviceId,
      label: `box`,
      kind: `server`,
      doctor,
      launch: true,
    })
    expect(launched).toMatchObject({ ok: true, removed: false })
    // Unshared: Remove was an unshare.
    expect(await shareOfRow(w.boxOwner, w.deviceId)).toEqual([])
    // Another account's removal of the same id is its own.
    const other = await user()
    await devicesAs(other).register({ deviceId: w.deviceId, label: `box`, kind: `server`, doctor })
    expect(await shareOfRow(other, w.deviceId)).toEqual([])
  })
})

// FEED-154: proof of possession. A share needs the row's register to have
// proven the machine's pinned key; the key holder's register clears the
// shares other accounts hold of the id without it.
describe.skipIf(!ready.ok)(`FEED-154: the machine key`, () => {
  it(`the first proven register pins the key; a proof for another account does not verify`, async () => {
    const owner = await user()
    const deviceId = `dev-${id8()}`
    made.devices.push(deviceId)
    const key = machineKey()
    const first = await devicesAs(owner).register({ deviceId, label: `box`, kind: `server`, doctor, launch: true, key: key.prove(owner, deviceId) })
    expect(first).toMatchObject({ ok: true, keyAccepted: true })
    const [pin] = await db.select().from(deviceKeys).where(eq(deviceKeys.deviceId, deviceId))
    expect(pin).toMatchObject({ publicKey: key.hex, userId: owner })
    // A replay of the owner's proof under another account fails the check.
    const thief = await user()
    const replay = await devicesAs(thief).register({ deviceId, label: `box`, kind: `server`, doctor, key: key.prove(owner, deviceId) })
    expect(replay).toMatchObject({ ok: true, keyAccepted: false })
    const [thiefRow] = await db
      .select({ publicKey: devices.publicKey })
      .from(devices)
      .where(and(eq(devices.userId, thief), eq(devices.deviceId, deviceId)))
    expect(thiefRow!.publicKey).toBeNull()
    // The pinning account rotates its own key (a fresh data dir).
    const rotated = machineKey()
    await devicesAs(owner).register({ deviceId, label: `box`, kind: `server`, doctor, key: rotated.prove(owner, deviceId) })
    const [after] = await db.select().from(deviceKeys).where(eq(deviceKeys.deviceId, deviceId))
    expect(after!.publicKey).toBe(rotated.hex)
  })

  it(`the key holder's register clears a legacy squat: its runs end, its owner is told`, async () => {
    const w = await world()
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    const squatter = await w.intruder({ shares: true })
    const squatted = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `squatted` } })).sessionId
    notices.calls = []
    await devicesAs(w.boxOwner).register({
      deviceId: w.deviceId,
      label: `box`,
      kind: `server`,
      doctor,
      key: w.box.prove(w.boxOwner, w.deviceId),
    })
    expect(await shareOfRow(squatter, w.deviceId)).toEqual([])
    expect(await status(squatted)).toBe(`ended`)
    expect(notices.calls).toEqual([
      expect.objectContaining({ ownerId: squatter, actorId: w.boxOwner, teamId: w.teamId, endedCount: 1, cause: `key` }),
    ])
    // The box owner shares again: no CONFLICT left.
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    expect(await shareOfRow(w.boxOwner, w.deviceId)).toEqual([w.teamId])
  })

  it(`no first pin while another account's row shares the id (pre-key boxes)`, async () => {
    const w = await world()
    await db.delete(deviceKeys).where(eq(deviceKeys.deviceId, w.deviceId))
    const newcomer = await w.intruder({ shares: false })
    const own = machineKey()
    const result = await devicesAs(newcomer).register({
      deviceId: w.deviceId,
      label: `box`,
      kind: `server`,
      doctor,
      key: own.prove(newcomer, w.deviceId),
    })
    expect(result).toMatchObject({ ok: true, keyAccepted: false })
    expect(await db.select().from(deviceKeys).where(eq(deviceKeys.deviceId, w.deviceId))).toEqual([])
    // The sharing box owner's proof pins it.
    const pinned = await devicesAs(w.boxOwner).register({
      deviceId: w.deviceId,
      label: `box`,
      kind: `server`,
      doctor,
      key: w.box.prove(w.boxOwner, w.deviceId),
    })
    expect(pinned).toMatchObject({ keyAccepted: true })
  })

  it(`an instance admin drops a share and resets a pin; nobody else can`, async () => {
    const w = await world()
    const admin = await user()
    await db.execute(sql`UPDATE users SET is_admin = true WHERE id = ${admin}`)
    const [boxRow] = await db
      .select({ id: devices.id })
      .from(devices)
      .where(and(eq(devices.userId, w.boxOwner), eq(devices.deviceId, w.deviceId)))
    await expect(adminAs(w.lead).dropDeviceShare({ id: boxRow!.id, teamId: w.teamId })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    await adminAs(admin).dropDeviceShare({ id: boxRow!.id, teamId: w.teamId })
    expect(await shareOfRow(w.boxOwner, w.deviceId)).toEqual([])
    expect(await status(w.hosted)).toBe(`ended`)
    expect(notices.calls).toEqual([
      expect.objectContaining({ ownerId: w.boxOwner, actorId: admin, teamId: w.teamId, endedCount: 1, cause: `dropped` }),
    ])
    await adminAs(admin).clearDeviceKey({ deviceId: w.deviceId })
    expect(await db.select().from(deviceKeys).where(eq(deviceKeys.deviceId, w.deviceId))).toEqual([])
    await expect(adminAs(admin).clearDeviceKey({ deviceId: w.deviceId })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})

// FEED-159: the owner signal for pinning is the account that registered
// the id FIRST, which Remove never clears. The eldest `devices` row was
// not: the owner's Remove + relaunch made the squat's row the eldest.
describe.skipIf(!ready.ok)(`FEED-159: the first-seen account pins`, () => {
  const register = (userId: string, deviceId: string, extra: { launch?: boolean; key?: ReturnType<ReturnType<typeof machineKey>[`prove`]> } = {}) =>
    devicesAs(userId).register({ deviceId, label: `box`, kind: `server`, doctor, ...extra })
  const pinOf = async (deviceId: string) =>
    (await db.select({ userId: deviceKeys.userId, publicKey: deviceKeys.publicKey }).from(deviceKeys).where(eq(deviceKeys.deviceId, deviceId)))[0]
  /** The box owner on an old client (no key), then a teammate squats the id. */
  async function squatted() {
    const owner = await user()
    const squat = await user()
    const deviceId = `dev-${id8()}`
    made.devices.push(deviceId)
    const box = machineKey()
    const own = machineKey()
    await register(owner, deviceId, { launch: true })
    await register(squat, deviceId)
    return { owner, squat, deviceId, box, own }
  }
  /** The box owner presses Remove and relaunches: its row is now younger. */
  async function removeAndRelaunch(owner: string, deviceId: string) {
    await devicesAs(owner).remove({ deviceId })
    await register(owner, deviceId, { launch: true })
    const ages = await db
      .select({ userId: devices.userId })
      .from(devices)
      .where(eq(devices.deviceId, deviceId))
      .orderBy(devices.createdAt, devices.id)
    expect(ages[0]!.userId).not.toBe(owner)
  }

  it(`after the owner's Remove + relaunch the squat's eldest row still cannot pin first; the owner's upgrade pins`, async () => {
    const m = await squatted()
    await removeAndRelaunch(m.owner, m.deviceId)
    expect(await register(m.squat, m.deviceId, { key: m.own.prove(m.squat, m.deviceId) })).toMatchObject({ keyAccepted: false })
    expect(await pinOf(m.deviceId)).toBeUndefined()
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: true })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.owner, publicKey: m.box.hex })
  })

  // FEED-165: here first-seen is ALREADY the owner's (its register came
  // first, after 0206); the pin is the squat's anyway. Whether the backfill
  // itself makes the owner first-seen is the FEED-165 suite below.
  it(`with the owner already first-seen, a squat's pin never returns after the owner's Remove + relaunch; the owner takes it over`, async () => {
    const m = await squatted()
    await removeAndRelaunch(m.owner, m.deviceId)
    // A pin the squat took before FEED-159 (the first key won).
    await db.insert(deviceKeys).values({ deviceId: m.deviceId, publicKey: m.own.hex, userId: m.squat })
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: true })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.owner, publicKey: m.box.hex })
    // The owner shares; the squat's next register neither re-pins nor withdraws it.
    const [team] = await db.insert(teams).values({ name: `Pin`, slug: `pin-${id8()}` }).returning({ id: teams.id })
    made.teams.push(team!.id)
    await db.insert(teamMembers).values([
      { teamId: team!.id, userId: m.owner, role: `owner` },
      { teamId: team!.id, userId: m.squat, role: `member` },
    ])
    await devicesAs(m.owner).setShared({ deviceId: m.deviceId, teamId: team!.id, shared: true })
    expect(await register(m.squat, m.deviceId, { key: m.own.prove(m.squat, m.deviceId) })).toMatchObject({ keyAccepted: false })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.owner, publicKey: m.box.hex })
    expect(await shareOfRow(m.owner, m.deviceId)).toEqual([team!.id])
  })

  it(`an admin clear hands the next pin to the first-seen owner, not the eldest row; pinTo moves it`, async () => {
    const m = await squatted()
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: true })
    await removeAndRelaunch(m.owner, m.deviceId)
    const admin = await user()
    await db.execute(sql`UPDATE users SET is_admin = true WHERE id = ${admin}`)
    await adminAs(admin).clearDeviceKey({ deviceId: m.deviceId })
    expect(await register(m.squat, m.deviceId, { key: m.own.prove(m.squat, m.deviceId) })).toMatchObject({ keyAccepted: false })
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: true })
    // The admin settles it the other way: only the named account's proof pins.
    await expect(adminAs(admin).clearDeviceKey({ deviceId: m.deviceId, pinTo: await user() })).rejects.toMatchObject({ code: `NOT_FOUND` })
    await adminAs(admin).clearDeviceKey({ deviceId: m.deviceId, pinTo: m.squat })
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: false })
    expect(await register(m.squat, m.deviceId, { key: m.own.prove(m.squat, m.deviceId) })).toMatchObject({ keyAccepted: true })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.squat, publicKey: m.own.hex })
  })

  it(`a second account proving the pinned key is accepted; the pin stays the first's`, async () => {
    const m = await squatted()
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: true })
    expect(await register(m.squat, m.deviceId, { key: m.box.prove(m.squat, m.deviceId) })).toMatchObject({ keyAccepted: true })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.owner, publicKey: m.box.hex })
  })
})

// FEED-165: 0206's backfill took the pin holder first, else the eldest live
// row, so a squat from BEFORE the deploy became first-seen. 0213 recomputes
// from the earliest evidence. Each case seeds the pre-deploy world, runs
// 0206's backfill then 0213 (the migration files themselves) in a rolled-back
// transaction, writes back only this id's result, then plays the upgrade.
describe.skipIf(!ready.ok)(`FEED-165: 0213 makes the earliest evidence first-seen`, () => {
  const register = (userId: string, deviceId: string, extra: { launch?: boolean; key?: ReturnType<ReturnType<typeof machineKey>[`prove`]> } = {}) =>
    devicesAs(userId).register({ deviceId, label: `box`, kind: `server`, doctor, ...extra })
  const pinOf = async (deviceId: string) =>
    (await db.select({ userId: deviceKeys.userId, publicKey: deviceKeys.publicKey }).from(deviceKeys).where(eq(deviceKeys.deviceId, deviceId)))[0]
  const migration = (file: string) =>
    readFileSync(new URL(`../../db/out/${file}`, import.meta.url), `utf8`)
      .split(`--> statement-breakpoint`)
      .filter((statement) => statement.includes(`INSERT INTO "device_first_seen"`))
  const firstSeenIn = async (tx: { execute: typeof db.execute }, deviceId: string) =>
    (await tx.execute(sql`SELECT user_id FROM device_first_seen WHERE device_id = ${deviceId}`)).rows[0]?.user_id
  /** 0206's backfill on a world from before it (no first-seen row), then 0213. */
  async function backfill(deviceId: string) {
    const out = { after0206: undefined as unknown, after0213: undefined as unknown, mismatch: true }
    await inRollback(async (tx) => {
      await tx.execute(sql`DELETE FROM device_first_seen WHERE device_id = ${deviceId}`)
      for (const statement of migration(`0206_feed159_device_first_seen.sql`)) await tx.execute(sql.raw(statement))
      out.after0206 = await firstSeenIn(tx, deviceId)
      for (const statement of migration(`0213_feed165_device_first_seen_earliest_evidence.sql`)) await tx.execute(sql.raw(statement))
      out.after0213 = await firstSeenIn(tx, deviceId)
      const { deviceFirstSeenFindings } = await import(`@/lib/device-first-seen-audit`)
      out.mismatch = (await deviceFirstSeenFindings(tx as never)).some(
        (finding) => finding.deviceId === deviceId && finding.kind === `mismatch`
      )
    })
    await db
      .insert(deviceFirstSeen)
      .values({ deviceId, userId: out.after0213 as string })
      .onConflictDoUpdate({ target: deviceFirstSeen.deviceId, set: { userId: out.after0213 as string } })
    return out
  }
  /** 0213 alone on the world as it stands (rolled back). */
  async function only0213(deviceId: string) {
    let after: unknown
    await inRollback(async (tx) => {
      for (const statement of migration(`0213_feed165_device_first_seen_earliest_evidence.sql`)) await tx.execute(sql.raw(statement))
      after = await firstSeenIn(tx, deviceId)
    })
    return after
  }
  /** The box owner on an old client, then a squat of the id; a team. */
  async function preDeploy() {
    const owner = await user()
    const squat = await user()
    const mate = await user()
    const deviceId = `dev-${id8()}`
    made.devices.push(deviceId)
    const [team] = await db.insert(teams).values({ name: `Seen`, slug: `seen-${id8()}` }).returning({ id: teams.id })
    made.teams.push(team!.id)
    await db.insert(teamMembers).values([
      { teamId: team!.id, userId: owner, role: `owner` },
      { teamId: team!.id, userId: squat, role: `member` },
      { teamId: team!.id, userId: mate, role: `member` },
    ])
    await register(owner, deviceId, { launch: true })
    await register(squat, deviceId)
    /** A session `userId` started on the id `daysAgo` days ago. */
    const started = async (userId: string, daysAgo: number) => {
      const id = crypto.randomUUID()
      await db.insert(sessions).values({
        id,
        rootId: id,
        teamId: team!.id,
        userId,
        deviceId,
        status: `ended`,
        createdAt: new Date(Date.now() - daysAgo * 86_400_000),
      })
    }
    return { owner, squat, mate, deviceId, started, box: machineKey(), own: machineKey() }
  }

  it(`a squat that held the pin before the deploy keeps first-seen (0213 never moves a pin holder's); the audit lists it`, async () => {
    const m = await preDeploy()
    // The squat's pin from before FEED-159 (the first key won).
    await db.insert(deviceKeys).values({ deviceId: m.deviceId, publicKey: m.own.hex, userId: m.squat })
    expect(await backfill(m.deviceId)).toEqual({ after0206: m.squat, after0213: m.squat, mismatch: true })
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: false })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.squat, publicKey: m.own.hex })
  })

  it(`an owner who removed, relaunched and pinned keeps first-seen over a squat's older row; the squat never pins`, async () => {
    const m = await preDeploy()
    await db
      .update(devices)
      .set({ createdAt: new Date(Date.now() - 10 * 86_400_000) })
      .where(and(eq(devices.userId, m.squat), eq(devices.deviceId, m.deviceId)))
    // The relaunch deletes the removal: only the pin is left to guard on.
    await devicesAs(m.owner).remove({ deviceId: m.deviceId })
    await register(m.owner, m.deviceId, { launch: true })
    await db.insert(deviceKeys).values({ deviceId: m.deviceId, publicKey: m.box.hex, userId: m.owner })
    expect(await backfill(m.deviceId)).toEqual({ after0206: m.owner, after0213: m.owner, mismatch: true })
    expect(await register(m.squat, m.deviceId, { key: m.own.prove(m.squat, m.deviceId) })).toMatchObject({ keyAccepted: false })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.owner, publicKey: m.box.hex })
  })

  it(`no pin, the owner's Remove + relaunch before the deploy: 0206 took the squat's eldest row, 0213 the owner's first session`, async () => {
    const m = await preDeploy()
    // A teammate's start on the shared box is older still, but it never
    // registered the id: not a candidate.
    await m.started(m.mate, 2)
    await m.started(m.owner, 1)
    await devicesAs(m.owner).remove({ deviceId: m.deviceId })
    await register(m.owner, m.deviceId, { launch: true })
    expect(await backfill(m.deviceId)).toEqual({ after0206: m.squat, after0213: m.owner, mismatch: false })
    expect(await register(m.squat, m.deviceId, { key: m.own.prove(m.squat, m.deviceId) })).toMatchObject({ keyAccepted: false })
    expect(await pinOf(m.deviceId)).toBeUndefined()
    expect(await register(m.owner, m.deviceId, { key: m.box.prove(m.owner, m.deviceId) })).toMatchObject({ keyAccepted: true })
    expect(await pinOf(m.deviceId)).toEqual({ userId: m.owner, publicKey: m.box.hex })
  })

  it(`an owner whose Remove stands keeps first-seen over a squat's older row`, async () => {
    const m = await preDeploy()
    // 0206 made the owner first-seen (no pin) from a row since gone (an old
    // backfill); the squat's row reads older still; then the owner removed
    // the box (the hard delete takes its row, the removal stays).
    await db
      .update(deviceFirstSeen)
      .set({ userId: m.owner, firstSeenAt: new Date(Date.now() - 20 * 86_400_000) })
      .where(eq(deviceFirstSeen.deviceId, m.deviceId))
    await db
      .update(devices)
      .set({ createdAt: new Date(Date.now() - 30 * 86_400_000) })
      .where(and(eq(devices.userId, m.squat), eq(devices.deviceId, m.deviceId)))
    await devicesAs(m.owner).remove({ deviceId: m.deviceId })
    expect(await only0213(m.deviceId)).toBe(m.owner)
    await db.delete(deviceRemovals).where(eq(deviceRemovals.deviceId, m.deviceId))
    expect(await only0213(m.deviceId)).toBe(m.squat)
  })

  it(`an admin's pinTo from before 0213 is never overwritten`, async () => {
    const m = await preDeploy()
    // An admin pinned the id to the squat on purpose (the clear drops the
    // pin, so only the marker protects the row): 0213 keeps it.
    await db.insert(deviceKeys).values({ deviceId: m.deviceId, publicKey: m.own.hex, userId: m.squat })
    const admin = await user()
    await db.execute(sql`UPDATE users SET is_admin = true WHERE id = ${admin}`)
    await adminAs(admin).clearDeviceKey({ deviceId: m.deviceId, pinTo: m.squat })
    expect(await only0213(m.deviceId)).toBe(m.squat)
    // The same row as 0206 would have written it (the squat's own row's
    // `created_at`, in SQL: a JS Date drops the microseconds) moves.
    await db.execute(sql`
      UPDATE device_first_seen f SET first_seen_at = d.created_at
        FROM devices d WHERE d.device_id = f.device_id AND d.user_id = f.user_id AND f.device_id = ${m.deviceId}`)
    expect(await only0213(m.deviceId)).toBe(m.owner)
  })

  it(`the audit lists a first-seen squat 0206 left behind, with the pinTo that settles it`, async () => {
    const m = await preDeploy()
    await db.insert(deviceKeys).values({ deviceId: m.deviceId, publicKey: m.own.hex, userId: m.squat })
    await inRollback(async (tx) => {
      await tx.execute(sql`UPDATE device_first_seen SET user_id = ${m.squat} WHERE device_id = ${m.deviceId}`)
      const { deviceFirstSeenFindings, settleCall } = await import(`@/lib/device-first-seen-audit`)
      const finding = (await deviceFirstSeenFindings(tx as never)).find((entry) => entry.deviceId === m.deviceId)
      expect(finding).toMatchObject({
        kind: `mismatch`,
        firstSeen: { userId: m.squat },
        earliest: { userId: m.owner, source: `devices`, hasRow: true },
        pin: { userId: m.squat },
      })
      expect(finding!.evidence.map((item) => [item.userId, item.source])).toContainEqual([m.squat, `pin`])
      expect(settleCall(finding!)).toBe(`admin.clearDeviceKey({"deviceId":"${m.deviceId}","pinTo":"${m.owner}"})`)
    })
  })
})

describe.skipIf(!ready.ok)(`FEED-149: the release train's duplicate-share check`, () => {
  it(`lists a legacy duplicate pair, host row first`, async () => {
    const { duplicateDeviceShares } = await import(`@/lib/device-share-duplicates`)
    const w = await world()
    const legacy = await w.intruder({ shares: true })
    await inRollback(async (tx) => {
      const pair = (await duplicateDeviceShares(tx as never)).find((entry) => entry.deviceId === w.deviceId)
      expect(pair?.teamId).toBe(w.teamId)
      expect(pair?.rows.map((row) => row.userId)).toEqual([w.boxOwner, legacy])
    })
  })
})
