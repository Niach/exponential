import { decodeToolResult, type Decoded, type IssueDetail, type IssueRow, type NotificationRow, type ToolResult } from "./wire"

// Typed callers for the tools the views and the mods read. A host gives its
// own transport: `(tool, args) => Promise<ToolResult>` (a mod's
// `$.mcp.call(server, tool, args)`, a view's `tools/call` through its host).

export type CallTool = (tool: string, args: Record<string, unknown>) => Promise<ToolResult>

async function call<T>(transport: CallTool, tool: string, args: Record<string, unknown>): Promise<Decoded<T>> {
  try {
    return decodeToolResult<T>(await transport(tool, args))
  } catch (error) {
    return { kind: `error`, message: error instanceof Error ? error.message : String(error) }
  }
}

/** A list op's rows. Outside a session a view list op answers `{items, page}`
 *  as its structured result and a summary as its text (#VAPP-292); inside one
 *  it answers the plain array. */
async function rows<T>(transport: CallTool, tool: string, args: Record<string, unknown>): Promise<Decoded<T[]>> {
  const got = await call<T[] | { items?: T[] }>(transport, tool, args)
  if (got.kind === `error`) return got.message.includes(`shown to the person`) ? { kind: `error`, message: `The list could not be read.` } : got
  if (Array.isArray(got.data)) return { kind: `ok`, data: got.data }
  return Array.isArray(got.data?.items) ? { kind: `ok`, data: got.data.items } : { kind: `error`, message: `The list could not be read.` }
}

/** The issue with its pull requests and live sessions; no comments. */
export function getIssue(transport: CallTool, id: string): Promise<Decoded<IssueDetail>> {
  return call(transport, `exponential_issues`, { op: `get`, id, commentsLimit: 0 })
}

export interface IssueListQuery {
  boardId?: string
  teamId?: string
  assigneeId?: string
  search?: string
  limit?: number
}

/** Open issues, the tool's order (plain rows: `op: "list"`, never the view). */
export function listIssues(transport: CallTool, query: IssueListQuery = {}): Promise<Decoded<IssueRow[]>> {
  return rows(transport, `exponential_issues`, { op: `list`, limit: 30, ...query })
}

/** The caller's unread notifications, newest first. */
export function listUnreadNotifications(transport: CallTool, limit = 20): Promise<Decoded<NotificationRow[]>> {
  return rows(transport, `exponential_notifications`, { op: `list`, unreadOnly: true, limit })
}

/** The PR tool's name, whatever prefix the host gives it. */
export function isPullRequestWrite(tool: string): boolean {
  return /(^|__)exponential_pr_write$/.test(tool)
}
