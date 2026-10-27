import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, gte, inArray, isNull, ne, sql } from "drizzle-orm"
import { isPersistent } from "@exp/domain-contract/session-kind"
import { db } from "@/db/connection"
import { agents, attachments, devices, messages, sessions, type Session } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { emitEventInTx } from "@/lib/events"
import { assertCanSteer, loadSessionForUser, sessionVisibleTo } from "@/lib/sessions/access"
import { getTeamMember } from "@/lib/auth/membership"
import { busReport, closeAskMirrorsOf, forgetWatched } from "@/lib/sessions/bus"
import { postAgentMessage, postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { hostedByCondition, resolveSessionHost } from "@/lib/sessions/host"
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
  const endedIds = ended.map((row) => row.id)
  await closeAskMirrorsOf(tx, endedIds, now)
  return endedIds
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
    // NEW-2: the builtin `session.ended` event. #EXP-1409: the row syncs
    // to the whole team, so a PRIVATE session's carries its ids only (its
    // title is often the prompt); the full payload reaches only a trigger
    // firing whose readers may see the session (`askHidden`).
    const [endedRow] = await tx
      .select({
        teamId: sessions.teamId,
        rootId: sessions.rootId,
        title: sessions.title,
        branch: sessions.branch,
        shared: sessions.shared,
        userId: sessions.userId,
      })
      .from(sessions)
      .where(eq(sessions.id, row.id))
      .limit(1)
    if (endedRow) {
      const ids = { sessionId: row.id, rootId: endedRow.rootId }
      const full = { ...ids, title: endedRow.title, branch: endedRow.branch }
      await emitEventInTx(tx, {
        teamId: endedRow.teamId,
        name: `session.ended`,
        payload: endedRow.shared ? full : ids,
        ...(endedRow.shared ? {} : { privateDetail: { origin: endedRow, payload: full } }),
        emittedBy: { server: true },
      })
    }
  }
  await forgetWatched(tx, endedIds)
  return endedIds
}

/** Whether the session's host device heartbeat within the contract's
 *  online window (`devices.last_seen_at`). FEED-148: the resolved host's
 *  row only, never a decoy row naming the same machine. */
export async function sessionDeviceOnline(
  session: Pick<Session, `deviceId` | `userId` | `teamId`>,
  now: Date = new Date()
): Promise<boolean> {
  const since = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  const host = await resolveSessionHost(session)
  if (!host) return false
  const [row] = await db
    .select({ id: devices.id })
    .from(devices)
    .where(and(eq(devices.deviceId, session.deviceId), eq(devices.userId, host), gte(devices.lastSeenAt, since)))
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
}): Promise<{ txid: number; delivered: boolean }> {
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
  // Chats: `delivered` = the device got the interrupt; a queued, ghost,
  // ended or idle (not busy) session has nothing to deliver (true).
  if (session.status === `queued` || session.status === `ended` || ghost) return { txid, delivered: true }
  const config = getSteerRelayConfig()
  if (!config) return { txid, delivered: !session.busy }
  const { delivered } = await relayPostInterrupt(config, session.id)
  return { txid, delivered: delivered || !session.busy }
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

/**
 * Chats (fixtures/chats.json menus): delete a ROOT chat and its whole tree
 * (not an agent's conversation, not an app session). The starter or a team
 * owner. What is live ends first (each row's `session_ended` card, its bus
 * report and the `session.ended` event, like End), then the root goes and
 * the FK cascades take the children, conversations, messages, links and
 * reads; pull requests, events, notifications and drafts keep their rows
 * (SET NULL). Message files fall back to owner-less uploads the orphan
 * sweep reclaims.
 */
export async function deleteSession(args: { userId: string; sessionId: string }): Promise<{ txid: number }> {
  const [session] = await db.select().from(sessions).where(eq(sessions.id, args.sessionId)).limit(1)
  const member = session ? await getTeamMember(args.userId, session.teamId) : undefined
  if (!session || !member) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  const owner = member.role === `owner`
  if (!owner && !sessionVisibleTo(session, args.userId)) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  }
  if (session.parentId !== null || session.agentId !== null || session.appId !== null) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Only a chat's root session can be deleted` })
  }
  if (!owner && session.userId !== args.userId) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Only the chat's starter or a team owner can delete it` })
  }
  const result = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const endedIds = await endRowsInTx(tx, await liveSubtree(tx, session))
    const tree = await tx.select({ id: sessions.id }).from(sessions).where(eq(sessions.rootId, session.id))
    await forgetWatched(tx, tree.map((row) => row.id))
    await closeAskMirrorsOf(tx, tree.map((row) => row.id))
    await tx
      .update(attachments)
      .set({ messageId: null })
      .where(
        inArray(
          attachments.messageId,
          tx.select({ id: messages.id }).from(messages).where(eq(messages.rootId, session.id))
        )
      )
    await tx.delete(sessions).where(eq(sessions.id, session.id))
    return { txid, endedIds }
  })
  await relayKillSessionsBestEffort(result.endedIds)
  return { txid: result.txid }
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
 * EXP-445: the consent behind a hosted run is going away (the host unshares
 * the box, leaves the team, or a team owner drops the share): every live
 * THREAD session someone else started in `teamId` on the machines in
 * `deviceIds` that `hostUserId` HOSTS. Agents never end: `pauseDeviceAgents`
 * disarms them. FEED-148: only the sessions `hostUserId` hosts (the resolved
 * host, `hostedByCondition`), never every session on a machine id they
 * merely registered too. FEED-149: read in the share write's transaction,
 * after `lockDeviceShares` and BEFORE the write, so no other account's share
 * can slip in and change who hosts between the read and the end; hand the
 * ids to `endHostedSessions` after commit.
 */
export async function foreignHostedSessionIds(
  tx: Tx,
  hostUserId: string,
  teamId: string,
  deviceIds: readonly string[]
): Promise<string[]> {
  if (deviceIds.length === 0) return []
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        ne(sessions.userId, hostUserId),
        eq(sessions.teamId, teamId),
        ne(sessions.status, `ended`),
        isNull(sessions.agentId),
        inArray(sessions.deviceId, [...deviceIds]),
        hostedByCondition(hostUserId, sessions)
      )
    )
  return rows.map((row) => row.id)
}

/** FEED-149: end the set `foreignHostedSessionIds` read, after the share
 *  write committed (rows that ended meanwhile stay as they are). */
export async function endHostedSessions(ids: readonly string[]): Promise<string[]> {
  if (ids.length === 0) return []
  const endedIds = await db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId, depth: sessions.depth, agentId: sessions.agentId })
      .from(sessions)
      .where(and(inArray(sessions.id, [...ids]), ne(sessions.status, `ended`)))
      .orderBy(desc(sessions.depth), asc(sessions.createdAt))
    return endRowsInTx(tx, rows)
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
