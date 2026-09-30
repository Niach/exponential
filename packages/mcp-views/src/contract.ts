// EXP-1153: the contract between the web server's view tools and the MCP App
// views — pure types and names, imported by both sides (`@exp/mcp-views/contract`).
// A view renders exactly the `structuredContent` its tool returns; the text
// half of the same result is what the model reads, so a host without MCP
// Apps loses nothing.

export const VIEW_NAMES = [`board`, `issue`] as const
export type ViewName = (typeof VIEW_NAMES)[number]

export const VIEW_TITLES: Record<ViewName, string> = {
  board: `Board view`,
  issue: `Issue view`,
}

/** `ui://exponential/<view>` — the resource URI a tool's `_meta.ui.resourceUri` names. */
export function viewResourceUri(view: ViewName): string {
  return `ui://exponential/${view}`
}

/** The tool that renders a view; also what the view re-calls on Refresh. */
export const VIEW_TOOLS: Record<ViewName, string> = {
  board: `exponential_board_view`,
  issue: `exponential_issue_view`,
}

export const VIEW_MIME_TYPE = `text/html;profile=mcp-app`

export interface ViewStatus {
  id: string
  name: string
  /** `#rrggbb` */
  color: string
  category: string
  /** An @exp/ui IconName (status-icons.ts); typed loosely so this file stays dependency-free. */
  icon: string
}

export interface ViewUser {
  id: string
  name: string | null
  email: string | null
}

export interface ViewLabel {
  id: string
  name: string
  color: string
}

export interface ViewIssue {
  id: string
  identifier: string
  title: string
  priority: string
  /** Resolved against the team's rows (a custom status or its builtin anchor). */
  statusId: string
  assignee: ViewUser | null
  labels: ViewLabel[]
  pr: { state: string; url: string | null; number: number | null } | null
  dueDate: string | null
  url: string
}

export interface ViewBoard {
  id: string
  name: string
  prefix: string
  color: string
  icon: string | null
  repositoryId: string | null
  url: string
}

export interface BoardViewData {
  board: ViewBoard
  statuses: ViewStatus[]
  /** Open issues only, the same default as `exponential_issues_list`. */
  issues: ViewIssue[]
}

export interface ViewComment {
  id: string
  author: ViewUser | null
  body: string
  createdAt: string
  source: string
}

export interface ViewRelation {
  type: string
  direction: string
  otherIdentifier: string
  otherTitle: string | null
}

export interface IssueViewData {
  issue: ViewIssue & {
    description: string | null
    estimate: number | null
    createdAt: string
    updatedAt: string
  }
  status: ViewStatus
  board: ViewBoard
  /** Newest first, capped by the tool. */
  comments: ViewComment[]
  relations: ViewRelation[]
}
