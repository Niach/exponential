import { TRPCError } from "@trpc/server"
import { and, eq, inArray, ne, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { sessions, type Session } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import { postAgentMessage, postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import {
  getSteerRelayConfig,
  relayPostInterrupt,
  relayPostKill,
} from "@/lib/steer"
import { reportSchedulerRun } from "@/lib/metrics/registry"

// EXP-1291: the session lifecycle — queued → running ⇄ idle → ended.
// Merging ends nothing any more; a session ends only explicitly (End, the
// agent's own `sessions_end`, the consent behind a hosted run going away)
// and a root's End ends its whole tree. Stop interrupts the running TURN and
// leaves the session live. The DB row is the durable signal (the device
// watches its sessions over Electric); the relay frames only kill latency.

/** Best-effort relay kills for rows already flipped to ended. */
export async function relayKillSessionsBestEffort(sessionIds: string[]): Promise<void> {
  if (sessionIds.length === 0) return
  const config = getSteerRelayConfig()
  if (!config) return
  await Promise.all(sessionIds.map((id) => relayPostKill(config, id)))
}

/** The session and every live descendant (a root = its whole tree). */
async function liveSubtreeIds(tx: Tx, session: Session): Promise<string[]> {
  if (session.parentId === null) {
    const rows = await tx
      .select({ id: sessions.id })
      .from(sessions)
      .where(and(eq(sessions.rootId, session.id), ne(sessions.status, `ended`)))
    return rows.map((row) => row.id)
  }
  const result = await tx.execute(sql`
    WITH RECURSIVE sub(id) AS (
      SELECT id FROM sessions WHERE id = ${session.id}
      UNION ALL
      SELECT s.id FROM sessions s JOIN sub ON s.parent_id = sub.id
    )
    SELECT s.id FROM sessions s JOIN sub ON sub.id = s.id WHERE s.status <> 'ended'
  `)
  return (result.rows as Array<{ id: string }>).map((row) => row.id)
}

/** Flips rows to ended (never agents). Returns the ids it ended. */
export async function markEnded(tx: Tx, ids: string[]): Promise<string[]> {
  if (ids.length === 0) return []
  const now = new Date()
  const ended = await tx
    .update(sessions)
    .set({ status: `ended`, endedAt: now, busy: false, needsInput: false, updatedAt: now })
    .where(
      and(inArray(sessions.id, ids), ne(sessions.status, `ended`), eq(sessions.kind, `thread`))
    )
    .returning({ id: sessions.id })
  return ended.map((row) => row.id)
}

/** Stop: interrupt the current turn. A session that never started (queued)
 *  simply never starts. The row write gives the caller its txid. */
export async function stopSession(args: {
  userId: string
  sessionId: string
}): Promise<{ txid: number }> {
  const session = await loadSessionForUser(args.userId, args.sessionId)
  await assertCanSteer(args.userId, session)
  const txid = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    if (session.status === `queued` && session.kind === `thread`) {
      const ended = await markEnded(tx, [session.id])
      if (ended.length) {
        await postSystemCard(tx, session, {
          kind: `event`,
          event: `session_ended`,
          sessionId: session.id,
        })
      }
    } else {
      await tx.update(sessions).set({ updatedAt: new Date() }).where(eq(sessions.id, session.id))
    }
    return txid
  })
  if (session.status !== `queued` && session.status !== `ended`) {
    const config = getSteerRelayConfig()
    if (config) await relayPostInterrupt(config, session.id)
  }
  return { txid }
}

/** End: explicit and final (agents refuse: they idle, or are deleted).
 *  `summary` = the session's closing agent message (MCP `sessions_end`). */
export async function endSession(args: {
  userId: string
  sessionId: string
  summary?: string | null
}): Promise<{ txid: number; endedIds: string[] }> {
  const session = await loadSessionForUser(args.userId, args.sessionId)
  await assertCanSteer(args.userId, session)
  if (session.kind === `agent`) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `An agent never ends; delete it in Settings → Agents`,
    })
  }
  const result = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    if (args.summary?.trim() && session.status !== `ended`) {
      await postAgentMessage(tx, session, args.summary.trim())
    }
    const endedIds = await markEnded(tx, await liveSubtreeIds(tx, session))
    if (endedIds.includes(session.id)) {
      await postSystemCard(tx, session, {
        kind: `event`,
        event: `session_ended`,
        sessionId: session.id,
      })
    }
    return { txid, endedIds }
  })
  await relayKillSessionsBestEffort(result.endedIds)
  return result
}

