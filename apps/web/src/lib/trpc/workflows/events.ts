import { z } from "zod"
import { and, eq } from "drizzle-orm"
import {
  WORKFLOW_EVENT_MESSAGE_MAX,
  wfEventKindValues,
  type WfEventKind,
} from "@exp/db-schema/domain"
import { codingSessions, workflowNodes } from "@/db/schema"
import { authedProcedure, generateTxId } from "@/lib/trpc"
import { writeWorkflowEvent } from "@/lib/workflows/record-event"
import { assertEngine, bad, loadWorkflow } from "./shared"

// EXP-1082 §3: the engine's event log — one short line per decision outcome
// (`wfEventKind`), synced through the `workflow-events` shape and rendered by
// `WorkflowEventList` ×4. ENGINE-gated like every report: only the runner
// device's owner appends. Every append trims the workflow to its newest
// WORKFLOW_EVENTS_MAX rows (`at desc, id desc`) in the same transaction, so
// the shape stays bounded without a sweep.

/** The kinds only the SERVER writes (its transactional and audit lines):
 *  no engine sends them, so an append naming one is refused. */
export const SERVER_ONLY_EVENT_KINDS: readonly WfEventKind[] = [
  `review_verdict`,
  `cleared_at_cap`,
  `question_asked`,
  `question_answered`,
  `skipped`,
  `final_pr_reopened`,
  `completed`,
]

export const workflowEventProcedures = {
  /** ENGINE: append one event line, then trim to the newest 50. The node
   *  must be the workflow's and the session must carry that workflow (or no
   *  workflow at all). */
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
      if (SERVER_ONLY_EVENT_KINDS.includes(input.kind)) {
        throw bad(`The server records ${input.kind} events itself`)
      }
      if (input.nodeId) {
        const [node] = await ctx.db
          .select({ id: workflowNodes.id })
          .from(workflowNodes)
          .where(
            and(eq(workflowNodes.id, input.nodeId), eq(workflowNodes.workflowId, input.workflowId))
          )
          .limit(1)
        if (!node) throw bad(`That node is not part of the workflow`)
      }
      if (input.sessionId) {
        const [session] = await ctx.db
          .select({ workflowId: codingSessions.workflowId })
          .from(codingSessions)
          .where(eq(codingSessions.id, input.sessionId))
          .limit(1)
        if (!session || (session.workflowId !== null && session.workflowId !== input.workflowId)) {
          throw bad(`That run is not part of the workflow`)
        }
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await writeWorkflowEvent(tx, { ...input, teamId: workflow.teamId })
        return { txId }
      })
    }),
}
