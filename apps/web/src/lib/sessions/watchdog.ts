import { and, eq, inArray, isNull, lt, or, sql } from "drizzle-orm"
import type { CodingSessionBlocked } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { devices, messages, sessions } from "@/db/schema"
import { fireScheduledTriggers } from "@/lib/agents/triggers"
import { pruneEvents } from "@/lib/events"
import { BUS_COPY, busLine } from "@/lib/sessions/bus"
import { hostDeviceCondition, resolveSessionHost } from "@/lib/sessions/host"
import { pendingInput } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// NEW-1: the server watchdog (every minute; contract `sessionWatchdog`).
// Nothing waits on a device's good behaviour: what stalls is flagged on
// the row (`blocked.kind`), lands under Needs you (`needs_input`) and is
// said ONCE in the parent's conversation through the bus. Each flag has
// exactly one clearer: a claim (`unclaimed`), the next device report
// (`stuck`), the device's next heartbeat (`device_offline`).

export const WATCHDOG_INTERVAL_MS = 60 * 1000

const MIN = 60 * 1000

export interface WatchdogRun {
  unclaimed: number
  nudged: number
  stuck: number
  offline: number
  /** NEW-2: agent schedule firings this tick, and pruned event rows. */
  fired: number
  pruned: number
}

const LIVE = [`running`, `idle`] as const

function flag(kind: CodingSessionBlocked[`kind`], now: Date): CodingSessionBlocked {
  return { kind, agent: null, window: null, resetsAt: null, since: now.toISOString() }
}

/** `blocked` cleared and `needs_input` re-derived (an open card still waits). */
export async function clearBlockedInTx(tx: Tx, sessionIds: readonly string[]): Promise<void> {
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
    .where(inArray(sessions.id, [...sessionIds]))
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

/** A busy session with no device report for `stuckMinutes`: flagged once. */
async function flagStuck(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.stuckMinutes * MIN)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          eq(sessions.busy, true),
          isNull(sessions.blocked),
          lt(sessions.updatedAt, cutoff)
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
          // `hostDeviceCondition` names the devices table itself: no alias.
          sql`NOT EXISTS (SELECT 1 FROM ${devices} WHERE ${hostDeviceCondition(sessions, devices)} AND ${devices.lastSeenAt} >= ${cutoff})`
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

/** The device heartbeat: its `device_offline` sessions are live again. */
export async function restoreDeviceSessionsInTx(
  tx: Tx,
  device: { deviceId: string; userId: string }
): Promise<number> {
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        eq(sessions.deviceId, device.deviceId),
        sql`${sessions.blocked} ->> 'kind' = 'device_offline'`,
        or(
          eq(sessions.userId, device.userId),
          sql`EXISTS (SELECT 1 FROM ${devices} d WHERE d.device_id = ${device.deviceId} AND d.user_id = ${device.userId} AND ${sessions.teamId} = ANY(d.shared_team_ids))`
        )
      )
    )
  await clearBlockedInTx(
    tx,
    rows.map((row) => row.id)
  )
  return rows.length
}

export async function runWatchdog(now: Date = new Date()): Promise<WatchdogRun> {
  return {
    unclaimed: await flagUnclaimed(now),
    nudged: await nudgeStalled(now),
    stuck: await flagStuck(now),
    offline: await flagOffline(now),
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
          detail: `${run.unclaimed} unclaimed, ${run.nudged} nudged, ${run.stuck} stuck, ${run.offline} offline, ${run.fired} fired, ${run.pruned} events pruned`,
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

