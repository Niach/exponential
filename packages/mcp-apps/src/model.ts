import { SEMANTIC_ICONS, type IconConcept, type IconName } from "@exp/icons"
import {
  issuePriorityValues,
  issueStatusValues,
  type IssuePriority,
  type IssueStatus,
} from "@exp/db-schema/domain"
import type { ToolResult } from "./bridge"

// EXP-1183 — the pure half of the MCP Apps views: which view a resource is,
// and what the tool results the host forwards decode to. The tools answer
// with `ok()` (apps/web/src/lib/mcp/helpers.ts): one text block of JSON.

export const MCP_APP_VIEWS = [`issues`, `issue`, `run`, `runs`, `inbox`, `devices`] as const
export type McpAppView = (typeof MCP_APP_VIEWS)[number]

/** The tool each view renders the result of (apps/web/src/lib/mcp/apps.ts
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

export type Decoded<T> =
  | { kind: `ok`; data: T }
  | { kind: `error`; message: string }

/** A tool result's JSON payload, or the error text it carried. */
export function decodeToolResult<T>(result: ToolResult): Decoded<T> {
  const text = (result.content ?? [])
    .filter((block) => block.type === `text` && typeof block.text === `string`)
    .map((block) => block.text)
    .join(``)
  if (result.isError) {
    return { kind: `error`, message: text || `The tool call failed.` }
  }
  if (result.structuredContent !== undefined) {
    return { kind: `ok`, data: result.structuredContent as T }
  }
  try {
    return { kind: `ok`, data: JSON.parse(text) as T }
  } catch {
    return { kind: `error`, message: text || `The tool returned no data.` }
  }
}

export interface IssueRow {
  id: string
  identifier: string
  title: string
  description?: string | null
  status: IssueStatus
  priority: IssuePriority
  prUrl?: string | null
  prNumber?: number | null
  prState?: string | null
  dueDate?: string | null
  updatedAt?: string
}

export interface IssueComment {
  id: string
  authorId: string | null
  parentId: string | null
  source: string
  body: string
  createdAt: string
}

export interface IssueRelation {
  type?: string
  direction?: string
  otherIdentifier?: string
}

export interface IssueDetail extends IssueRow {
  url?: string
  recentComments?: IssueComment[]
  relations?: IssueRelation[]
  attachmentCount?: number
}

export interface RunResult {
  topic: string
  text?: string
  files?: string[]
  label?: string
  url?: string
}

export interface RunDetail {
  id: string
  createdAt?: string
  /** The run's page in the app (sessions_get only). */
  url?: string
  issueId?: string | null
  issueIdentifier?: string | null
  issueTitle?: string | null
  actionName?: string | null
  agent?: string | null
  status: string
  branch?: string | null
  prUrl?: string | null
  prNumber?: number | null
  prState?: string | null
  agentBusy?: boolean | null
  agentCaption?: string | null
  needsInput?: boolean | null
  results?: RunResult[]
}

const STATUS_CONCEPT: Record<IssueStatus, IconConcept> = {
  backlog: `status-backlog`,
  in_progress: `status-in-progress`,
  in_review: `status-in-review`,
  done: `status-done`,
  cancelled: `status-cancelled`,
  duplicate: `status-duplicate`,
}

export const STATUS_LABEL: Record<IssueStatus, string> = {
  backlog: `Backlog`,
  in_progress: `In Progress`,
  in_review: `In Review`,
  done: `Done`,
  cancelled: `Cancelled`,
  duplicate: `Duplicate`,
}

export const PRIORITY_LABEL: Record<IssuePriority, string> = {
  urgent: `Urgent`,
  high: `High`,
  medium: `Medium`,
  low: `Low`,
  none: `No priority`,
}

const PRIORITY_CONCEPT: Record<IssuePriority, IconConcept> = {
  urgent: `priority-urgent`,
  high: `priority-high`,
  medium: `priority-medium`,
  low: `priority-low`,
  none: `priority-none`,
}

export function normalizeStatus(value: unknown): IssueStatus {
  return (issueStatusValues as readonly unknown[]).includes(value)
    ? (value as IssueStatus)
    : `backlog`
}

export function normalizePriority(value: unknown): IssuePriority {
  return (issuePriorityValues as readonly unknown[]).includes(value)
    ? (value as IssuePriority)
    : `none`
}

export function statusIcon(status: IssueStatus): IconName {
  return SEMANTIC_ICONS[STATUS_CONCEPT[status]]
}

export function priorityIcon(priority: IssuePriority): IconName {
  return SEMANTIC_ICONS[PRIORITY_CONCEPT[priority]]
}

export interface IssueGroup {
  status: IssueStatus
  issues: IssueRow[]
}

/** Rows grouped by their anchor status in the contract's display order;
 *  empty groups are dropped, row order inside a group is the tool's. The
 *  tool rows carry the ANCHOR, so a team's custom status lists under the
 *  builtin it anchors to. */
export function groupIssuesByStatus(rows: readonly IssueRow[]): IssueGroup[] {
  const groups = new Map<IssueStatus, IssueRow[]>()
  for (const row of rows) {
    const status = normalizeStatus(row.status)
    const group = groups.get(status) ?? []
    group.push({ ...row, status, priority: normalizePriority(row.priority) })
    groups.set(status, group)
  }
  return issueStatusValues
    .filter((status) => groups.has(status))
    .map((status) => ({ status, issues: groups.get(status) ?? [] }))
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

export interface NotificationRow {
  id: string
  issueId: string | null
  type: string
  title: string
  body?: string | null
  readAt: string | null
  createdAt: string
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

/** What a run is about: its issue, its action, or a chat. */
export function runSubject(run: RunDetail): string {
  if (run.issueIdentifier) {
    return `${run.issueIdentifier} ${run.issueTitle ?? ``}`.trim()
  }
  return run.actionName || `Chat`
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
