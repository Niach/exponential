import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, gte, inArray, isNull, ne, or, sql } from "drizzle-orm"
import { isPersistent } from "@exp/domain-contract/session-kind"
import { db } from "@/db/connection"
import { agents, devices, sessions, type Session } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { emitEventInTx } from "@/lib/events"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import { busReport, forgetWatched } from "@/lib/sessions/bus"
import { postAgentMessage, postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { getSteerRelayConfig, relayPostInterrupt, relayPostKill } from "@/lib/steer"
import { contract } from "@exp/domain-contract"

// EXP-1291/NEW-1: the session lifecycle — queued → running ⇄ idle → ended.
// Merging ends nothing; a session ends only explicitly (End, the agent's
// own `sessions_end`, the consent behind a hosted run going away) or when
// its process is gone. A root's End ends its whole tree, deepest first,
// each child reporting into its parent (and watchers) through the bus
// before its row ends; a child's End ends only its subtree. Stop
// interrupts the running TURN and leaves the session live. The DB row is
// the durable signal (the device watches its sessions over Electric); the
// relay frames only kill latency. What stalls is the watchdog's business.

/** Best-effort relay kills for rows already flipped to ended. */
export async function relayKillSessionsBestEffort(sessionIds: string[]): Promise<void> {
  if (sessionIds.length === 0) return
  const config = getSteerRelayConfig()
  if (!config) return
  await Promise.all(sessionIds.map((id) => relayPostKill(config, id)))
}

type TreeRow = Pick<Session, `id` | `parentId` | `depth` | `agentId`>

/** The session and every live descendant (a root = its whole tree),
 *  deepest first. */
async function liveSubtree(tx: Tx, session: Pick<Session, `id` | `parentId`>): Promise<TreeRow[]> {
  const fields = { id: sessions.id, parentId: sessions.parentId, depth: sessions.depth, agentId: sessions.agentId }
  if (session.parentId === null) {
    return tx
      .select(fields)
      .from(sessions)
      .where(and(eq(sessions.rootId, session.id), ne(sessions.status, `ended`)))
      .orderBy(desc(sessions.depth), asc(sessions.createdAt))
  }
  const result = await tx.execute(sql`
    WITH RECURSIVE sub(id) AS (
      SELECT id FROM sessions WHERE id = ${session.id}
      UNION ALL
      SELECT s.id FROM sessions s JOIN sub ON s.parent_id = sub.id
    )
    SELECT s.id, s.parent_id AS "parentId", s.depth, s.agent_id AS "agentId"
    FROM sessions s JOIN sub ON sub.id = s.id
    WHERE s.status <> 'ended'
    ORDER BY s.depth DESC, s.created_at ASC
  `)
  return result.rows as unknown as TreeRow[]
}

/** Flips rows to ended (never a persistent one: an agent's conversation,
 *  session-kind.json). Returns the ids it ended. */
export async function markEnded(tx: Tx, ids: string[]): Promise<string[]> {
  if (ids.length === 0) return []
  const now = new Date()
  const ended = await tx
    .update(sessions)
    .set({ status: `ended`, endedAt: now, busy: false, needsInput: false, blocked: null, updatedAt: now })
    .where(and(inArray(sessions.id, ids), ne(sessions.status, `ended`), isNull(sessions.agentId)))
    .returning({ id: sessions.id })
  return ended.map((row) => row.id)
}

/**
 * Ends `rows` (deepest first, as `liveSubtree` orders them): each row that
 * flips posts its `session_ended` card and reports into its parent and
 * watchers (the bus), `summary` on the named session only. Returns the
 * ended ids.
 */
export async function endRowsInTx(
  tx: Tx,
  rows: readonly TreeRow[],
  named?: { id: string; summary?: string | null } | null
): Promise<string[]> {
  const endedIds: string[] = []
  for (const row of rows) {
    if (isPersistent(row)) continue
    const ended = await markEnded(tx, [row.id])
    if (ended.length === 0) continue
    endedIds.push(row.id)
    await postSystemCard(tx, row.id, { kind: `event`, event: `session_ended`, sessionId: row.id })
    await busReport(tx, row, named?.id === row.id ? named.summary : null)
    // NEW-2: the builtin `session.ended` event.
    const [endedRow] = await tx
      .select({ teamId: sessions.teamId, rootId: sessions.rootId, title: sessions.title, branch: sessions.branch })
      .from(sessions)
      .where(eq(sessions.id, row.id))
      .limit(1)
    if (endedRow) {
      await emitEventInTx(tx, {
        teamId: endedRow.teamId,
        name: `session.ended`,
        payload: { sessionId: row.id, rootId: endedRow.rootId, title: endedRow.title, branch: endedRow.branch },
        emittedBy: { server: true },
      })
    }
  }
  await forgetWatched(tx, endedIds)
  return endedIds
}

/** Whether the session's host device heartbeat within the contract's
 *  online window (`devices.last_seen_at`). */
export async function sessionDeviceOnline(
  session: Pick<Session, `deviceId` | `userId` | `teamId`>,
  now: Date = new Date()
): Promise<boolean> {
  const since = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  const [row] = await db
    .select({ id: devices.id })
    .from(devices)
    .where(
      and(
        eq(devices.deviceId, session.deviceId),
        or(eq(devices.userId, session.userId), sql`${session.teamId} = ANY(${devices.sharedTeamIds})`),
        gte(devices.lastSeenAt, since)
      )
    )
    .limit(1)
  return !!row
}

/** Stop: interrupt the current turn. A session that never started (queued)
 *  simply never starts. The row write gives the caller its txid. #FEED-85: a
 *  live row whose device is OFFLINE has no turn to interrupt and nobody to
 *  clear its state: Stop frees it (idle, not busy) so a message resumes it
 *  once the device is back. */
export async function stopSession(args: {
  userId: string
  sessionId: string
}): Promise<{ txid: number }> {
  const session = await loadSessionForUser(args.userId, args.sessionId)
  await assertCanSteer(args.userId, session)
  const live = session.status === `running` || session.status === `idle`
  const ghost = live && !(await sessionDeviceOnline(session))
  const txid = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    // A re-queued session keeps `started_at`: Stop never ends a session
    // that already ran.
    if (session.status === `queued` && !session.startedAt && !isPersistent(session)) {
      await endRowsInTx(tx, [session])
    } else if (ghost) {
      await tx
        .update(sessions)
        .set({ status: `idle`, busy: false, updatedAt: new Date() })
        .where(and(eq(sessions.id, session.id), ne(sessions.status, `ended`)))
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

/** End: explicit and final (agents refuse: they idle, or are deleted). A
 *  root ends its tree deepest first, every child reporting into its parent
 *  before it goes. `summary` = the session's closing agent message (MCP
 *  `sessions_end`). */
export async function endSession(args: {
  userId: string
  sessionId: string
  summary?: string | null
}): Promise<{ txid: number; endedIds: string[] }> {
  const session = await loadSessionForUser(args.userId, args.sessionId)
  await assertCanSteer(args.userId, session)
  if (isPersistent(session)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `An agent's conversation never ends; delete the agent in Settings → Agents`,
    })
  }
  const result = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    if (args.summary?.trim() && session.status !== `ended`) {
      await postAgentMessage(tx, session, args.summary.trim())
    }
    const endedIds = await endRowsInTx(tx, await liveSubtree(tx, session), {
      id: session.id,
      summary: args.summary,
    })
    return { txid, endedIds }
  })
  await relayKillSessionsBestEffort(result.endedIds)
  return result
}

