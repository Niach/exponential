// FEED-87: how an issue event's actor acted, stamped on `issue_events` and
// read back by exponential_issues_activity. Its own module so the many
// test mocks of lib/integrations/activity.ts never have to know about it.
import { parseMcpSessionHeader } from "@/lib/mcp/session-header"
import type { Context } from "@/lib/trpc"

/** FEED-87: how an event's actor acted. `viaMcp` = an agent's MCP call,
 *  `sessionId` = the coding run behind it (null outside one). */
export type IssueEventOrigin = { viaMcp: boolean; sessionId: string | null }

/** The origin a tRPC context implies. Only MCP calls read the session header:
 *  a person's own app request never speaks for a run. */
export function eventOrigin(
  ctx: Pick<Context, `viaMcp` | `request`>
): IssueEventOrigin {
  if (!ctx.viaMcp) return { viaMcp: false, sessionId: null }
  return { viaMcp: true, sessionId: parseMcpSessionHeader(ctx.request) }
}
