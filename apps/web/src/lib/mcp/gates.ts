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
//
// EXP-700: the third gate is askParent — only a run another run started (and
// whose parent linkage is stamped) can ask its starter a question, so the
// tool registers for nobody else. Same hygiene rule: the handler re-checks.
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, teams } from "@/db/schema"
import { getUserTeamIds } from "@/lib/team-membership"
import type { McpAccess } from "./scope"

export interface McpToolGates {
  helpdesk: boolean
  sessionsEnd: boolean
  /** EXP-700: the caller's run was started BY another run (`started_reason`
   * = 'agent' with a linked parent) — it may ask its starter a question via
   * `exponential_sessions_ask_parent`. */
  askParent: boolean
}

/** The worst-case surface — the default `registerExponentialTools` takes, so
 * the tests and the context budget measure EVERY tool. The route passes the
 * resolved value; nothing else should. */
export const ALL_MCP_TOOL_GATES: McpToolGates = {
  helpdesk: true,
  sessionsEnd: true,
  askParent: true,
}

export async function resolveMcpToolGates(
  userId: string,
  access: McpAccess,
  // EXP-679: the coding_sessions row this request runs inside (null for a
  // human's MCP client, which never gets the close-out tool).
  sessionId: string | null = null
): Promise<McpToolGates> {
  const { sessionsEnd, askParent } = await resolveSessionGates(
    userId,
    sessionId
  )
  const memberTeamIds = await getUserTeamIds(userId)
  // Helpdesk tools need a FULL team grant (threads carry reporter PII), so a
  // board-confined OAuth token must not see them either.
  const teamIds = access.full
    ? memberTeamIds
    : memberTeamIds.filter((id) => access.fullTeamIds.has(id))
  if (teamIds.length === 0) return { helpdesk: false, sessionsEnd, askParent }
  const rows = await db
    .select({ id: teams.id })
    .from(teams)
    .where(and(inArray(teams.id, teamIds), eq(teams.helpdeskEnabled, true)))
    .limit(1)
  return { helpdesk: rows.length > 0, sessionsEnd, askParent }
}

/** One indexed lookup for both session-header gates: the header's run must
 * exist and belong to the caller (owner or host — the same pair
 * `endSessionByAgent` accepts). `sessionsEnd` needs it started unattended;
 * `askParent` (EXP-700) needs it started by another run with the parent
 * linkage stamped. */
async function resolveSessionGates(
  userId: string,
  sessionId: string | null
): Promise<{ sessionsEnd: boolean; askParent: boolean }> {
  const closed = { sessionsEnd: false, askParent: false }
  if (!sessionId) return closed
  const [row] = await db
    .select({
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      startedReason: codingSessions.startedReason,
      parentSessionId: codingSessions.parentSessionId,
    })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  if (!row) return closed
  if (row.userId !== userId && row.hostUserId !== userId) return closed
  return {
    sessionsEnd: row.startedReason !== null,
    askParent: row.startedReason === `agent` && row.parentSessionId !== null,
  }
}
