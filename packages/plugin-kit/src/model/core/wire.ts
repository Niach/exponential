// #NEW-23: the wire rows every adapter reads (the HTML views, the Claude Code
// mods, later pi): what the `exponential` MCP tools answer, decoded once.
// This folder imports nothing outside itself: the generator copies it into
// each adapter that cannot resolve a workspace package (a mod's hooks module).

/** An MCP tool result: `ok()` on the server = one text block of JSON. */
export interface ToolResult {
  content?: ReadonlyArray<{ type: string; text?: string; [field: string]: unknown }>
  structuredContent?: unknown
  isError?: boolean
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

/** The anchor statuses in display order (contract `issueStatus`; pinned by
 *  `model.test.ts`). */
export const ISSUE_STATUSES = [`backlog`, `in_progress`, `in_review`, `done`, `cancelled`, `duplicate`] as const
export type IssueStatus = (typeof ISSUE_STATUSES)[number]

export const ISSUE_PRIORITIES = [`urgent`, `high`, `medium`, `low`, `none`] as const
export type IssuePriority = (typeof ISSUE_PRIORITIES)[number]

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

/** One of `exponential_issues {op: "get"}`'s `pullRequests`: `status` is the
 *  header pill's label (merge-train.json `statusPill`), worded by the server. */
export interface IssuePullRequest {
  pullRequestId: string
  number: number
  url: string
  state: string
  status: string
  checks: string
}

/** One of `exponential_issues {op: "get"}`'s `sessions` (live ones only). */
export interface IssueSession {
  sessionId: string
  rootId: string
  status: string
  busy: boolean
  needsInput: boolean
  branch?: string | null
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
  url?: string | null
  assigneeId?: string | null
  recentComments?: IssueComment[]
  relations?: IssueRelation[]
  attachmentCount?: number
  pullRequests?: IssuePullRequest[]
  sessions?: IssueSession[]
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
