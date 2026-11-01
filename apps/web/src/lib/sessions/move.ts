import { TRPCError } from "@trpc/server"
import { and, desc, eq, ne, sql } from "drizzle-orm"
import { isPersistent } from "@exp/domain-contract/session-kind"
import { db } from "@/db/connection"
import { devices, events, sessions, type Device, type Session } from "@/db/schema"
import { carriedAccountOption, flattenAccounts } from "@/lib/accounts/account-option"
import { sessionDeviceOwner } from "@/lib/sessions/host"
import { effectiveCli, resolveSessionDevice } from "@/lib/sessions/send"
import { visibilityMove } from "@/lib/session-device-move"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { emitEventInTx } from "@/lib/events"
import type { Tx } from "@/lib/sessions/tx"

// NEW-1: moving a session to another device = `sessions.send({sessionId,
// deviceId})`: a QUEUED row simply re-targets; a LIVE one ends here and
// continues on the target as a new session on the same branch (the device
// pushes its branch before it goes, NEW-5). No handoff package, nothing
// pending on the row. The refusal copy is byte-identical with every
// client's (`session-device-move` ×4).

export const MOVE_COPY = {
  steer: `Only people who can steer this session can move it.`,
  ended: `This session has ended.`,
  agent: `Agents move from Settings → Agents.`,
  offline: `This device is offline.`,
  visibility: `This device is shared differently, so the session can't move here.`,
  noLogin: `This device has no signed-in login for this agent.`,
} as const

export const movedText = (from: string, to: string) => `Moved from ${from} to ${to}`

type DeviceRow = Pick<
  Device,
  | `deviceId`
  | `label`
  | `userId`
  | `lastSeenAt`
  | `version`
  | `agentAccounts`
  | `agentUsage`
  | `launchDefaults`
>

/** ONE machine's devices row (devices are per (user, deviceId)). */
export async function loadDeviceRow(deviceId: string, ownerId: string): Promise<DeviceRow | null> {
  const [row] = await db
    .select({
      deviceId: devices.deviceId,
      label: devices.label,
      userId: devices.userId,
      lastSeenAt: devices.lastSeenAt,
      version: devices.version,
      agentAccounts: devices.agentAccounts,
      agentUsage: devices.agentUsage,
      launchDefaults: devices.launchDefaults,
    })
    .from(devices)
    .where(and(eq(devices.deviceId, deviceId), eq(devices.userId, ownerId)))
    .limit(1)
  return row ?? null
}

export function deviceName(row: Pick<DeviceRow, `label`> | null, deviceId: string): string {
  return row?.label?.trim() || deviceId
}

/** The agent a session runs: its `cli`, else its host device's default, else
 *  claude (contract order). */
async function sessionAgent(session: Session): Promise<string> {
  return (await effectiveCli(session)) ?? `claude`
}

/** The refusals that hold whatever the target is (agent, ended). */
export function assertMovable(session: Session): void {
  if (isPersistent(session)) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: MOVE_COPY.agent })
  }
  if (session.status === `ended`) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.ended })
  }
}

export interface MoveTarget {
  deviceId: string
  name: string
  ownerId: string
  /** The login to run on there (a profile id of the target), else null. */
  account: string | null
  /** The visibility the session has there (its own, or the target's when a
   *  queued root adopts it, `visibilityMove`). */
  shared: boolean
}

/**
 * The target of a move: visible to the session's STARTER exactly like a new
 * session's device (own, or shared with the team), the SAME visibility
 * (`shared`) as the session (or the one a queued root adopts,
 * `visibilityMove`), online (when `requireOnline`) and signed in for the
 * session's agent. `account` = an explicit pick (must be a
 * signed-in login of that agent there), else the source's login carried by
 * email (`carriedAccountOption`), else null.
 */
export async function resolveMoveTarget(
  session: Session,
  deviceId: string,
  opts: {
    account?: string | null
    requireOnline: boolean
    now?: Date
    /** Who moves it (`visibilityMove`); unset = never adopts. */
    moverId?: string
  }
): Promise<MoveTarget> {
  let device: Awaited<ReturnType<typeof resolveSessionDevice>>
  try {
    device = await resolveSessionDevice(session.userId, session.teamId, deviceId)
  } catch (e) {
    // #EXP-1440: another team's machine says so, not "shared differently".
    if (e instanceof TRPCError && e.code === `FORBIDDEN`) throw e
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.visibility })
  }
  const shared = visibilityMove({
    sessionShared: session.shared,
    targetShared: device.shared,
    queuedRoot:
      opts.moverId !== undefined &&
      session.status === `queued` &&
      session.parentId === null &&
      !(await hasSubSessions(session.id)),
    targetIsStarters: device.ownerId === session.userId,
    moverIsStarter: opts.moverId === session.userId,
  })
  if (shared === null) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.visibility })
  }
  const row = await loadDeviceRow(deviceId, device.ownerId)
  if (!row) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.offline })
  }
  if (opts.requireOnline && !deviceRowIsOnline(row.lastSeenAt, opts.now ?? new Date())) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.offline })
  }
  const agent = await sessionAgent(session)
  const options = flattenAccounts(row).filter((option) => option.agent === agent)
  if (options.length === 0) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: MOVE_COPY.noLogin })
  }
  let account: string | null = null
  if (opts.account) {
    if (!options.some((option) => option.id === opts.account)) {
      throw new TRPCError({
        code: `BAD_REQUEST`,
        message: `That login is not signed in on this device`,
      })
    }
    account = opts.account
  } else {
    account = (await carriedAccount(session, agent, options)) ?? null
  }
  return {
    deviceId,
    name: deviceName(row, deviceId),
    ownerId: device.ownerId,
    account,
    shared,
  }
}

