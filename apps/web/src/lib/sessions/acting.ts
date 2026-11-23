import { and, eq } from "drizzle-orm"
import { alias } from "drizzle-orm/pg-core"
import { db } from "@/db/connection"
import { apikeys } from "@/db/auth-schema"
import { sessions, teams } from "@/db/schema"
import type { ApiKeyKind, ApiKeyScope } from "@/lib/auth/api-key-kind"
import { getMemberTeamIds, getUserTeamIds } from "@/lib/auth/membership"
import { isSessionPrincipal } from "@/lib/sessions/host"
import { resolveMcpAccessClampedToTeams, type McpAccess } from "@/lib/emcp/scope"
import { treeReach } from "@/lib/sessions/tree-teams"

// #EXP-1383: the session a request SPEAKS FOR (the launcher's
// X-Exp-Session-Id), verified to be the caller's own (its starter, or the
// host of its device: lib/sessions/host.ts `isSessionPrincipal`). Its ROOT's
// `agent_id` is the ACTING AGENT: writes made through it are stamped with
// that agent (agent-settings.json `actor`). #EXP-2027: every such request
// is confined to the session's tree's teams (/api/mcp, `sessions.send`). A foreign or vanished id
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

/** #EXP-2028: `to` (the gate's copy of `from` with the bound session's
 *  header) reads the answers `from` already resolved. */
export function shareActingSessions(from: Request, to: Request): void {
  let perRequest = cache.get(from)
  if (!perRequest) {
    perRequest = new Map()
    cache.set(from, perRequest)
  }
  cache.set(to, perRequest)
}

/** /api/mcp (defense in depth behind the team agent's scoped key): a
 *  request speaking for a session of the caller's is confined to its tree's
 *  teams (#EXP-1720: a team agent's = its team; the Assistant's = its
 *  scope; #EXP-2027: a plain chat's = its team + its Teams pick,
 *  lib/sessions/tree-teams.ts), whatever credential carried it; a request
 *  speaking for no session keeps its access. */
export async function confineMcpAccessToActingSession(
  access: McpAccess,
  request: Request,
  userId: string,
  sessionId: string | null
): Promise<McpAccess> {
  const acting = await actingSessionForRequest(request, userId, sessionId)
  if (!acting) return access
  const reach = await treeReach(acting.rootId)
  return resolveMcpAccessClampedToTeams(access, reach?.teams ?? [], reach?.kind === `agent` ? `agent` : `session`)
}

/** #EXP-2079: THE KEY A MOVED ASSISTANT CONVERSATION STILL RUNS ON. Before
 *  the move (#EXP-1984) an Assistant lived in a team T and its conversation
 *  launched on the device's agent key of T (stored scope `[T]`). The move puts
 *  the conversation into its member's personal team, but a run that stays
 *  live keeps that key until its next relaunch (which mints the
 *  assistant-scoped one): the tools would refuse everything that names the
 *  session's own team (its cards, its children, itself) and every other team
 *  of the member. So exactly that request resolves as the Assistant's scope
 *  (the member's teams of the moment; the tree clamp narrows it as ever):
 *    * an AGENT key scoped to exactly one team, no board, which the member
 *      still belongs to;
 *    * created BEFORE the member's personal team existed (a key minted for a
 *      regular team afterwards is never widened: the exception ends with
 *      each conversation's first relaunch and no fresh team key reaches it);
 *    * speaking for a session the caller STARTED (and is principal of), that
 *      sits in their personal team, in their own Assistant's tree.
 *  Anything else: null, the key's own scope stands. */
export async function movedAssistantKeyScope(
  key: { id: string | null | undefined; kind: ApiKeyKind | null | undefined; scope: ApiKeyScope | null },
  request: Request,
  userId: string,
  sessionId: string | null
): Promise<ApiKeyScope | null> {
  const { scope } = key
  if (!key.id || key.kind !== `agent` || !scope || scope.assistant) return null
  if (scope.teamIds.length !== 1 || scope.boardIds.length !== 0) return null
  const acting = await actingSessionForRequest(request, userId, sessionId)
  if (!acting || acting.userId !== userId) return null
  const [personal] = await db
    .select({ id: teams.id, createdAt: teams.createdAt })
    .from(teams)
    .where(and(eq(teams.ownerId, userId), eq(teams.kind, `personal`)))
    .limit(1)
  if (!personal || acting.teamId !== personal.id) return null
  const [root] = await db
    .select({ teamId: sessions.teamId, userId: sessions.userId })
    .from(sessions)
    .where(eq(sessions.id, acting.rootId))
    .limit(1)
  if (!root || root.teamId !== personal.id || root.userId !== userId) return null
  if ((await treeReach(acting.rootId))?.kind !== `assistant`) return null
  if (!(await getMemberTeamIds(userId)).includes(scope.teamIds[0]!)) return null
  const [row] = await db
    .select({ createdAt: apikeys.createdAt })
    .from(apikeys)
    .where(and(eq(apikeys.id, key.id), eq(apikeys.referenceId, userId)))
    .limit(1)
  if (!row || !(row.createdAt < personal.createdAt)) return null
  return { allTeams: false, teamIds: await getUserTeamIds(userId), boardIds: [], assistant: true }
}
