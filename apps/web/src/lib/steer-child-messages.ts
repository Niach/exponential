// EXP-700: parent↔child session messages. When a coding session starts
// another via `exponential_sessions_start`, the child reports back — its
// question (`exponential_sessions_ask_parent`) or its close-out summary —
// as text injected into the parent's live steer channel, exactly the rail a
// human uses to steer a run. The receiving agent must never mistake that for
// its human: every injected message carries a bracketed source prefix, and
// this module is the ONE home of that convention (formatters), the child+
// parent lookup, and the end-of-child notification both end paths call.
//
// It lives outside `lib/trpc/coding-sessions.ts` for the same reason as
// coding-session-end.ts: the MCP tool tests mock this one module.
import { eq } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { codingSessions, issues } from "@/db/schema"
import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import type { Context } from "@/lib/trpc"

export const CHILD_RUN_TAG = `Exponential child run`

/** Statuses a parent can still receive messages in (same pair the agent end
 * path accepts — `in_review` = its PR is open but the run is live). */
export const PARENT_LIVE_STATUSES = [`running`, `in_review`] as const

/** Injected text must land as ONE message: the submit convention is a
 * separate `\r` frame, so a newline inside the text would submit early and
 * fragment a summary into several user messages. */
function oneLine(text: string): string {
  return text.replace(/\s*[\r\n]+\s*/g, ` `).trim()
}

export interface ChildRunRef {
  id: string
  issueIdentifier: string | null
  actionName: string | null
}

/** `EXP-12 3f2a9c1b`, `Nightly build 3f2a9c1b`, or the bare short id. */
export function childRunLabel(child: ChildRunRef): string {
  const short = child.id.slice(0, 8)
  const subject = child.issueIdentifier ?? child.actionName
  return subject ? `${subject} ${short}` : short
}

export function formatChildFinished(
  child: ChildRunRef,
  summary: string
): string {
  return `[${CHILD_RUN_TAG} ${childRunLabel(child)} finished] ${oneLine(summary)}`
}

export function formatChildEndedSilently(
  child: ChildRunRef,
  endedBy: string
): string {
  return `[${CHILD_RUN_TAG} ${childRunLabel(child)} ended without a report (${endedBy})]`
}

/** Names the child's FULL uuid — that is what the parent passes back to
 * `exponential_sessions_message`. */
export function formatChildQuestion(
  child: ChildRunRef,
  question: string
): string {
  return `[${CHILD_RUN_TAG} ${childRunLabel(child)} asks — reply with exponential_sessions_message sessionId=${child.id}] ${oneLine(question)}`
}

/** A header-less caller (a plain `expu_`-key orchestrator) messaging a run
 * it started or owns. */
export function formatStarterMessage(text: string): string {
  return `[Message from your starter via exponential_sessions_message] ${oneLine(text)}`
}

/** The parent answering its own child's ask — distinct prefix so the child
 * can match the reply to its question. */
export function formatParentAnswer(
  parentSessionId: string,
  text: string
): string {
  return `[Answer from your parent run ${parentSessionId.slice(0, 8)} via exponential_sessions_message] ${oneLine(text)}`
}

// The parent row reached through the child's parent_session_id self-FK.
const parentSessions = alias(codingSessions, `parent_sessions`)

export interface ChildParentContext {
  id: string
  userId: string
  hostUserId: string | null
  startedReason: string | null
  parentSessionId: string | null
  actionName: string | null
  issueIdentifier: string | null
  /** null = no parent row (never linked, or since deleted). */
  parentStatus: string | null
}

/** ONE select: the child, its issue identifier and its parent's status via
 * a self-join — single-query on purpose, like every MCP tool lookup. */
export async function loadChildParentContext(
  db: Context[`db`],
  childSessionId: string
): Promise<ChildParentContext | null> {
  const [row] = await db
    .select({
      id: codingSessions.id,
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      startedReason: codingSessions.startedReason,
      parentSessionId: codingSessions.parentSessionId,
      actionName: codingSessions.actionName,
      issueIdentifier: issues.identifier,
      parentStatus: parentSessions.status,
    })
    .from(codingSessions)
    .leftJoin(issues, eq(issues.id, codingSessions.issueId))
    .leftJoin(parentSessions, eq(parentSessions.id, codingSessions.parentSessionId))
    .where(eq(codingSessions.id, childSessionId))
    .limit(1)
  return row ?? null
}

/**
 * Tell a live parent that its agent-started child ended. `summary` null =
 * the child ended WITHOUT reporting (the client end path — a closed tab or
 * dead daemon); `endedBy` then names the ender. No-op unless the child is
 * agent-started with a linked, live parent and the relay is configured.
 * Best-effort and never throws — an end must never fail on its notification.
 */
export async function notifyParentOfChildEnd(
  db: Context[`db`],
  childSessionId: string,
  end: { summary: string | null; endedBy: string }
): Promise<{ delivered: boolean }> {
  try {
    const child = await loadChildParentContext(db, childSessionId)
    if (!child || child.startedReason !== `agent` || !child.parentSessionId) {
      return { delivered: false }
    }
    if (
      !child.parentStatus ||
      !(PARENT_LIVE_STATUSES as readonly string[]).includes(child.parentStatus)
    ) {
      return { delivered: false }
    }
    const config = getSteerRelayConfig()
    if (!config) return { delivered: false }
    const message =
      end.summary !== null
        ? formatChildFinished(child, end.summary)
        : formatChildEndedSilently(child, end.endedBy)
    return await relayPostInput(config, child.parentSessionId, message)
  } catch {
    return { delivered: false }
  }
}