async function hasSubSessions(rootId: string): Promise<boolean> {
  const [row] = await db
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(eq(sessions.rootId, rootId), ne(sessions.id, rootId)))
    .limit(1)
  return !!row
}

/** The source's login (by email, off its host device's report) on the
 *  target's options; undefined when it has no address or no match. */
async function carriedAccount(
  session: Session,
  agent: string,
  options: ReturnType<typeof flattenAccounts>
): Promise<string | undefined> {
  if (!session.account) return undefined
  const source = await loadDeviceRow(session.deviceId, await sessionDeviceOwner(session))
  const profile = source?.agentAccounts?.[agent]?.profiles?.find(
    (entry) => entry?.id === session.account
  )
  if (!profile?.email) return undefined
  return carriedAccountOption(options, { agent: agent as `claude`, email: profile.email })?.id
}

/** #FEED-182 (#EXP-1532): the login a CHILD started on ANOTHER device than
 *  its parent runs on. Profile ids are device-local, so neither the parent's
 *  account nor an id the caller copied from the parent's device means
 *  anything there. `requested` (a profile id OR an email) must name a login
 *  signed in and healthy for `agent` on the target, else the send is
 *  refused naming the logins that are; without one, the parent's login is
 *  carried by email, else null (the device picks a working login itself). */
export async function childAccountOnDevice(
  parent: Session,
  agent: string,
  target: { deviceId: string; ownerId: string },
  requested: string | undefined
): Promise<string | null> {
  const row = await loadDeviceRow(target.deviceId, target.ownerId)
  // A device that reported no logins is unknowable here: its own launch
  // gate decides, so nothing foreign is pinned on it.
  if (!row?.agentAccounts || Object.keys(row.agentAccounts).length === 0) return null
  const options = flattenAccounts(row).filter(
    (option) => option.agent === agent && option.health !== `signed_out` && option.health !== `needs_relogin`
  )
  if (requested) {
    const wanted = requested.trim().toLowerCase()
    const byId = options.find((option) => option.id === requested.trim())
    const byEmail = options.find((option) => option.email.trim().toLowerCase() === wanted)
    // An id from the PARENT's device: the same login there, by email.
    const carried = byId || byEmail ? undefined : await carriedProfile(parent, agent, requested.trim(), options)
    const hit = byId ?? byEmail ?? carried
    if (hit) return hit.id
    const names = options.map((option) => option.email).filter(Boolean)
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: names.length
        ? `Account "${requested}" is not signed in for ${agent} on ${deviceName(row, target.deviceId)}. Signed in there: ${names.join(`, `)}.`
        : `${deviceName(row, target.deviceId)} has no signed-in login for ${agent}.`,
    })
  }
  return (await carriedAccount(parent, agent, options)) ?? null
}

/** A profile id of `session`'s OWN device, carried to `options` by email. */
async function carriedProfile(
  session: Session,
  agent: string,
  profileId: string,
  options: ReturnType<typeof flattenAccounts>
) {
  const source = await loadDeviceRow(session.deviceId, await sessionDeviceOwner(session))
  const profile = source?.agentAccounts?.[agent]?.profiles?.find((entry) => entry?.id === profileId)
  if (!profile?.email) return undefined
  return carriedAccountOption(options, { agent: agent as `claude`, email: profile.email })
}

// ── EXP-1542 (#FEED-169): the hand-off of a moved session's branch ─────────
// A move that leaves work behind on its SOURCE device (a live session's
// relocation, an ended root revived on another device) is recorded as a
// server-emitted `session.moved` event: ids only (the events shape syncs to
// every member, #EXP-1409). The source device reads it off its synced
// events (THE record that it is the source: never the moved line's text),
// pushes the branch, then reports `sessions.handedOff`, which records ONE
// `session.handed_off` event per move. The continuation's device reads
// both through its brief (`moved`) and `sessions.moveState`, and waits for
// the hand-off before it cuts its worktree. A queued row's re-target ran
// nowhere: nothing to hand off, no record.

export const SESSION_MOVED_EVENT = `session.moved`
export const SESSION_HANDED_OFF_EVENT = `session.handed_off`

export const handOffOutcomeValues = [`pushed`, `parked`, `none`] as const
export type HandOffOutcome = (typeof handOffOutcomeValues)[number]

