// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, boards, devices, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"

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
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
}))

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
const { endHostedSessions, foreignHostedSessionIds } = await import(`@/lib/sessions/lifecycle`)
const { guardAndCleanupTeamsForUserDeletion } = await import(`@/lib/account-deletion`)
const { devicesRouter } = await import(`@/lib/trpc/devices`)
const { teamMembersRouter } = await import(`@/lib/trpc/team-members`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

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
  await db.insert(devices).values({ userId: boxOwner, deviceId, label: `box`, kind: `server`, sharedTeamIds: [teamId] })
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
  return { lead, boxOwner, mate, teamId, deviceId, hosted, intruder }
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
    await expect(
      devicesAs(lowId).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `CONFLICT` })
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

// FEED-149: the follow-ups. The box's own row predates anyone who learned its
// id from a team-visible session, so an older row's share displaces a younger
// squat; a team owner can drop any member's share, and reads who holds one.
describe.skipIf(!ready.ok)(`FEED-149: squats, the owner's drop, the holder's name`, () => {
  const shareOf = async (userId: string, deviceId: string) =>
    (
      await db
        .select({ sharedTeamIds: devices.sharedTeamIds })
        .from(devices)
        .where(and(eq(devices.userId, userId), eq(devices.deviceId, deviceId)))
    )[0]!.sharedTeamIds

  it(`the box owner's share displaces a younger account's squat and ends the runs it hosted`, async () => {
    const w = await world()
    // The box owner stops sharing; a teammate who read the id shares first.
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: false })
    const squatter = await w.intruder({ shares: false })
    await devicesAs(squatter).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    const squatted = (await sendSession({ userId: w.mate, input: { teamId: w.teamId, deviceId: w.deviceId, text: `squatted` } })).sessionId
    const [row] = await db.select().from(sessions).where(eq(sessions.id, squatted))
    expect(await resolveSessionHost(row!)).toBe(squatter)
    // The box owner shares again: no CONFLICT, the squat is gone, and so is
    // the run it was the consent for.
    await devicesAs(w.boxOwner).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    expect(await shareOf(squatter, w.deviceId)).toEqual([])
    expect(await shareOf(w.boxOwner, w.deviceId)).toEqual([w.teamId])
    expect(await status(squatted)).toBe(`ended`)
    // And the squatter cannot take it back.
    await expect(
      devicesAs(squatter).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    ).rejects.toMatchObject({ code: `CONFLICT` })
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
    // An OLDER row (it would be refused otherwise): once the box owner's
    // share is gone, the rival's share makes it the host of w.hosted.
    await db
      .update(devices)
      .set({ createdAt: new Date(Date.now() - 86_400_000) })
      .where(and(eq(devices.userId, rival), eq(devices.deviceId, w.deviceId)))
    // The unshare's transaction, step by step: lock, read the set, write.
    const ids = await db.transaction(async (tx) => {
      await lockDeviceShares(tx, w.deviceId)
      const read = await foreignHostedSessionIds(tx as never, w.boxOwner, w.teamId, [w.deviceId])
      await tx.update(devices).set({ sharedTeamIds: [] }).where(and(eq(devices.userId, w.boxOwner), eq(devices.deviceId, w.deviceId)))
      return read
    })
    expect(ids).toEqual([w.hosted])
    // The rival's share lands after the commit, before the end.
    await devicesAs(rival).setShared({ deviceId: w.deviceId, teamId: w.teamId, shared: true })
    const [row] = await db.select().from(sessions).where(eq(sessions.id, w.hosted))
    expect(await resolveSessionHost(row!)).toBe(rival)
    // The end still covers it: the set was fixed before anyone else could share.
    await endHostedSessions(ids)
    expect(await status(w.hosted)).toBe(`ended`)
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