/** The device reported its process gone: the row ends like an explicit End
 *  of that one session (its live children keep running and report into an
 *  ended parent, pre-consumed). Returns whether THIS call ended it. Only a
 *  CHILD gets here: a thread's root and an agent's conversation survive
 *  their process (`sessions.report` turns their `ended` into `idle`,
 *  FEED-134) and a message resumes the same row. */
export async function endDeadSessionInTx(tx: Tx, session: TreeRow): Promise<boolean> {
  const ended = await endRowsInTx(tx, [session])
  return ended.length > 0
}

/**
 * EXP-445: the consent behind a hosted run went away (the host unshared the
 * box, or the requester left the team): end every live THREAD session
 * someone else started in `teamId` on a device `hostUserId` owns (narrowed
 * to one machine on unshare). Agents never end: `pauseDeviceAgents` disarms
 * them.
 */
export async function endForeignHostedSessions(
  hostUserId: string,
  teamId: string,
  deviceId?: string
): Promise<string[]> {
  const endedIds = await db.transaction(async (tx) => {
    const hosted = await tx
      .select({ id: sessions.id, parentId: sessions.parentId, depth: sessions.depth, agentId: sessions.agentId })
      .from(sessions)
      .where(
        and(
          ne(sessions.userId, hostUserId),
          eq(sessions.teamId, teamId),
          ne(sessions.status, `ended`),
          isNull(sessions.agentId),
          sql`EXISTS (SELECT 1 FROM ${devices} d WHERE d.user_id = ${hostUserId} AND d.device_id = ${sessions.deviceId})`,
          // The starter's own row for the machine makes the starter the host.
          sql`NOT EXISTS (SELECT 1 FROM ${devices} d WHERE d.user_id = ${sessions.userId} AND d.device_id = ${sessions.deviceId})`,
          ...(deviceId ? [eq(sessions.deviceId, deviceId)] : [])
        )
      )
      .orderBy(desc(sessions.depth), asc(sessions.createdAt))
    return endRowsInTx(tx, hosted)
  })
  await relayKillSessionsBestEffort(endedIds)
  return endedIds
}

/**
 * SLOP-2/EXP-1291/NEW-2: a share withdrawn from `teamIds` leaves those
 * teams' agents on this device running on the owner's credentials with
 * nobody able to stop them (agent settings are team-owner-only). Disarm
 * them in the caller's transaction: every trigger of an `agents` row on
 * the device disabled, its conversations idle. Their owners re-arm them on
 * another device.
 */
export async function pauseDeviceAgents(
  tx: Tx,
  teamIds: string[],
  deviceId: string
): Promise<number> {
  if (teamIds.length === 0) return 0
  const paused = await tx
    .update(agents)
    .set({
      triggers: sql`(SELECT coalesce(jsonb_agg(t || '{"enabled": false}'::jsonb), '[]'::jsonb) FROM jsonb_array_elements(${agents.triggers}) t)`,
      updatedAt: new Date(),
    })
    .where(and(eq(agents.deviceId, deviceId), inArray(agents.teamId, teamIds)))
    .returning({ id: agents.id })
  if (paused.length > 0) {
    await tx
      .update(sessions)
      .set({ status: `idle`, busy: false, updatedAt: new Date() })
      .where(
        and(
          inArray(sessions.agentId, paused.map((row) => row.id)),
          isNull(sessions.parentId),
          ne(sessions.status, `ended`)
        )
      )
  }
  return paused.length
}
