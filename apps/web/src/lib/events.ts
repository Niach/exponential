import { eq, lt } from "drizzle-orm"
import type { EventEmitter } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { events, issues, type Event } from "@/db/schema"
import { fireEventTriggersInTx } from "@/lib/agents/triggers"
import type { Tx } from "@/lib/sessions/tx"

// NEW-2: EVENTS, the team's event stream (synced `events` shape). A row =
// a name + a payload + who emitted it. The server builtins (`issue.created`,
// `issue.status_changed`, `pr.opened`, `pr.merged`, `session.ended`) are
// emitted by the writers that make those moments; anything else comes over
// MCP `events_emit`. Every emit fans out to the agents listening on the
// name (an event trigger) inside the same transaction, as pending input in
// their owners' conversation sessions.

export interface EmitEventArgs {
  teamId: string
  name: string
  payload?: Record<string, unknown> | null
  /** The emitter: a session (its id), a person, or the server. */
  emittedBy: { sessionId: string } | { userId: string } | { server: true }
}

export async function emitEventInTx(tx: Tx, args: EmitEventArgs): Promise<Event> {
  const by: EventEmitter =
    `sessionId` in args.emittedBy ? `session` : `userId` in args.emittedBy ? `user` : `server`
  const [row] = await tx
    .insert(events)
    .values({
      teamId: args.teamId,
      name: args.name,
      payload: args.payload ?? {},
      emittedBy: by,
      sessionId: `sessionId` in args.emittedBy ? args.emittedBy.sessionId : null,
      userId: `userId` in args.emittedBy ? args.emittedBy.userId : null,
    })
    .returning()
  await fireEventTriggersInTx(tx, row!)
  return row!
}

/** Rows older than `events.retentionDays` go (the watchdog's sweep). */
export async function pruneEvents(now: Date = new Date()): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.events.retentionDays * 24 * 60 * 60 * 1000)
  const gone = await db.delete(events).where(lt(events.createdAt, cutoff)).returning({ id: events.id })
  return gone.length
}

/** The builtin `issue.created` / `issue.status_changed` events: the issue's
 *  identity rides the payload (plus the writer's extras). A vanished issue
 *  emits nothing. */
export async function emitIssueEventInTx(
  tx: Tx,
  args: {
    issueId: string
    teamId: string
    name: `issue.created` | `issue.status_changed`
    payload: Record<string, unknown>
    emittedBy: EmitEventArgs[`emittedBy`]
  }
): Promise<Event | null> {
  const [issue] = await tx
    .select({
      identifier: issues.identifier,
      title: issues.title,
      boardId: issues.boardId,
      status: issues.status,
      statusId: issues.statusId,
    })
    .from(issues)
    .where(eq(issues.id, args.issueId))
    .limit(1)
  if (!issue) return null
  return emitEventInTx(tx, {
    teamId: args.teamId,
    name: args.name,
    payload: { issueId: args.issueId, ...issue, ...args.payload },
    emittedBy: args.emittedBy,
  })
}
