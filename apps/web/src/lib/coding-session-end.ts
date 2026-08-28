// EXP-637: the AGENT end path. Every other end is a client/user/merge/system
// decision about a run; this one is the run's own close-out — the agent calls
// the `exponential_sessions_end` MCP tool with a one-paragraph summary and an
// outcome, and the row carries both to every runs list on every client.
//
// EXP-673: the close-out ENDS the row only for a run an automation started
// (`started_reason` set — nobody is watching that tab). A run a person
// started (issue, batch, action or chat) records the summary and outcome and
// STAYS live, so the person can keep talking to the agent; it ends when they
// close the tab, kill it, or its PR merges — and the summary rides along.
//
// It lives outside `lib/trpc/coding-sessions.ts` on purpose: the MCP tool
// tests mock this one module instead of the whole session router.
import { and, eq, inArray } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import type { CodingSessionOutcome } from "@exp/db-schema/domain"
import { codingSessions } from "@/db/schema"
import type { Context } from "@/lib/trpc"

/** Statuses an agent may close out of (`in_review` = its PR is already open). */
const LIVE_STATUSES = [`running`, `in_review`] as const

export interface AgentEndResult {
  sessionId: string
  status: string
  outcome: string | null
  /** The row was already `ended` — the earlier close-out is preserved. */
  alreadyEnded: boolean
  /** EXP-673: a person started this run, so the close-out was recorded but
   * the session stays live for their follow-ups. */
  keptOpen: boolean
}

/**
 * Record `sessionId`'s close-out as the agent's own report, and END the run
 * only when an automation started it (EXP-673). Owner-OR-HOST like the rest
 * of the session procedures (EXP-432: a shared-device run is requester-owned
 * while the hosting daemon operates it), and idempotent — an already-ended
 * row keeps whatever summary it has, so a retried tool call never blanks a
 * good one.
 */
export async function endSessionByAgent(
  db: Context[`db`],
  sessionId: string,
  callerId: string,
  close: { summary: string; outcome: CodingSessionOutcome }
): Promise<AgentEndResult> {
  const [existing] = await db
    .select({
      id: codingSessions.id,
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      status: codingSessions.status,
      outcome: codingSessions.outcome,
      startedReason: codingSessions.startedReason,
    })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)

  if (!existing) {
    throw new TRPCError({
      code: `NOT_FOUND`,
      message: `Coding session not found`,
    })
  }
  if (existing.userId !== callerId && existing.hostUserId !== callerId) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the session owner can end it`,
    })
  }

  if (existing.status === `ended`) {
    return {
      sessionId,
      status: `ended`,
      outcome: existing.outcome,
      alreadyEnded: true,
      keptOpen: false,
    }
  }

  // EXP-673: only an automation-started run is really over once the agent
  // says so. A person-started run keeps its status — they may answer.
  const automated = existing.startedReason !== null

  // Status-conditioned so a close-out racing a kill can never resurrect the
  // row (nor stamp a summary onto one that was just killed). needsInput is
  // cleared on the ending path: a run that just declared itself finished is
  // not waiting on a human. The kept-open path leaves it alone — the idle
  // nudge that follows the agent's last turn is exactly "your turn now".
  const [session] = await db
    .update(codingSessions)
    .set(
      automated
        ? {
            status: `ended`,
            endedAt: new Date(),
            endedBy: `agent`,
            summary: close.summary,
            outcome: close.outcome,
            needsInput: false,
            updatedAt: new Date(),
          }
        : {
            summary: close.summary,
            outcome: close.outcome,
            updatedAt: new Date(),
          }
    )
    .where(
      and(
        eq(codingSessions.id, sessionId),
        inArray(codingSessions.status, [...LIVE_STATUSES])
      )
    )
    .returning({
      id: codingSessions.id,
      status: codingSessions.status,
      outcome: codingSessions.outcome,
    })

  // Lost the race against a concurrent end — treat it as already closed
  // rather than reporting a failure the agent would retry.
  if (!session) {
    return {
      sessionId,
      status: `ended`,
      outcome: existing.outcome,
      alreadyEnded: true,
      keptOpen: false,
    }
  }

  return {
    sessionId: session.id,
    status: session.status,
    outcome: session.outcome,
    alreadyEnded: false,
    keptOpen: !automated,
  }
}
