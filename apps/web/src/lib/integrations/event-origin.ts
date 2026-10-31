// FEED-87: how an issue event's actor acted, stamped on `issue_events` and
// read back by exponential_issues_activity. Its own module so the many
// test mocks of lib/integrations/activity.ts never have to know about it.
import { parseMcpSessionHeader } from "@/lib/emcp/session-header"
import { actingSessionForRequest } from "@/lib/sessions/acting"
import type { Context } from "@/lib/trpc"

/** FEED-87: how an event's actor acted. `viaMcp` = an agent's MCP call,
 *  `sessionId` = the coding run behind it (null outside one). #EXP-1383:
 *  `agentId` = the ACTING agent (the root agent of the caller's own
 *  session); `null` = none, `undefined` = not resolved here (the writer
 *  derives it from `sessionId`, which internal callers pass verified). */
export type IssueEventOrigin = {
  viaMcp: boolean
  sessionId: string | null
  agentId?: string | null
}

/** The origin a tRPC context implies. Only MCP calls read the session header:
 *  a person's own app request never speaks for a run. The acting agent is
 *  resolved once per request and only from a session that is the caller's. */
export async function eventOrigin(
  ctx: Pick<Context, `viaMcp` | `request` | `session`>
): Promise<IssueEventOrigin> {
  if (!ctx.viaMcp) return { viaMcp: false, sessionId: null, agentId: null }
  const sessionId = parseMcpSessionHeader(ctx.request)
  const userId = ctx.session?.user?.id
  const acting =
    sessionId && userId ? await actingSessionForRequest(ctx.request, userId, sessionId) : null
  return { viaMcp: true, sessionId, agentId: acting?.rootAgentId ?? null }
}

/** #EXP-1383: the agent a tRPC call acts as (null = the person). */
export async function actingAgentId(
  ctx: Pick<Context, `viaMcp` | `request` | `session`>
): Promise<string | null> {
  return (await eventOrigin(ctx)).agentId ?? null
}
