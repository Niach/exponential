// EXP-1065: the ANSWER to a run's open question (`coding_sessions.
// pending_question`, written by `exponential_sessions_ask_parent({to:
// 'user'})`). The person answers in the run's own composer, which reaches
// the device over the steer relay, never this server — so the server learns
// of the answer from what the device DOES post: `setAgentBusy(true)` on the
// turn that consumes it (every turn start), and `setNeedsInput(false)` when
// the device's own flag flips. A parent answering through MCP
// `exponential_sessions_message` is the one server-side delivery, hooked
// too. All three clear the question and the flag off the row.
import { and, eq, inArray } from "drizzle-orm"
import { codingSessions } from "@/db/schema"

type Executor = Pick<typeof import("@/db/connection").db, `select` | `update`>

/** The row's question, cleared. Returns whether a question was open (and is
 *  now answered). */
export async function answerPendingQuestion(
  executor: Executor,
  sessionId: string
): Promise<boolean> {
  const [row] = await executor
    .select({ pendingQuestion: codingSessions.pendingQuestion })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
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
  return cleared.length > 0
}
