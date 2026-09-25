// EXP-1065: the SERVER's own `workflow_events` writer — the audit line the
// engine's `workflows.appendEvent` (trpc/workflows/events.ts, engine-gated)
// cannot write because the server, not the runner device, is the actor:
// a node landing at the review cap (`cleared_at_cap`), a run's question to a
// person (`question_asked`) and its answer (`question_answered`). Same
// bound as the router's: after every insert the workflow keeps only its
// newest WORKFLOW_EVENTS_MAX rows (`at desc, id desc`), so the synced shape
// never grows past it. Best-effort: an audit line never fails the write it
// annotates, so this logs and returns instead of throwing.
import { and, eq, sql } from "drizzle-orm"
import {
  WORKFLOW_EVENTS_MAX,
  WORKFLOW_EVENT_MESSAGE_MAX,
  type WfEventKind,
} from "@exp/db-schema/domain"
import { workflowEvents, workflows } from "@/db/schema"

type Executor = Pick<typeof import("@/db/connection").db, `select` | `insert` | `delete`>

export interface WorkflowEventInput {
  workflowId: string
  /** The workflow's team; looked up when omitted. */
  teamId?: string
  nodeId?: string | null
  sessionId?: string | null
  kind: WfEventKind
  message: string
}

/** One line, cut to the column's cap. */
export function workflowEventMessage(text: string): string {
  return text.replace(/\s+/g, ` `).trim().slice(0, WORKFLOW_EVENT_MESSAGE_MAX)
}

export async function recordWorkflowEvent(
  executor: Executor,
  input: WorkflowEventInput
): Promise<boolean> {
  try {
    let teamId = input.teamId
    if (!teamId) {
      const [row] = await executor
        .select({ teamId: workflows.teamId })
        .from(workflows)
        .where(eq(workflows.id, input.workflowId))
        .limit(1)
      if (!row) return false
      teamId = row.teamId
    }
    await executor.insert(workflowEvents).values({
      workflowId: input.workflowId,
      teamId,
      nodeId: input.nodeId ?? null,
      sessionId: input.sessionId ?? null,
      kind: input.kind,
      message: workflowEventMessage(input.message),
    })
    await executor
      .delete(workflowEvents)
      .where(
        and(
          eq(workflowEvents.workflowId, input.workflowId),
          sql`${workflowEvents.id} NOT IN (
            SELECT ${workflowEvents.id} FROM ${workflowEvents}
            WHERE ${workflowEvents.workflowId} = ${input.workflowId}
            ORDER BY ${workflowEvents.at} DESC, ${workflowEvents.id} DESC
            LIMIT ${WORKFLOW_EVENTS_MAX}
          )`
        )
      )
    return true
  } catch (err) {
    console.error(`[workflows] event ${input.kind} not recorded:`, err)
    return false
  }
}
