import { and, eq, inArray, isNull, lt, notInArray, sql } from "drizzle-orm"
import type { CodingSessionBlocked } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { devices, messages, sessions } from "@/db/schema"
import { fireScheduledTriggers } from "@/lib/agents/triggers"
import { pruneEvents } from "@/lib/events"
import { BUS_COPY, busLine } from "@/lib/sessions/bus"
import { hostRowCondition, hostedByCondition, resolveSessionHost } from "@/lib/sessions/host"
import { pendingInput } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// NEW-1: the server watchdog (every minute; contract `sessionWatchdog`).
// Nothing waits on a device's good behaviour: what stalls is flagged on
// the row (`blocked.kind`), lands under Needs you (`needs_input`) and is
// said ONCE in the parent's conversation through the bus. Each flag has
// exactly one clearer: a claim (`unclaimed`), the next device report
// (`stuck`), the device's next heartbeat (`device_offline`). FEED-129: the
// device-written usage wall (`rate_limit`) is the device's to clear, or
// this watchdog's once its `resetsAt` passed.

export const WATCHDOG_INTERVAL_MS = 60 * 1000

const MIN = 60 * 1000
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export interface WatchdogRun {
  unclaimed: number
  nudged: number
  stuck: number
  offline: number
  /** FEED-129: usage walls whose reset time passed, cleared this tick. */
  expired: number
  /** NEW-2: agent schedule firings this tick, and pruned event rows. */
  fired: number
  pruned: number
}

const LIVE = [`running`, `idle`] as const

function flag(kind: CodingSessionBlocked[`kind`], now: Date): CodingSessionBlocked {
  return { kind, agent: null, window: null, resetsAt: null, since: now.toISOString() }
}

/** `blocked` cleared and `needs_input` re-derived (an open card still waits).
 *  `kind` (FEED-146) clears only a flag of that kind: a usage wall the
 *  device wrote in between is never wiped by another clearer. */
export async function clearBlockedInTx(
  tx: Tx,
  sessionIds: readonly string[],
  kind?: CodingSessionBlocked[`kind`],
  /** EXP-1376: only a wall of this usage window (a lifted MODEL wall). */
  window?: string
): Promise<void> {
  if (sessionIds.length === 0) return
  await tx
    .update(sessions)
    .set({
      blocked: null,
      // Any open question/plan card still waits: the device's (`sessions.reply`)
      // or the session's own MCP one (same count as `stampAnsweredCardInTx`).
      needsInput: sql`EXISTS (SELECT 1 FROM ${messages} m WHERE m.session_id = ${sessions.id} AND m.answered_at IS NULL AND m.card->>'kind' IN ('question', 'plan'))`,
      updatedAt: new Date(),
    })
    .where(
      and(
        inArray(sessions.id, [...sessionIds]),
        ...(kind ? [sql`${sessions.blocked} ->> 'kind' = ${kind}`] : []),
        ...(window ? [sql`${sessions.blocked} ->> 'window' = ${window}`] : [])
      )
    )
}

/** A queued session nobody claimed in `unclaimedMinutes`: flagged once. */
async function flagUnclaimed(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.unclaimedMinutes * MIN)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          eq(sessions.status, `queued`),
          isNull(sessions.startedAt),
          isNull(sessions.blocked),
          lt(sessions.createdAt, cutoff)
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({ blocked: flag(`unclaimed`, now), needsInput: true, updatedAt: now })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `session_unclaimed`, BUS_COPY.session_unclaimed, { waking: true })
    }
    return rows.length
  })
}

/** A live session with pending input older than `nudgeMinutes`: its host
 *  device is nudged again (no row write; every tick while it lasts). */
async function nudgeStalled(now: Date): Promise<number> {
  const config = getSteerRelayConfig()
  if (!config) return 0
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.nudgeMinutes * MIN)
  const rows = await db
    .select({
      id: sessions.id,
      deviceId: sessions.deviceId,
      userId: sessions.userId,
      teamId: sessions.teamId,
    })
    .from(sessions)
    .where(
      and(
        inArray(sessions.status, [...LIVE]),
        // NULL-safe: an unblocked row has no kind at all.
        sql`(${sessions.blocked} ->> 'kind') IS DISTINCT FROM 'device_offline'`,
        // `pendingInput()` names the messages table itself: no alias here.
        sql`EXISTS (SELECT 1 FROM ${messages} WHERE ${messages.sessionId} = ${sessions.id} AND ${messages.consumedAt} IS NULL AND ${messages.createdAt} < ${cutoff} AND (${pendingInput()}))`
      )
    )
  const seen = new Set<string>()
  for (const row of rows) {
    const host = await resolveSessionHost(row)
    if (!host) continue
    const key = `${host}:${row.deviceId}`
    if (seen.has(key)) continue
    seen.add(key)
    void relayPostNudge(config, host, row.deviceId)
  }
  return seen.size
}

