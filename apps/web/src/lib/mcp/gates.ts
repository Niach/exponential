// EXP-660: per-request tool gates. The MCP server is rebuilt on every POST
// (stateless transport), so what a client sees in tools/list can depend on
// the caller — and a tool an agent can never use is pure context noise once
// it has searched for it. The first gate is helpdesk: its seven tools only
// register when at least one team the caller could use them in has helpdesk
// switched on. Registration stays context hygiene, NOT the security boundary
// — every helpdesk tool re-checks the specific team's flag on the call, and
// membership lives in the router.
//
// EXP-679: the second gate is sessionsEnd. A close-out only means something
// for an UNATTENDED run (`started_reason` set) — that is the only run the
// call actually ends. A person-started run keeps going and the human is right
// there, so the tool is noise plus an invitation to sign off mid-conversation.
// Registration stays context hygiene here too: `endSessionByAgent` remains
// the authority, so a stale client that still calls the tool gets the same
// kept-open behaviour it always did.
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, teams } from "@/db/schema"
import { getUserTeamIds } from "@/lib/team-membership"
import type { McpAccess } from "./scope"

export interface McpToolGates {
  helpdesk: boolean
  sessionsEnd: boolean
}

/** The worst-case surface — the default `registerExponentialTools` takes, so
 * the tests and the context budget measure EVERY tool. The route passes the
 * resolved value; nothing else should. */
export const ALL_MCP_TOOL_GATES: McpToolGates = {
  helpdesk: true,
  sessionsEnd: true,
}

export async function resolveMcpToolGates(
  userId: string,
  access: McpAccess,
  // EXP-679: the coding_sessions row this request runs inside (null for a
  // human's MCP client, which never gets the close-out tool).
  sessionId: string | null = null
): Promise<McpToolGates> {
  const sessionsEnd = await resolveSessionsEndGate(userId, sessionId)
  const memberTeamIds = await getUserTeamIds(userId)
  // Helpdesk tools need a FULL team grant (threads carry reporter PII), so a
  // board-confined OAuth token must not see them either.
  const teamIds = access.full
    ? memberTeamIds
    : memberTeamIds.filter((id) => access.fullTeamIds.has(id))
  if (teamIds.length === 0) return { helpdesk: false, sessionsEnd }
  const rows = await db
    .select({ id: teams.id })
    .from(teams)
    .where(and(inArray(teams.id, teamIds), eq(teams.helpdeskEnabled, true)))
    .limit(1)
  return { helpdesk: rows.length > 0, sessionsEnd }
}

/** One indexed lookup: the header's run must exist, belong to the caller
 * (owner or host — the same pair `endSessionByAgent` accepts) and have been
 * started unattended. */
async function resolveSessionsEndGate(
  userId: string,
  sessionId: string | null
): Promise<boolean> {
  if (!sessionId) return false
  const [row] = await db
    .select({
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      startedReason: codingSessions.startedReason,
    })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  if (!row) return false
  if (row.userId !== userId && row.hostUserId !== userId) return false
  return row.startedReason !== null
}
