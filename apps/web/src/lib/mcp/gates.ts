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
// the authority — it ends the run for whoever reaches it, so a stale client
// that still calls the tool on an attended run ends it rather than getting
// some softer legacy behaviour.
//
// EXP-879: the fourth gate is sessionResults. Publishing a screenshot is an
// act ON this run, so it needs the same owner-or-host header check — but
// nothing beyond it: an attended run's pictures are exactly as useful as an
// automation's. A caller with no run of its own (a human's MCP client) has
// nothing to publish on, so the tool never registers for it.
//
// EXP-700 / EXP-1089: the third gate is askParent. It used to open only for
// a run another run started; since EXP-1089 EVERY run of the caller's gets
// the tool (tools are lazy-loaded, an unused one costs nothing): `to: 'user'`
// asks the person who owns the run (a workflow's creator inside a workflow)
// from any run, and the planner run of a workflow clears its questions with
// the person before the graph exists. `parent`/`root` still need a starter,
// which the handler checks (the parent stamps `parent_session_id` only after
// its sessions_start poll returns, so linkage is never part of the gate).
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { codingSessions, teams } from "@/db/schema"
import { getUserTeamIds } from "@/lib/team-membership"
import type { McpAccess } from "./scope"

export interface McpToolGates {
  helpdesk: boolean
  sessionsEnd: boolean
  /** EXP-700 / EXP-1089: the caller runs INSIDE a coding session of its own
   * (owner or host, any `started_reason`) — it may ask a question via
   * `exponential_sessions_ask_parent`: its starter, or the person (`to:
   * 'user'`). Who may be asked is the handler's check, not the gate's. */
  askParent: boolean
  /** EXP-879: the caller runs INSIDE a coding session of its own (owner or
   * host, any `started_reason`) — it may publish screenshots of its work with
   * `exponential_sessions_results`. A human's MCP client has no run to publish
   * on, so the tool stays out of its surface entirely. */
  sessionResults: boolean
}

/** The worst-case surface — the default `registerExponentialTools` takes, so
 * the tests and the context budget measure EVERY tool. The route passes the
 * resolved value; nothing else should. */
export const ALL_MCP_TOOL_GATES: McpToolGates = {
  helpdesk: true,
  sessionsEnd: true,
  askParent: true,
  sessionResults: true,
}

export async function resolveMcpToolGates(
  userId: string,
  access: McpAccess,
  // EXP-679: the coding_sessions row this request runs inside (null for a
  // human's MCP client, which never gets the close-out tool).
  sessionId: string | null = null
): Promise<McpToolGates> {
  const { sessionsEnd, askParent, sessionResults } = await resolveSessionGates(
    userId,
    sessionId
  )
  const memberTeamIds = await getUserTeamIds(userId)
  // Helpdesk tools need a FULL team grant (threads carry reporter PII), so a
  // board-confined OAuth token must not see them either.
  const teamIds = access.full
    ? memberTeamIds
    : memberTeamIds.filter((id) => access.fullTeamIds.has(id))
  if (teamIds.length === 0) {
    return { helpdesk: false, sessionsEnd, askParent, sessionResults }
  }
  const rows = await db
    .select({ id: teams.id })
    .from(teams)
    .where(and(inArray(teams.id, teamIds), eq(teams.helpdeskEnabled, true)))
    .limit(1)
  return { helpdesk: rows.length > 0, sessionsEnd, askParent, sessionResults }
}

/** One indexed lookup for all three session-header gates: the header's run
 * must exist and belong to the caller (owner or host — the same pair
 * `endSessionByAgent` accepts). `sessionsEnd` needs it started unattended;
 * `askParent` (EXP-1089) and `sessionResults` (EXP-879) need nothing more —
 * any run of the caller's may ask a question or publish screenshots of its
 * own work, attended or not. */
async function resolveSessionGates(
  userId: string,
  sessionId: string | null
): Promise<{
  sessionsEnd: boolean
  askParent: boolean
  sessionResults: boolean
}> {
  const closed = {
    sessionsEnd: false,
    askParent: false,
    sessionResults: false,
  }
  if (!sessionId) return closed
  const [row] = await db
    .select({
      userId: codingSessions.userId,
      hostUserId: codingSessions.hostUserId,
      startedReason: codingSessions.startedReason,
    })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  if (!row) return closed
  if (row.userId !== userId && row.hostUserId !== userId) return closed
  return {
    sessionsEnd: row.startedReason !== null,
    askParent: true,
    sessionResults: true,
  }
}