/** A busy session with no device report for `stuckMinutes`: flagged once.
 *  FEED-145: a device only reports on a state edge, so a long turn (a
 *  subagent fan-out, a build) is report-free by design. A session its
 *  ONLINE host lists in the heartbeat's `liveSessionIds` has a live process
 *  whose own stall watchdog (FEED-25) guards the turn: never flagged here
 *  (it read "needs input" while busy). An older device that never claims
 *  keeps the 30-minute rule. */
async function flagStuck(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.stuckMinutes * MIN)
  const online = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          eq(sessions.busy, true),
          isNull(sessions.blocked),
          lt(sessions.updatedAt, cutoff),
          // FEED-148: the resolved host's row only (`hostRowCondition`): a
          // decoy row naming the machine never vouches for a session.
          sql`NOT EXISTS (SELECT 1 FROM ${devices} h WHERE ${hostRowCondition(`h`)} AND h.last_seen_at >= ${online} AND ${sessions.id}::text = ANY(h.live_session_ids))`
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({ blocked: flag(`stuck`, now), needsInput: true, updatedAt: now })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `session_stuck`, BUS_COPY.session_stuck, { waking: true })
    }
    return rows.length
  })
}

/** A live session whose host device missed the online window: idle, not
 *  busy, `device_offline` (its next heartbeat clears it). */
async function flagOffline(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          isNull(sessions.blocked),
          // FEED-148: the resolved host's row only: a decoy row online
          // never keeps a dead host's sessions live.
          sql`NOT EXISTS (SELECT 1 FROM ${devices} h WHERE ${hostRowCondition(`h`)} AND h.last_seen_at >= ${cutoff})`
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({
          status: `idle`,
          busy: false,
          blocked: flag(`device_offline`, now),
          needsInput: true,
          updatedAt: now,
        })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `device_offline`, BUS_COPY.device_offline, { waking: false })
    }
    return rows.length
  })
}

/** FEED-129: a usage wall (`rate_limit`) whose reset time passed: cleared.
 *  The device clears a wall on the next assistant token, but a walled run
 *  whose engine went away (an account switch that failed, a host restart)
 *  has nobody to do it, and the row read "Usage limit" hours after the
 *  window reopened. A wall naming no reset stays the device's. Silent: the
 *  bus said the wall once; its passing is no line. */
async function clearExpiredWalls(now: Date): Promise<number> {
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, blocked: sessions.blocked })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          sql`${sessions.blocked} ->> 'kind' = 'rate_limit'`,
          sql`${sessions.blocked} ->> 'resetsAt' IS NOT NULL`
        )
      )
      .for(`update`, { skipLocked: true })
    // Parsed here, never cast in SQL: the stamp is a free string from the
    // device, and a malformed one must not fail the whole tick.
    const expired = rows.filter((row) => {
      const at = Date.parse(row.blocked?.resetsAt ?? ``)
      return Number.isFinite(at) && at <= now.getTime()
    })
    await clearBlockedInTx(
      tx,
      expired.map((row) => row.id),
      `rate_limit`
    )
    return expired.length
  })
}

/** FEED-146: the heartbeat's `liveSessionIds` is a free list from the
 *  device: only distinct uuids (lower-cased, as Postgres prints them) can
 *  name a session row; anything else is dropped, never cast in SQL (one bad
 *  id would fail every beat). FEED-148: a list of nothing but junk cleans
 *  to `[]`, which is still a claim ("I run nothing"): every busy session
 *  the sender hosts there reads dead. Accepted on purpose: only the host's
 *  own device can send it, and a broken client fails loudly, not silently
 *  keeps dead sessions busy. */
export function cleanLiveSessionIds(ids: readonly unknown[]): { ids: string[]; dropped: number } {
  const valid = ids.filter((id): id is string => typeof id === `string` && UUID.test(id))
  return { ids: [...new Set(valid.map((id) => id.toLowerCase()))], dropped: ids.length - valid.length }
}