export interface SessionMoveState {
  /** The `session.moved` event's id. */
  moveId: string
  /** The row that moved (a relocation's ended row; a revived root itself). */
  sessionId: string
  continuationId: string
  fromDeviceId: string
  toDeviceId: string
  movedAt: string
  /** Null = the source device has not handed off yet. */
  handedOff: { outcome: HandOffOutcome; leftover: string | null; at: string } | null
}

export async function recordMoveInTx(
  tx: Tx,
  move: { teamId: string; sessionId: string; continuationId: string; fromDeviceId: string; toDeviceId: string }
): Promise<void> {
  await emitEventInTx(tx, {
    teamId: move.teamId,
    name: SESSION_MOVED_EVENT,
    payload: {
      sessionId: move.sessionId,
      continuationId: move.continuationId,
      fromDeviceId: move.fromDeviceId,
      toDeviceId: move.toDeviceId,
    },
    emittedBy: { server: true },
  })
}

const str = (value: unknown): string | null => (typeof value === `string` && value ? value : null)

/** The NEWEST move the continuation `sessionId` came from, and its hand-off. */
export async function moveStateOf(session: Pick<Session, `id` | `teamId`>): Promise<SessionMoveState | null> {
  const [moved] = await db
    .select()
    .from(events)
    .where(
      and(
        eq(events.teamId, session.teamId),
        eq(events.name, SESSION_MOVED_EVENT),
        eq(events.emittedBy, `server`),
        sql`${events.payload}->>'continuationId' = ${session.id}`
      )
    )
    .orderBy(desc(events.createdAt))
    .limit(1)
  if (!moved) return null
  const payload = moved.payload as Record<string, unknown>
  const [sessionId, continuationId, fromDeviceId, toDeviceId] = [
    str(payload.sessionId),
    str(payload.continuationId),
    str(payload.fromDeviceId),
    str(payload.toDeviceId),
  ]
  if (!sessionId || !continuationId || !fromDeviceId || !toDeviceId) return null
  const handed = await handOffOf(moved.teamId, moved.id)
  return {
    moveId: moved.id,
    sessionId,
    continuationId,
    fromDeviceId,
    toDeviceId,
    movedAt: moved.createdAt.toISOString(),
    handedOff: handed,
  }
}

async function handOffOf(teamId: string, moveId: string): Promise<SessionMoveState[`handedOff`]> {
  const [row] = await db
    .select()
    .from(events)
    .where(
      and(
        eq(events.teamId, teamId),
        eq(events.name, SESSION_HANDED_OFF_EVENT),
        eq(events.emittedBy, `server`),
        sql`${events.payload}->>'moveId' = ${moveId}`
      )
    )
    .orderBy(events.createdAt)
    .limit(1)
  if (!row) return null
  const payload = row.payload as Record<string, unknown>
  const outcome = handOffOutcomeValues.find((value) => value === payload.outcome) ?? `none`
  return { outcome, leftover: str(payload.leftover), at: row.createdAt.toISOString() }
}

/**
 * The SOURCE device's hand-off report: only the account its device runs
 * under (the move's `fromDeviceId` resolved like any session host) may
 * report it. Idempotent: the first report is THE record; a repeat (a
 * restarted daemon, the run's finish and the pickup both) returns it.
 */
export async function recordHandOff(
  userId: string,
  input: { moveId: string; outcome: HandOffOutcome; leftover?: string | null }
): Promise<{ outcome: HandOffOutcome; leftover: string | null }> {
  const [moved] = await db
    .select()
    .from(events)
    .where(and(eq(events.id, input.moveId), eq(events.name, SESSION_MOVED_EVENT), eq(events.emittedBy, `server`)))
    .limit(1)
  const payload = (moved?.payload ?? {}) as Record<string, unknown>
  const sessionId = str(payload.sessionId)
  const fromDeviceId = str(payload.fromDeviceId)
  if (!moved || !sessionId || !fromDeviceId) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Move not found` })
  }
  const [session] = await db
    .select({ userId: sessions.userId, teamId: sessions.teamId })
    .from(sessions)
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!session || (await sessionDeviceOwner({ ...session, deviceId: fromDeviceId })) !== userId) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Only the device the session moved from can hand it off` })
  }
  const leftover = input.outcome === `parked` ? (input.leftover ?? null) : null
  return db.transaction(async (tx) => {
    // One record per move, whoever reports first.
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`hand-off:${moved.id}`}))`)
    const existing = await handOffOf(moved.teamId, moved.id)
    if (existing) return { outcome: existing.outcome, leftover: existing.leftover }
    await emitEventInTx(tx, {
      teamId: moved.teamId,
      name: SESSION_HANDED_OFF_EVENT,
      payload: { moveId: moved.id, sessionId, fromDeviceId, outcome: input.outcome, ...(leftover ? { leftover } : {}) },
      emittedBy: { server: true },
    })
    return { outcome: input.outcome, leftover }
  })
}
