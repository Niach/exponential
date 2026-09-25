// EXP-1082 §4 — the open questions of a workflow's live runs, the source of
// every node chip's `needs you` badge (NEEDS_YOU_LABEL, `lib/workflow-view.ts`)
// and of the page's question list ×4.
//
// The question lives in `coding_sessions.pending_question` (jsonb
// `{ question, askedAt }`; the schema + the `coding-sessions` shape column
// landed in this contract). EXP-1065 writes it from
// `exponential_sessions_ask_parent({ to: 'user' })`, clears it when the
// answer lands, and implements this selector. An open question never
// changes a node's state, only the badge. CONTRACT STUB: returns `[]`.
import type { CodingSession } from "@/db/schema"

export interface WorkflowOpenQuestion {
  nodeId: string
  sessionId: string
  question: string
  askedAt: string
}

export function workflowOpenQuestions(
  _sessions: readonly Pick<
    CodingSession,
    `id` | `workflowId` | `workflowNodeId` | `pendingQuestion` | `status`
  >[],
  _workflowId: string
): WorkflowOpenQuestion[] {
  return []
}
