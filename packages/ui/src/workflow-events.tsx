// EXP-1082 §3 — the workflow event log (`workflow_events`, synced through the
// `workflow-events` shape): one short line per engine decision outcome, the
// newest WORKFLOW_EVENTS_MAX per workflow. CONTRACT STUB: typed and exported,
// renders an empty list; EXP-1064 draws the rows (kind glyph, message, time,
// node/run links) ×4.

export interface WorkflowEventListItem {
  id: string
  /** contract `wfEventKind`. */
  kind: string
  message: string
  at: string | Date
  nodeId?: string | null
  sessionId?: string | null
}

export interface WorkflowEventListProps {
  events: readonly WorkflowEventListItem[]
}

export function WorkflowEventList(_props: WorkflowEventListProps) {
  return <ul data-slot="workflow-event-list" />
}
