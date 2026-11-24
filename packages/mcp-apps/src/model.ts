import { SEMANTIC_ICONS, type IconConcept, type IconName } from "@exp/icons"
import type { IssuePriority, IssueStatus, RunDetail } from "@exp/plugin-kit/model"

// #NEW-23: the wire rows, labels and grouping live in `@exp/plugin-kit/model`
// (one source for the HTML views and the harness adapters); re-exported so
// the views keep one import.
export {
  PRIORITY_LABEL,
  STATUS_LABEL,
  decodeToolResult,
  groupIssuesByStatus,
  normalizePriority,
  normalizeStatus,
  type Decoded,
  type IssueComment,
  type IssueDetail,
  type IssueGroup,
  type IssueRelation,
  type IssueRow,
  type NotificationRow,
  type RunDetail,
  type RunResult,
} from "@exp/plugin-kit/model"

// EXP-1183 — the pure half of the MCP Apps views: which view a resource is,
// and what the tool results the host forwards decode to. The tools answer
// with `ok()` (apps/web/src/lib/emcp/helpers.ts): one text block of JSON.

export const MCP_APP_VIEWS = [`issues`, `issue`, `run`, `runs`, `inbox`, `devices`] as const
export type McpAppView = (typeof MCP_APP_VIEWS)[number]

/** The tool each view renders the result of (apps/web/src/lib/emcp/apps.ts
 *  binds the same pairs on the server). */
export const MCP_APP_VIEW_TOOL: Record<McpAppView, string> = {
  issues: `exponential_issues_list`,
  issue: `exponential_issues_get`,
  run: `exponential_sessions_get`,
  runs: `exponential_sessions_list`,
  inbox: `exponential_notifications_list`,
  devices: `exponential_devices_list`,
}

export function parseView(value: string | null | undefined): McpAppView {
  return (MCP_APP_VIEWS as readonly string[]).includes(value ?? ``)
    ? (value as McpAppView)
    : `issues`
}

const STATUS_CONCEPT: Record<IssueStatus, IconConcept> = {
  backlog: `status-backlog`,
  in_progress: `status-in-progress`,
  in_review: `status-in-review`,
  done: `status-done`,
  cancelled: `status-cancelled`,
  duplicate: `status-duplicate`,
}

const PRIORITY_CONCEPT: Record<IssuePriority, IconConcept> = {
  urgent: `priority-urgent`,
  high: `priority-high`,
  medium: `priority-medium`,
  low: `priority-low`,
  none: `priority-none`,
}

export function statusIcon(status: IssueStatus): IconName {
  return SEMANTIC_ICONS[STATUS_CONCEPT[status]]
}

export function priorityIcon(priority: IssuePriority): IconName {
  return SEMANTIC_ICONS[PRIORITY_CONCEPT[priority]]
}

/** The PR concept for a row's PR state (null = no PR). */
export function prConcept(state: string | null | undefined): IconConcept | null {
  switch (state) {
    case `open`:
      return `pr-open`
    case `merged`:
      return `pr-merged`
    case `closed`:
      return `pr-closed`
    case `draft`:
      return `pr-draft`
    default:
      return null
  }
}

/** A run's one-line state, the words the clients' run rows use. */
export function runStateLabel(run: RunDetail): string {
  if (run.status === `ended`) return `Ended`
  if (run.needsInput) return `Needs input`
  if (run.status === `in_review`) return `In review`
  return run.agentBusy ? (run.agentCaption?.trim() || `Working`) : `Idle`
}

export type RunTone = `live` | `attention` | `done` | `idle` | `muted`

/** The run rows' dot: the clients' tones (LiveDot). */
export function runTone(run: RunDetail): RunTone {
  if (run.status === `ended`) return `muted`
  if (run.needsInput) return `attention`
  if (run.status === `in_review`) return `done`
  return run.agentBusy ? `live` : `idle`
}

export function runIsWorking(run: RunDetail): boolean {
  return run.status !== `ended` && Boolean(run.agentBusy) && !run.needsInput
}

/** What a run is about: its issue, its title, or a chat. */
export function runSubject(run: RunDetail): string {
  if (run.issueIdentifier) {
    return `${run.issueIdentifier} ${run.issueTitle ?? ``}`.trim()
  }
  return run.title || `Chat`
}

const RUN_GROUPS = [
  { status: `running`, label: `Running` },
  { status: `in_review`, label: `In review` },
  { status: `ended`, label: `Ended` },
] as const

export interface RunGroup {
  label: string
  runs: RunDetail[]
}

/** Live runs first, then open PRs, then history; the tool's order inside. */
export function groupRuns(runs: readonly RunDetail[]): RunGroup[] {
  return RUN_GROUPS.map((group) => ({
    label: group.label,
    runs: runs.filter((run) =>
      group.status === `ended`
        ? run.status !== `running` && run.status !== `in_review`
        : run.status === group.status
    ),
  })).filter((group) => group.runs.length > 0)
}

/** One `exponential_devices_list` row — only what the views read. */
export interface DeviceRow {
  deviceId: string
  label?: string | null
  kind?: string | null
  online?: boolean
  lastSeenAt?: string | null
  agents?: string[] | null
  [key: string]: unknown
}