/**
 * EXP-445: the consent behind a hosted run went away (the host unshared the
 * box, or the requester left the team): end every live THREAD session
 * hosted by `hostUserId` for someone else in `teamId` (narrowed to one
 * machine on unshare). Agents never end: `pauseDeviceAgents` disarms them.
 */
export async function endForeignHostedSessions(
  hostUserId: string,
  teamId: string,
  deviceId?: string
): Promise<string[]> {
  const endedIds = await db.transaction(async (tx) => {
    const now = new Date()
    const ended = await tx
      .update(sessions)
      .set({ status: `ended`, endedAt: now, busy: false, needsInput: false, updatedAt: now })
      .where(
        and(
          eq(sessions.hostUserId, hostUserId),
          ne(sessions.userId, hostUserId),
          eq(sessions.teamId, teamId),
          ne(sessions.status, `ended`),
          eq(sessions.kind, `thread`),
          ...(deviceId ? [eq(sessions.deviceId, deviceId)] : [])
        )
      )
      .returning({ id: sessions.id })
    return ended.map((row) => row.id)
  })
  await relayKillSessionsBestEffort(endedIds)
  return endedIds
}

/**
 * SLOP-2/EXP-1291: a share withdrawn from `teamIds` leaves those teams'
 * agents on this device running on the owner's credentials with nobody able
 * to stop them (agent settings are team-owner-only). Disarm them in the
 * caller's transaction: every trigger disabled, the agent idle. Their
 * owners re-arm them on another device.
 */
export async function pauseDeviceAgents(
  tx: Tx,
  teamIds: string[],
  deviceId: string
): Promise<number> {
  if (teamIds.length === 0) return 0
  const paused = await tx
    .update(sessions)
    .set({
      triggers: sql`(SELECT coalesce(jsonb_agg(t || '{"enabled": false}'::jsonb), '[]'::jsonb) FROM jsonb_array_elements(${sessions.triggers}) t)`,
      status: `idle`,
      busy: false,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(sessions.kind, `agent`),
        eq(sessions.deviceId, deviceId),
        inArray(sessions.teamId, teamIds)
      )
    )
    .returning({ id: sessions.id })
  return paused.length
}

// ── the stale sweep ───────────────────────────────────────────────────────────

/** A device silent this long is gone: its running sessions read idle. */
export const STALE_DEVICE_MS = 2 * 60 * 60 * 1000
const SWEEP_INTERVAL_MS = 10 * 60 * 1000

/** Running sessions whose device stopped heartbeating go idle (nothing runs
 *  there any more; a message resumes them once the device is back). */
export async function sweepStaleSessions(now: Date = new Date()): Promise<number> {
  const cutoff = new Date(now.getTime() - STALE_DEVICE_MS)
  const result = await db.execute(sql`
    UPDATE sessions s
    SET status = 'idle', busy = false, updated_at = now()
    WHERE s.status = 'running'
      AND s.updated_at < ${cutoff}
      AND NOT EXISTS (
        SELECT 1 FROM devices d
        WHERE d.device_id = s.device_id
          AND d.user_id = coalesce(s.host_user_id, s.user_id)
          AND d.last_seen_at > ${cutoff}
      )
    RETURNING s.id
  `)
  return result.rows.length
}

let sweepStarted = false

export function startSessionSweepScheduler(): void {
  if (sweepStarted) return
  sweepStarted = true
  setInterval(() => {
    const started = performance.now()
    sweepStaleSessions()
      .then((count) =>
        reportSchedulerRun(`session-sweep`, {
          ok: true,
          durationMs: performance.now() - started,
          detail: `${count} idled`,
        })
      )
      .catch((error: unknown) => {
        reportSchedulerRun(`session-sweep`, {
          ok: false,
          durationMs: performance.now() - started,
          error: String(error),
        })
        console.error(`[session-sweep] failed:`, error)
      })
  }, SWEEP_INTERVAL_MS)
}
