// EXP-1065: the ANSWER to a run's open question (`coding_sessions.
// pending_question`, written by `exponential_sessions_ask_parent({to:
// 'user'})`). The person answers in the run's own composer, which reaches
// the device over the steer relay, never this server — so the server learns
// of the answer from what the device DOES post: `setAgentBusy(true)` on the
// turn that consumes it (every turn start), and `setNeedsInput(false)` when
// the device's own flag flips. A parent answering through MCP
// `exponential_sessions_message` is the one server-side delivery, hooked
// too. All three call this: the question and the flag come off the row and
// a workflow run's answer lands in its event log (`question_answered`).
import { and, eq, inArray } from "drizzle-orm"
import type { CodingSessionPendingQuestion } from "@exp/db-schema/domain"
import { codingSessions } from "@/db/schema"
import { recordWorkflowEvent } from "@/lib/workflows/record-event"

type Executor = Pick<typeof import("@/db/connection").db, `select` | `update` | `insert` | `delete`>

export interface PendingQuestionRow {
  pendingQuestion: CodingSessionPendingQuestion | null
  workflowId: string | null
  workflowNodeId: string | null
  /** The run's team (= the workflow's); the recorder looks it up otherwise. */
  teamId?: string | null
}

/** The row's question, cleared. Reads the row when the caller has not. Returns
 *  whether a question was open (and is now answered). */
export async function answerPendingQuestion(
  executor: Executor,
  sessionId: string,
  known?: PendingQuestionRow
): Promise<boolean> {
  let row = known
  if (!row) {
    const [loaded] = await executor
      .select({
        pendingQuestion: codingSessions.pendingQuestion,
        workflowId: codingSessions.workflowId,
        workflowNodeId: codingSessions.workflowNodeId,
        teamId: codingSessions.teamId,
      })
      .from(codingSessions)
      .where(eq(codingSessions.id, sessionId))
      .limit(1)
    row = loaded
  }
  if (!row?.pendingQuestion) return false
  const cleared = await executor
    .update(codingSessions)
    .set({ pendingQuestion: null, needsInput: false })
    .where(
      and(
        eq(codingSessions.id, sessionId),
        inArray(codingSessions.status, [`running`, `in_review`])
      )
    )
    .returning({ id: codingSessions.id })
  if (cleared.length === 0) return false
  await recordQuestionAnswered(executor, sessionId, row)
  return true
}

/** The `question_answered` line of a workflow run; nothing outside one. */
export async function recordQuestionAnswered(
  executor: Executor,
  sessionId: string,
  row: PendingQuestionRow
): Promise<void> {
  if (!row.workflowId || !row.pendingQuestion) return
  await recordWorkflowEvent(executor, {
    workflowId: row.workflowId,
    ...(row.teamId && { teamId: row.teamId }),
    nodeId: row.workflowNodeId,
    sessionId,
    kind: `question_answered`,
    message: `Answered: ${row.pendingQuestion.question.slice(0, 200)}`,
  })
}
