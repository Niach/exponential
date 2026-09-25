// EXP-1065 — the open questions of a workflow's live runs, the source of
// every node chip's `needs you` badge (NEEDS_YOU_LABEL, `lib/workflow-view.ts`)
// and of the page's question list ×4 (desktop `domain::workflow_questions`,
// iOS `WorkflowQuestions`, Android `WorkflowQuestions`: the same rule).
//
// The question lives in `coding_sessions.pending_question` (jsonb
// `{ question, askedAt }`), written by `exponential_sessions_ask_parent({ to:
// 'user' })` and cleared when the run's next turn starts (`setNeedsInput`
// false) or the run ends. An open question never changes a node's state,
// only the badge. A planner run's question (no node) is not listed here: it
// reaches the person through the notification and the run's own composer.
import type { CodingSession } from "@/db/schema"

export interface WorkflowOpenQuestion {
  nodeId: string
  sessionId: string
  question: string
  askedAt: string
}

const LIVE = new Set([`running`, `in_review`])

export function workflowOpenQuestions(
  sessions: readonly Pick<
    CodingSession,
    `id` | `workflowId` | `workflowNodeId` | `pendingQuestion` | `status`
  >[],
  workflowId: string
): WorkflowOpenQuestion[] {
  const open: WorkflowOpenQuestion[] = []
  for (const session of sessions) {
    if (session.workflowId !== workflowId || !session.workflowNodeId) continue
    if (!LIVE.has(session.status)) continue
    const pending = session.pendingQuestion
    const question = typeof pending?.question === `string` ? pending.question.trim() : ``
    if (!question) continue
    open.push({
      nodeId: session.workflowNodeId,
      sessionId: session.id,
      question,
      askedAt: typeof pending?.askedAt === `string` ? pending.askedAt : ``,
    })
  }
  return open.sort(
    (a, b) => a.askedAt.localeCompare(b.askedAt) || a.sessionId.localeCompare(b.sessionId)
  )
}
