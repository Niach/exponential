import { TRPCError } from "@trpc/server"
import { and, desc, eq, gte, inArray, isNull, ne, notInArray, sql } from "drizzle-orm"
import { PERSISTENT_KINDS, TRIGGER_KINDS, isPersistent } from "@exp/domain-contract/session-kind"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { devices, messages, sessions, type Session } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import { insertMessage, postAgentMessage, postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import {
  getSteerRelayConfig,
  relayPostInterrupt,
  relayPostKill,
} from "@/lib/steer"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { reapplyOpenedAfterWorkInTx } from "@/lib/integrations/pr-sync"

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

/** Flips rows to ended (never a persistent one: EXP-1341 session-kind.json).
 *  Returns the ids it ended. */
export async function markEnded(tx: Tx, ids: string[]): Promise<string[]> {
  if (ids.length === 0) return []
  const now = new Date()
  const ended = await tx
    .update(sessions)
    .set({ status: `ended`, endedAt: now, busy: false, needsInput: false, updatedAt: now })
    .where(
      and(
        inArray(sessions.id, ids),
        ne(sessions.status, `ended`),
        notInArray(sessions.kind, [...PERSISTENT_KINDS])
      )
    )
    .returning({ id: sessions.id })
  return ended.map((row) => row.id)
}

/** A report's summary: the closing summary, else the child's last turn
 *  reply, else the fallback. */
export const REPORT_FALLBACK = `Ended without a summary.`
const REPORT_MAX = 4000

/**
 * EXP-1329: a child's end lands on its PARENT as a `report` card, agent-
 * authored by the child, `consumed_at` NULL: an idle parent resumes and reads
 * it as its next turn (the person-message path, `pendingInput`). Any parent,
 * any depth, an agent root too. Never for a `/btw` aside (it must not wake
 * anything) or a parent that has ended (pre-consumed: shown, never fed).
 */
export async function postReportToParent(
  tx: Tx,
  child: Pick<Session, `id` | `parentId` | `aside`>,
  summary?: string | null
): Promise<string | null> {
  if (!child.parentId || child.aside) return null
  const [parent] = await tx
    .select({
      id: sessions.id,
      rootId: sessions.rootId,
      teamId: sessions.teamId,
      shared: sessions.shared,
      userId: sessions.userId,
      status: sessions.status,
    })
    .from(sessions)
    .where(eq(sessions.id, child.parentId))
    .limit(1)
  if (!parent) return null
  let text = summary?.trim() ?? ``
  if (!text) {
    const [last] = await tx
      .select({ body: messages.body })
      .from(messages)
      .where(
        and(
          eq(messages.sessionId, child.id),
          eq(messages.authorKind, `agent`),
          isNull(messages.card),
          isNull(messages.authorSessionId)
        )
      )
      .orderBy(desc(messages.createdAt))
      .limit(1)
    text = last?.body.trim() ?? ``
  }
  text = (text || REPORT_FALLBACK).slice(0, REPORT_MAX)
  return insertMessage(tx, parent, {
    authorKind: `agent`,
    authorSessionId: child.id,
    body: text,
    card: { kind: `report`, sessionId: child.id, summary: text },
    consumedAt: parent.status === `ended` ? new Date() : null,
  })
}

/** EXP-1350: work in a thread stopped (ended, or a work child's turn is
 *  over): issues its follow-up took back to In Progress return to the
 *  PR-open target once nothing else in the tree works and the PR is live. */
export async function settleFollowUpIssuesInTx(
  tx: Tx,
  stopped: Pick<Session, `rootId`>,
  stoppedIds: string[],
  actorUserId: string | null
): Promise<void> {
  if (stoppedIds.length === 0) return
  await reapplyOpenedAfterWorkInTx(tx, { rootSessionId: stopped.rootId, stoppedIds, actorUserId })
}

/** Whether the session's host device heartbeat within the contract's
 *  online window (`devices.last_seen_at`). */
export async function sessionDeviceOnline(
  session: Pick<Session, `deviceId` | `userId` | `hostUserId`>,
  now: Date = new Date()
): Promise<boolean> {
  const since = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  const [row] = await db
    .select({ id: devices.id })
    .from(devices)
    .where(
      and(
        eq(devices.deviceId, session.deviceId),
        eq(devices.userId, session.hostUserId ?? session.userId),
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
    // A moved session re-queues on its target with `started_at` kept: Stop
    // never ends a session that already ran.
    if (session.status === `queued` && !session.startedAt && !isPersistent(session)) {
      const ended = await markEnded(tx, [session.id])
      await settleFollowUpIssuesInTx(tx, session, ended, args.userId)
      if (ended.length) {
        await postSystemCard(tx, session, {
          kind: `event`,
          event: `session_ended`,
          sessionId: session.id,
        })
      }
    } else if (ghost) {
      const idled = await tx
        .update(sessions)
        .set({ status: `idle`, busy: false, updatedAt: new Date() })
        .where(and(eq(sessions.id, session.id), ne(sessions.status, `ended`)))
        .returning({ id: sessions.id })
      // EXP-1350: its turn is over like any other: the issues its follow-up
      // took back to In Progress return once nothing else works.
      await settleFollowUpIssuesInTx(
        tx,
        session,
        idled.map((row) => row.id),
        args.userId
      )
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
  if (isPersistent(session)) {
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
    await settleFollowUpIssuesInTx(tx, session, endedIds, args.userId)
    if (endedIds.includes(session.id)) {
      await postSystemCard(tx, session, {
        kind: `event`,
        event: `session_ended`,
        sessionId: session.id,
      })
      await postReportToParent(tx, session, args.summary)
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
          notInArray(sessions.kind, [...PERSISTENT_KINDS]),
          ...(deviceId ? [eq(sessions.deviceId, deviceId)] : [])
        )
      )
      .returning({ id: sessions.id, rootId: sessions.rootId })
    // EXP-1350: an end is an end, whoever caused it: the issues a work
    // child's follow-up took back to In Progress return per thread.
    await settleStoppedByThreadInTx(tx, ended)
    return ended.map((row) => row.id)
  })
  await relayKillSessionsBestEffort(endedIds)
  return endedIds
}

/** `settleFollowUpIssuesInTx` once per thread of the stopped rows (an
 *  automation, no actor). */
async function settleStoppedByThreadInTx(
  tx: Tx,
  stopped: ReadonlyArray<{ id: string; rootId: string }>
): Promise<void> {
  const byRoot = new Map<string, string[]>()
  for (const row of stopped) {
    const ids = byRoot.get(row.rootId) ?? []
    ids.push(row.id)
    byRoot.set(row.rootId, ids)
  }
  for (const [rootId, ids] of byRoot) {
    await settleFollowUpIssuesInTx(tx, { rootId }, ids, null)
  }
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
        inArray(sessions.kind, [...TRIGGER_KINDS]),
        isNull(sessions.parentId),
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
 *  there any more; a message resumes them once the device is back), and an
 *  idle row's stale `busy` clears too (#FEED-85: a spinner nobody stops). */
export async function sweepStaleSessions(now: Date = new Date()): Promise<number> {
  const cutoff = new Date(now.getTime() - STALE_DEVICE_MS)
  return db.transaction(async (tx) => {
    const result = await tx.execute(sql`
      UPDATE sessions s
      SET status = 'idle', busy = false, updated_at = now()
      WHERE (s.status = 'running' OR (s.status = 'idle' AND s.busy))
        AND s.updated_at < ${cutoff}
        AND NOT EXISTS (
          SELECT 1 FROM devices d
          WHERE d.device_id = s.device_id
            AND d.user_id = coalesce(s.host_user_id, s.user_id)
            AND d.last_seen_at > ${cutoff}
        )
      RETURNING s.id, s.root_id
    `)
    const idled = (result.rows as Array<{ id: string; root_id: string }>).map(
      (row) => ({
        id: row.id,
        rootId: row.root_id,
      })
    )
    // EXP-1350: a work child idled here never reports its turn's end: the
    // issues its follow-up took back to In Progress would stay there.
    await settleStoppedByThreadInTx(tx, idled)
    return idled.length
  })
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
