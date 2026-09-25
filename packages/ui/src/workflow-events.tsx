// EXP-1082 §3 / EXP-1064 — the workflow EVENT LOG (`workflow_events`, synced
// through the `workflow-events` shape): one short line per engine decision
// outcome, the newest WORKFLOW_EVENTS_MAX per workflow. Without it every stuck
// workflow was a psql session and a read of the runner's settings.json.
//
// ONE row per event, newest first: the kind's CONCEPT glyph, the node's
// identifier (resolved by the caller — the row only carries `node_id`), the
// host-written sentence, and the local `HH:mm`. A row with a `session_id`
// opens that run through `onOpenSession`. Mirrored ×4 (desktop
// `ui::workflow_events`, iOS `WorkflowEventList`, Android
// `WorkflowEventList.kt`) with the same glyph table.
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"

type IconConcept = Parameters<typeof conceptIcon>[0]
import { ListRow } from "./glass-rows"

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
  /** The node's identifier (`EXP-12`) for a row that names one; absent or
   *  null = no lead-in. */
  nodeLabel?: (nodeId: string) => string | null | undefined
  /** Makes rows with a `sessionId` open that run. */
  onOpenSession?: (sessionId: string) => void
  className?: string
}

/** The glyph each event kind wears — a CONCEPT, never a raw icon (EXP-273).
 *  Byte-identical table ×4. */
export const WORKFLOW_EVENT_GLYPH: Record<string, IconConcept> = {
  node_started: `action-run`,
  retrying: `run-resume`,
  review_started: `nav-reviews`,
  review_verdict: `nav-reviews`,
  review_no_verdict: `nav-reviews`,
  gave_up: `ui-warning`,
  failed: `ui-warning`,
  following_resume: `ui-info`,
  adopting_run: `ui-info`,
  cleared_at_cap: `ui-info`,
  account_picked: `nav-account`,
  account_switched: `nav-account`,
  waiting_reset: `ui-clock`,
  question_asked: `ui-help`,
  question_answered: `ui-help`,
  landed: `ui-check`,
  completed: `ui-check`,
  final_pr_opened: `pr-open`,
  final_pr_reopened: `pr-open`,
  skipped: `ui-close`,
  cancelled: `ui-close`,
}

/** The kinds that mean something went wrong — painted amber. */
const WARNING_KINDS = new Set([`failed`, `gave_up`, `review_no_verdict`, `waiting_reset`])

export function workflowEventGlyph(kind: string): IconConcept {
  return WORKFLOW_EVENT_GLYPH[kind] ?? `ui-info`
}

function stamp(value: string | Date): number {
  const ms = (typeof value === `string` ? new Date(value) : value).getTime()
  return Number.isNaN(ms) ? 0 : ms
}

/** Newest first, ties on id descending — the order every client draws. */
export function sortWorkflowEvents<T extends WorkflowEventListItem>(events: readonly T[]): T[] {
  return [...events].sort(
    (a, b) => stamp(b.at) - stamp(a.at) || (a.id < b.id ? 1 : a.id > b.id ? -1 : 0)
  )
}

/** Local `HH:mm`, empty for an unreadable stamp. */
export function workflowEventTime(value: string | Date): string {
  const ms = stamp(value)
  if (ms === 0) return ``
  const at = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, `0`)
  return `${pad(at.getHours())}:${pad(at.getMinutes())}`
}

export function WorkflowEventList({
  events,
  nodeLabel,
  onOpenSession,
  className,
}: WorkflowEventListProps) {
  const rows = sortWorkflowEvents(events)
  if (rows.length === 0) return <ul data-slot="workflow-event-list" />
  return (
    <ul data-slot="workflow-event-list" className={cn(`flex flex-col`, className)}>
      {rows.map((event) => {
        const Glyph = conceptIcon(workflowEventGlyph(event.kind))
        const label = event.nodeId ? nodeLabel?.(event.nodeId) : null
        const open = event.sessionId && onOpenSession ? event.sessionId : null
        return (
          <li key={event.id} className="list-none">
            <ListRow
              interactive={open !== null}
              density="compact"
              onClick={open ? () => onOpenSession?.(open) : undefined}
              data-kind={event.kind}
              data-testid={`workflow-event-${event.id}`}
            >
              <Glyph
                className={cn(
                  `size-3.5 shrink-0`,
                  WARNING_KINDS.has(event.kind) ? `text-amber-400` : `text-muted-foreground`
                )}
              />
              {label && (
                <span className="shrink-0 font-mono text-xs text-muted-foreground">{label}</span>
              )}
              <span className="min-w-0 flex-1 truncate" title={event.message}>
                {event.message}
              </span>
              <span className="shrink-0 font-mono text-xs tabular-nums text-muted-foreground">
                {workflowEventTime(event.at)}
              </span>
            </ListRow>
          </li>
        )
      })}
    </ul>
  )
}