/** The heartbeat's device: its sender and the machine id it beats for. */
export interface BeatingDevice {
  deviceId: string
  userId: string
}

/** FEED-147: the sessions a heartbeat may move: on that machine AND hosted
 *  by its sender (`hostedByCondition`), never every row naming the id. */
function beatScope(device: BeatingDevice) {
  return and(eq(sessions.deviceId, device.deviceId), hostedByCondition(device.userId))!
}

/** The device heartbeat: its `device_offline` sessions are live again. */
export async function restoreDeviceSessionsInTx(tx: Tx, device: BeatingDevice): Promise<number> {
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(beatScope(device), sql`${sessions.blocked} ->> 'kind' = 'device_offline'`))
    .for(`update`, { skipLocked: true })
  await clearBlockedInTx(
    tx,
    rows.map((row) => row.id),
    `device_offline`
  )
  return rows.length
}

/** FEED-145: a session the heartbeat lists in `liveSessionIds` is alive: a
 *  `stuck` flag on it (the 30-minute rule's, from before its device
 *  claimed) is cleared and `needs_input` re-derived from its open cards. */
export async function clearLiveStuckInTx(
  tx: Tx,
  device: BeatingDevice & { liveSessionIds: readonly string[] }
): Promise<number> {
  const live = cleanLiveSessionIds(device.liveSessionIds).ids
  if (live.length === 0) return 0
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        beatScope(device),
        inArray(sessions.id, live),
        inArray(sessions.status, [...LIVE]),
        sql`${sessions.blocked} ->> 'kind' = 'stuck'`
      )
    )
    .for(`update`, { skipLocked: true })
  await clearBlockedInTx(
    tx,
    rows.map((row) => row.id),
    `stuck`
  )
  return rows.length
}

/** NEW-5: the heartbeat's `liveSessionIds` = the sessions the host process
 *  runs. A `running` + `busy` session on that device that is NOT listed has
 *  a dead process (the daemon lives, the run does not): idle, not busy,
 *  `stuck` (its next report or claim clears it), said once to its parent. */
export async function flagDeadSessionsInTx(
  tx: Tx,
  device: BeatingDevice & { liveSessionIds: readonly string[] },
  now: Date = new Date()
): Promise<number> {
  const live = cleanLiveSessionIds(device.liveSessionIds).ids
  const rows = await tx
    .select({ id: sessions.id, parentId: sessions.parentId })
    .from(sessions)
    .where(
      and(
        beatScope(device),
        eq(sessions.status, `running`),
        eq(sessions.busy, true),
        ...(live.length > 0 ? [notInArray(sessions.id, live)] : [])
      )
    )
    .for(`update`, { skipLocked: true })
  for (const row of rows) {
    await tx
      .update(sessions)
      .set({
        status: `idle`,
        busy: false,
        blocked: flag(`stuck`, now),
        needsInput: true,
        updatedAt: now,
      })
      .where(eq(sessions.id, row.id))
    await busLine(tx, row, `session_stuck`, BUS_COPY.session_stuck, { waking: true })
  }
  return rows.length
}

export async function runWatchdog(now: Date = new Date()): Promise<WatchdogRun> {
  return {
    unclaimed: await flagUnclaimed(now),
    nudged: await nudgeStalled(now),
    stuck: await flagStuck(now),
    offline: await flagOffline(now),
    expired: await clearExpiredWalls(now),
    // NEW-2: the same tick fires the agents' schedules (a missed tick is
    // caught up here) and sweeps the event stream.
    fired: await fireScheduledTriggers(now),
    pruned: await pruneEvents(now),
  }
}

let started = false

export function startSessionWatchdog(): void {
  if (started) return
  started = true
  setInterval(() => {
    const begun = performance.now()
    runWatchdog()
      .then((run) =>
        reportSchedulerRun(`session-watchdog`, {
          ok: true,
          durationMs: performance.now() - begun,
          detail: `${run.unclaimed} unclaimed, ${run.nudged} nudged, ${run.stuck} stuck, ${run.offline} offline, ${run.expired} walls expired, ${run.fired} fired, ${run.pruned} events pruned`,
        })
      )
      .catch((error: unknown) => {
        reportSchedulerRun(`session-watchdog`, {
          ok: false,
          durationMs: performance.now() - begun,
          error: String(error),
        })
        console.error(`[session-watchdog] failed:`, error)
      })
  }, WATCHDOG_INTERVAL_MS)
}

