import { z } from "zod"
import { and, eq, sql } from "drizzle-orm"
import {
  WORKFLOW_EVENTS_MAX,
  WORKFLOW_EVENT_MESSAGE_MAX,
  wfEventKindValues,
} from "@exp/db-schema/domain"
import { authedProcedure, generateTxId } from "@/lib/trpc"
import { workflowEvents } from "@/db/schema"
import { assertEngine, loadWorkflow } from "./shared"

// EXP-1082 §3: the engine's event log — one short line per decision outcome
// (`wfEventKind`), synced through the `workflow-events` shape and rendered by
// `WorkflowEventList` ×4. ENGINE-gated like every report: only the runner
// device's owner appends. Every append trims the workflow to its newest
// WORKFLOW_EVENTS_MAX rows (`at desc, id desc`) in the same transaction, so
// the shape stays bounded without a sweep.
export const workflowEventProcedures = {
  /** ENGINE: append one event line, then trim to the newest 50. */
  appendEvent: authedProcedure
    .input(
      z.object({
        workflowId: z.string().uuid(),
        nodeId: z.string().uuid().optional(),
        sessionId: z.string().uuid().optional(),
        kind: z.enum(wfEventKindValues),
        message: z.string().max(WORKFLOW_EVENT_MESSAGE_MAX),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.workflowId)
      await assertEngine(workflow, ctx.session.user.id)
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.insert(workflowEvents).values({
          workflowId: input.workflowId,
          teamId: workflow.teamId,
          nodeId: input.nodeId ?? null,
          sessionId: input.sessionId ?? null,
          kind: input.kind,
          message: input.message,
        })
        await tx
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
        return { txId }
      })
    }),
}
