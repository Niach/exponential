import { eq } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { db } from "@/db/connection"
import { sessions } from "@/db/schema"
import { isSessionPrincipal } from "@/lib/sessions/host"
import { resolveMcpAccessClampedToTeam, type McpAccess } from "@/lib/mcp/scope"

// #EXP-1383: the session a request SPEAKS FOR (the launcher's
// X-Exp-Session-Id), verified to be the caller's own (its starter, or the
// host of its device: lib/sessions/host.ts `isSessionPrincipal`). Its ROOT's
// `agent_id` is the ACTING AGENT: writes made through it are stamped with
// that agent (agent-settings.json `actor`) and the request is confined to
// the session's team (/api/mcp, `sessions.send`). A foreign or vanished id
// resolves to null: the header is an identifier, never a credential.
// Resolved ONCE per request (a WeakMap on the Request object): every tool
// call and every tRPC procedure it drives reads the same answer.

export interface ActingSession {
  id: string
  teamId: string
  rootId: string
  /** The member whose session it is (the nudge's assigner, the audit). */
  userId: string
  /** The tree's acting agent (`agent_id ?? started_by_agent_id` of the
   *  root): set = an agent-tree session. */
  rootAgentId: string | null
}

const cache = new WeakMap<Request, Map<string, Promise<ActingSession | null>>>()

/** The caller's session `sessionId` with its root's agent, or null. */
export async function loadActingSession(
  userId: string,
  sessionId: string
): Promise<ActingSession | null> {
  const root = alias(sessions, `acting_root`)
  const [row] = await db
    .select({
      id: sessions.id,
      teamId: sessions.teamId,
      rootId: sessions.rootId,
      userId: sessions.userId,
      deviceId: sessions.deviceId,
      rootAgentId: root.agentId,
      rootStartedByAgentId: root.startedByAgentId,
    })
    .from(sessions)
    .leftJoin(root, eq(root.id, sessions.rootId))
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!row || !(await isSessionPrincipal(userId, row))) return null
  return {
    id: row.id,
    teamId: row.teamId,
    rootId: row.rootId,
    userId: row.userId,
    // #EXP-1383: a conversation's agent, or the agent that started the root.
    rootAgentId: row.rootAgentId ?? row.rootStartedByAgentId ?? null,
  }
}

/** `loadActingSession`, memoized on the request (per caller). */
export function actingSessionForRequest(
  request: Request,
  userId: string,
  sessionId: string | null
): Promise<ActingSession | null> {
  if (!sessionId) return Promise.resolve(null)
  let perRequest = cache.get(request)
  if (!perRequest) {
    perRequest = new Map()
    cache.set(request, perRequest)
  }
  const key = `${userId}\0${sessionId}`
  let pending = perRequest.get(key)
  if (!pending) {
    pending = loadActingSession(userId, sessionId).catch((error) => {
      perRequest!.delete(key)
      throw error
    })
    perRequest.set(key, pending)
  }
  return pending
}

/** /api/mcp (defense in depth behind the team agent's scoped key): a
 *  request speaking for an AGENT-tree session of the caller's is confined to
 *  that session's team, whatever credential carried it; any other request
 *  keeps its access. */
export async function confineMcpAccessToActingSession(
  access: McpAccess,
  request: Request,
  userId: string,
  sessionId: string | null
): Promise<McpAccess> {
  const acting = await actingSessionForRequest(request, userId, sessionId)
  return acting?.rootAgentId ? resolveMcpAccessClampedToTeam(access, acting.teamId) : access
}
