// #EXP-1798: the guard rails in front of every destructive op of the 22
// nouns (@exp/emcp `fixtures/deletes.json`): ONE explicit id per call, and
// the burst guard (default 5 per 60 s per caller key, `MCP_DELETE_BURST_*`)
// that refuses the excess and posts `mcp.delete_burst_blocked` to the team
// stream. The noun router (nouns.ts) asks before the op's handler runs, so
// no op's own code (or a sibling change to it) can skip the guard.
import { createHash } from "node:crypto"
import { eq } from "drizzle-orm"
import {
  burstAdmit,
  burstLimits,
  burstMessage,
  DELETE_BURST_EVENT,
  destructiveKey,
  newBurstState,
  singleIdError,
  singleIdMessage,
  type BurstLimits,
  type BurstState,
} from "@exp/emcp/deletes"
import { db } from "@/db/connection"
import { sessions } from "@/db/schema"
import { getPersonalTeamId } from "@/lib/auth/membership"
import { emitEventInTx } from "@/lib/events"
import type { McpAccess } from "@/lib/emcp/scope"

// Per server process; a key's window empties itself as calls age out.
const windows = new Map<string, BurstState>()
const SWEEP_AT = 1_000

/** The caller key (deletes.json `burstKey`): the caller's own verified
 *  session, else the credential (a hash of its Authorization header). */
export function deleteBurstKey(userId: string, verifiedSessionId: string | null, authorization: string | null): string {
  if (verifiedSessionId) return `session:${verifiedSessionId}`
  const token = authorization
    ? createHash(`sha256`).update(authorization, `utf8`).digest(`hex`).slice(0, 16)
    : `cookie`
  return `user:${userId}|token:${token}`
}

function sweep(nowMs: number, limits: BurstLimits): void {
  if (windows.size < SWEEP_AT) return
  const cutoff = nowMs - limits.windowSeconds * 1000
  for (const [key, state] of windows) {
    const live = state.admitted.some((at) => at > cutoff) || (state.notifiedAt ?? -Infinity) > cutoff
    if (!live) windows.delete(key)
  }
}

/** Tests only. */
export function resetDeleteBurstWindows(): void {
  windows.clear()
}

export interface DestructiveCallContext {
  noun: string
  op: string
  args: Record<string, unknown>
  userId: string
  /** The X-Exp-Session-Id, when it is the caller's own (`gates.inSession`). */
  verifiedSessionId: string | null
  authorization: string | null
  access: McpAccess
  now?: number
  env?: Record<string, string | undefined>
}

/** null = go ahead; else the refusal the call answers with. A call that is
 *  not destructive always goes ahead. */
export function guardDestructiveCall(ctx: DestructiveCallContext): string | null {
  const key = destructiveKey(ctx.noun, ctx.op)
  if (!key) return null
  const value = ctx.args[key]
  const idError = singleIdError(value)
  if (idError) return singleIdMessage(idError, ctx.noun, ctx.op, key)

  const limits = burstLimits(ctx.env ?? process.env)
  const nowMs = ctx.now ?? Date.now()
  sweep(nowMs, limits)
  const callerKey = deleteBurstKey(ctx.userId, ctx.verifiedSessionId, ctx.authorization)
  let state = windows.get(callerKey)
  if (!state) windows.set(callerKey, (state = newBurstState()))
  const verdict = burstAdmit(state, nowMs, limits)
  if (verdict === `ok`) return null
  if (verdict === `blocked`) {
    void postBurstBlocked(ctx, `${ctx.op}:${String(value)}`, limits).catch((e) =>
      console.error(`[emcp-deletes] could not post ${DELETE_BURST_EVENT}:`, e)
    )
  }
  return burstMessage(limits)
}

/** The team the event goes to: the session's, else the one team a confined
 *  credential reaches, else the caller's personal team. */
async function burstTeam(ctx: DestructiveCallContext): Promise<string | null> {
  if (ctx.verifiedSessionId) {
    const [row] = await db
      .select({ teamId: sessions.teamId })
      .from(sessions)
      .where(eq(sessions.id, ctx.verifiedSessionId))
      .limit(1)
    if (row?.teamId) return row.teamId
  }
  if (!ctx.access.full && ctx.access.visibleTeamIds.size === 1) return [...ctx.access.visibleTeamIds][0]!
  return getPersonalTeamId(ctx.userId)
}

async function postBurstBlocked(ctx: DestructiveCallContext, target: string, limits: BurstLimits): Promise<void> {
  const teamId = await burstTeam(ctx)
  if (!teamId) return
  await db.transaction((tx) =>
    emitEventInTx(tx, {
      teamId,
      name: DELETE_BURST_EVENT,
      payload: {
        tool: `exponential_${ctx.noun}_write`,
        op: ctx.op,
        target,
        max: limits.max,
        windowSeconds: limits.windowSeconds,
        sessionId: ctx.verifiedSessionId,
        userId: ctx.userId,
      },
      emittedBy: { server: true },
    })
  )
}
