// EXP-1082 §3: THE `workflow_events` writer. One short line per decision
// outcome (`wfEventKind`), synced through the `workflow-events` shape and
// rendered by `WorkflowEventList` ×4. Three callers, one write:
//
// - the runner device's `workflows.appendEvent` (trpc/workflows/events.ts,
//   engine-gated) for the engine's own decisions;
// - the server's transactional lines beside a workflow write
//   (`completed`, `final_pr_reopened`, `cancelled`, the final-PR `failed`)
//   via `recordWorkflowEventInTx` (lib/workflow-final-pr.ts);
// - the server's best-effort audit lines where the actor is neither
//   (`cleared_at_cap`, `question_asked`, `question_answered`) via
//   `recordWorkflowEvent` below.
//
// Every write inserts, then trims the workflow to its newest
// WORKFLOW_EVENTS_MAX rows (`at desc, id desc`) in the SAME executor, so the
// synced shape stays bounded without a sweep; the message is collapsed to
// one line and cut at the column's cap, never refused.
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
  /** The workflow's team; `recordWorkflowEvent` looks it up when omitted. */
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

/** The write itself: insert + trim, inside whatever executor the caller
 *  holds (a transaction or the pool). Throws like any other write. */
export async function writeWorkflowEvent(
  executor: Pick<Executor, `insert` | `delete`>,
  input: WorkflowEventInput & { teamId: string }
): Promise<void> {
  await executor.insert(workflowEvents).values({
    workflowId: input.workflowId,
    teamId: input.teamId,
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
}

/** Best-effort: an audit line never fails the write it annotates, so this
 *  logs and returns false instead of throwing. */
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
    await writeWorkflowEvent(executor, { ...input, teamId })
    return true
  } catch (err) {
    console.error(`[workflows] event ${input.kind} not recorded:`, err)
    return false
  }
}
