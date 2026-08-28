// EXP-660: per-request tool gates. The MCP server is rebuilt on every POST
// (stateless transport), so what a client sees in tools/list can depend on
// the caller — and a tool an agent can never use is pure context noise once
// it has searched for it. The first gate is helpdesk: its seven tools only
// register when at least one team the caller could use them in has helpdesk
// switched on. Registration stays context hygiene, NOT the security boundary
// — every helpdesk tool re-checks the specific team's flag on the call, and
// membership lives in the router.
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { teams } from "@/db/schema"
import { getUserTeamIds } from "@/lib/team-membership"
import type { McpAccess } from "./scope"

export interface McpToolGates {
  helpdesk: boolean
}

/** The worst-case surface — the default `registerExponentialTools` takes, so
 * the tests and the context budget measure EVERY tool. The route passes the
 * resolved value; nothing else should. */
export const ALL_MCP_TOOL_GATES: McpToolGates = { helpdesk: true }

export async function resolveMcpToolGates(
  userId: string,
  access: McpAccess
): Promise<McpToolGates> {
  const memberTeamIds = await getUserTeamIds(userId)
  // Helpdesk tools need a FULL team grant (threads carry reporter PII), so a
  // board-confined OAuth token must not see them either.
  const teamIds = access.full
    ? memberTeamIds
    : memberTeamIds.filter((id) => access.fullTeamIds.has(id))
  if (teamIds.length === 0) return { helpdesk: false }
  const rows = await db
    .select({ id: teams.id })
    .from(teams)
    .where(and(inArray(teams.id, teamIds), eq(teams.helpdeskEnabled, true)))
    .limit(1)
  return { helpdesk: rows.length > 0 }
}
