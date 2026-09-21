import type { SessionDisplayState } from "@/lib/coding-session-display"

/** One node's coding session as the graph reads it (`useWorkflowNodeRuns`). */
export interface WorkflowNodeRun {
  sessionId: string
  /** Still up (`running` / `in_review`). */
  live: boolean
  state: SessionDisplayState
  /** The agent is mid-turn: the dot pings. */
  working: boolean
}
