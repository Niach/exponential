// EXP-637: the AGENT end path. Every other end is a client/user/merge/system
// decision about a run; this one is the run's own close-out — the agent calls
// the `exponential_sessions_end` MCP tool with a one-paragraph summary of
// what it did. (EXP-686 dropped the self-reported outcome: the summary is the
// report.)
//
// EXP-862: the summary is REPORTED, not STORED. It goes to whoever started
// the run — a live parent run gets it as a message (`notifyParentOfChildEnd`)
// — and nowhere else: no client rendered the stored copy any more, and a
// paragraph nobody reads on a row every client syncs is not worth the column.
// The `summary` column itself is dropped in a follow-up; this path simply
// stops writing it.
//
// EXP-673/EXP-679: the close-out ENDS the row, and only an UNATTENDED run
// (`started_reason` set — nobody is watching that tab) ever gets here: the
// tool is REGISTERED per request off exactly that row (lib/mcp/gates.ts), so
// a person-started run — which keeps talking until the human closes the tab,
// kills it, or its PR merges — is never offered it.
//
// It lives outside `lib/trpc/coding-sessions.ts` on purpose: the MCP tool
// tests mock this one module instead of the whole session router.
import { and, eq, inArray } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { codingSessions } from "@/db/schema"
import type { Context } from "@/lib/trpc"

/** Statuses an agent may close out of (`in_review` = its PR is already open). */
const LIVE_STATUSES = [`running`, `in_review`] as const

export interface AgentEndResult {
  sessionId: string
  status: string
  /** The row was already `ended` — the earlier close-out is preserved. */
  alreadyEnded: boolean
}

/**
 * END the run as the agent's own close-out. Owner-OR-HOST like the rest
 * of the session procedures (EXP-432: a shared-device run is requester-owned
 * while the hosting daemon operates it), and idempotent — a second call on an
 * already-ended row changes nothing and reports `alreadyEnded`.
 *
 * EXP-862: `close.summary` is the agent's report to whoever started the run
 * (the caller relays it); this function does not store it.
 */
export async function endSessionByAgent(
  db: Context[`db`],
  sessionId: string,
  callerId: string,
  // EXP-862: kept in the signature (the tool hands its report here, and the
  // caller relays the same object to the parent run) but no longer written.
  _close: { summary: string }
): Promise<AgentEndResult> {
  const [existing] = await db
    .select({
      id: codingSessions.id,
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      status: codingSessions.status,
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
      alreadyEnded: true,
    }
  }

  // Status-conditioned so a close-out racing a kill can never resurrect the
  // row. needsInput is cleared: a run that just declared itself finished is
  // not waiting on a human.
  const [session] = await db
    .update(codingSessions)
    .set({
      status: `ended`,
      endedAt: new Date(),
      endedBy: `agent`,
      needsInput: false,
      // EXP-848/850: an ended run is never busy and says nothing.
      agentBusy: false,
      agentCaption: null,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(codingSessions.id, sessionId),
        inArray(codingSessions.status, [...LIVE_STATUSES])
      )
    )
    .returning({
      id: codingSessions.id,
      status: codingSessions.status,
    })

  // Lost the race against a concurrent end — treat it as already closed
  // rather than reporting a failure the agent would retry.
  if (!session) {
    return {
      sessionId,
      status: `ended`,
      alreadyEnded: true,
    }
  }

  return {
    sessionId: session.id,
    status: session.status,
    alreadyEnded: false,
  }
}
