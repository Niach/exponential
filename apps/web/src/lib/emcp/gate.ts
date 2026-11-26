// #EXP-1491: eMCP's ONE inbound gate, token → member → grant → access →
// audit, shared by `/api/mcp` and the coming `/api/mcp/apps` (#VAPP-181).
// An endpoint is a TOOL PROVIDER: given the authenticated caller it builds
// the MCP server for this request (stateless transport, rebuilt per POST);
// Exponential's 22 tools are one (`exponentialToolProvider`). The gate owns
// everything around it: credentials, the 401 with the protected-resource
// hint, the acting session's confinement and sign of life, the transport,
// and the `mcp_calls` row of every request (audit.ts; #NEW-147: every method).
import { runInSpace, SPACE_KEY_MESSAGE, type SpaceGrant } from "@/lib/spaces/context"
import { assertSessionOnSpace } from "@/lib/spaces/gate"
import { loadSpaceGrant } from "@/lib/spaces/grant"
import {
  createMcpHandler,
  isLegacyRequest,
  WebStandardStreamableHTTPServerTransport,
  type McpServer,
  type Server,
} from "@modelcontextprotocol/server"
import { clientContextOf } from "@exp/emcp/protocol"
import { and, eq, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { sessions } from "@/db/schema"
import { getPersonalTeamId, getUserTeamIds } from "@/lib/auth/membership"
import { resolveMcpCredential } from "@/lib/auth/resolve-bearer"
import { jsonResponse } from "@/lib/emcp/helpers"
import { auditRows, recordMcpCalls, requestsOf, type AuditOptions } from "@/lib/emcp/audit"
import { resolveMcpToolGates, type McpToolGates } from "@/lib/emcp/gates"
import { MCP_SESSION_HEADER, parseMcpSessionHeader } from "@/lib/emcp/session-header"
import { actingSession } from "@exp/emcp/access"
import { createExponentialMcpServer, type McpUser } from "@/lib/emcp/server"
import { grantOf, NO_CLIENT, type ClientContext } from "@/lib/emcp/request-context"
import { callOf, type McpCall } from "@/lib/emcp/mrtr"
import {
  FULL_ACCESS,
  resolveMcpAccessForGrant,
  resolveMcpTokenAccess,
  type McpAccess,
} from "@/lib/emcp/scope"
import {
  actingSessionForRequest,
  confineMcpAccessToActingSession,
  movedAssistantKeyScope,
  shareActingSessions,
} from "@/lib/sessions/acting"
import type { ApiKeyKind, ApiKeyScope } from "@/lib/auth/api-key-kind"
import { hostedByCondition } from "@/lib/sessions/host"
import { touchSessionActivity } from "@/lib/sessions/watchdog"
import { runAsSession } from "@/lib/apps/automation-scope"
import { requestWithViaAppClaim } from "@/lib/apps/via-app-claim"

/** Who a request acts as, after the gate. */
export interface EmcpCaller {
  user: McpUser
  access: McpAccess
  /** The OAuth client of a token; null for a cookie, a session bearer or an
   * `expu_` key. */
  clientId: string | null
  /** The X-Exp-Session-Id row this request runs inside (unverified: tools
   * check ownership; `gates.inSession` says whether it is the caller's). */
  sessionId: string | null
  gates: McpToolGates
  /** #NEW-144: the credential an MRTR requestState is sealed to
   *  (`oauth:<clientId>`, `key:<keyId>` or `member`). */
  grant: string
  /** #NEW-144: what this request's client declared, read once off its
   *  body (2026-07-28: the per-request `_meta` envelope; set by `serveMcp`). */
  client?: ClientContext
  /** #NEW-144: the one call this request serves (what an MRTR state binds). */
  call?: McpCall
}

/** #VAPP-181: what an endpoint adds to its audit rows: the team, the app of
 * each call and how its answers classify. */
export interface ServeOptions extends AuditOptions {
  teamId?: string | null
}

type Served = McpServer | Server | Response | { server: McpServer | Server; audit: ServeOptions }

/** Builds the MCP server one request talks to (#VAPP-181: async, with its
 * audit options, or the Response that refuses it, e.g. a token without an
 * apps grant). */
export type ToolProvider = (caller: EmcpCaller, request: Request) => Served | Promise<Served>

export const exponentialToolProvider: ToolProvider = async (caller, request) =>
  createExponentialMcpServer(
    caller.user,
    // VAPP-231: an app host's checked claim makes its writes read "via <app>".
    await requestWithViaAppClaim(request, caller.user.id, caller.access),
    caller.access,
    caller.sessionId,
    caller.gates,
    {
      client: caller.client ?? NO_CLIENT,
      principal: { userId: caller.user.id, grant: caller.grant },
      ...(caller.call ? { call: caller.call } : {}),
    }
  )

// Session cookies, bearer session tokens, and UNSCOPED personal `expu_` api
// keys are the user's own credentials → full membership access. A SCOPED key
// (FEED-76) is confined to the teams/boards chosen at mint, exactly like an
// OAuth2 access token (human MCP clients like Claude) is confined to the
// consent grant it resolves through.
async function resolveCredential(
  request: Request
): Promise<{
  userId: string
  clientId: string | null
  access: McpAccess
  keySessionId?: string | null
  /** #EXP-2161: the member space the agent key was minted through. */
  space?: SpaceGrant
  /** A scoped `expu_` key's row id, kind and stored scope (#EXP-2079). */
  key?: { id: string | null; kind: ApiKeyKind | null; scope: ApiKeyScope }
} | null> {
  const { session, keyScope, keyKind, keySessionId, keySpaceId } = await resolveMcpCredential(request)
  if (session?.user) {
    const userId = session.user.id
    // #EXP-2161: a space's own key is no MCP credential; an agent key minted
    // through it lives while the space does and reaches its teams only.
    if (keyKind === `space`) return null
    if (keySpaceId) {
      const space = await loadSpaceGrant(db, userId, keySpaceId)
      if (!space) return null
      const teamIds = keyScope && !keyScope.assistant ? space.reach.filter((id) => keyScope.teamIds.includes(id)) : space.reach
      return {
        userId,
        clientId: null,
        access: await resolveMcpAccessForGrant({ allTeams: false, teamIds, boardIds: [] }, keyScope ? `agent` : `session`),
        keySessionId: keySessionId ?? null,
        space,
      }
    }
    // #EXP-2028: a session-bound launcher key carries the member's access,
    // always clamped below to its session's tree.
    if (!keyScope) return { userId, clientId: null, access: FULL_ACCESS, keySessionId: keySessionId ?? null }
    // #EXP-1720: the Assistant's key reaches the member's teams of the moment
    // (the acting session's clamp narrows it to the turn's pick).
    const grant = keyScope.assistant
      ? { allTeams: false, teamIds: await getUserTeamIds(userId), boardIds: [] }
      : keyScope
    // #EXP-1383: a team agent's key (one team) words its denials as such.
    return {
      userId,
      clientId: null,
      access: await resolveMcpAccessForGrant(grant, keyKind === `agent` ? `agent` : `apiKey`),
      key: { id: session.session?.id ?? null, kind: keyKind ?? null, scope: keyScope },
    }
  }

  const authz = request.headers.get(`authorization`)
  const bearer = authz?.match(/^Bearer\s+(.+)$/i)?.[1]
  if (!bearer) return null

  const token = await resolveMcpTokenAccess(bearer)
  if (!token) return null
  return { userId: token.userId, clientId: token.clientId, access: token.access }
}

/** #VAPP-182: the apps endpoint has protected-resource metadata of its own
 * (`resource` = `/api/mcp/apps`), so a client authorizes for it and the
 * consent page asks for apps (lib/auth/mcp-authorize-guard.ts). */
export const APPS_RESOURCE_METADATA_PATH = `/.well-known/oauth-protected-resource/api/mcp/apps`

function unauthorized(request: Request): Response {
  const baseURL = process.env.BETTER_AUTH_URL?.replace(/\/$/, ``) ?? ``
  const apps = new URL(request.url).pathname.replace(/\/+$/, ``) === `/api/mcp/apps`
  const metadata = apps ? APPS_RESOURCE_METADATA_PATH : `/api/auth/.well-known/oauth-protected-resource`
  const wwwAuthenticate = baseURL ? `Bearer resource_metadata="${baseURL}${metadata}"` : `Bearer`
  return new Response(
    JSON.stringify({
      jsonrpc: `2.0`,
      error: {
        code: -32000,
        message: `Unauthorized: Authentication required`,
        "www-authenticate": wwwAuthenticate,
      },
      id: null,
    }),
    {
      status: 401,
      headers: {
        "content-type": `application/json`,
        "WWW-Authenticate": wwwAuthenticate,
        "Access-Control-Expose-Headers": `WWW-Authenticate`,
      },
    }
  )
}

/** #EXP-2028: the refusal of a session-bound key's request that names
 *  another session, or whose session no longer resolves as the caller's. */
export const BOUND_KEY_MISMATCH_MESSAGE = `This key speaks for one session only: the request named another one.`
export const BOUND_KEY_UNKNOWN_MESSAGE = `This key's session no longer exists or is no longer yours.`

/** The same request, carrying `X-Exp-Session-Id: sessionId` (what every
 *  tool and tRPC procedure behind the gate reads). */
async function withSessionHeader(request: Request, sessionId: string): Promise<Request> {
  const headers = new Headers(request.headers)
  headers.set(MCP_SESSION_HEADER, sessionId)
  const hasBody = request.method !== `GET` && request.method !== `HEAD`
  return new Request(request.url, {
    method: request.method,
    headers,
    body: hasBody ? await request.clone().arrayBuffer() : undefined,
    signal: request.signal,
  })
}

/** The caller, or the Response that refuses the request. `request` = the
 *  one to serve (#EXP-2028: a session-bound key's carries its session's
 *  header). */
export async function authenticateMcpRequest(
  incoming: Request
): Promise<{ caller: EmcpCaller; request: Request; space?: SpaceGrant } | { response: Response }> {
  const resolved = await resolveCredential(incoming)
  if (!resolved) return { response: unauthorized(incoming) }

  const [user] = await db.select().from(users).where(eq(users.id, resolved.userId)).limit(1)
  if (!user) return { response: jsonResponse(401, { error: `User not found for token` }) }

  // EXP-637: parsed once per request; ownership is enforced per tool.
  const headerSessionId = parseMcpSessionHeader(incoming)
  // #EXP-2028 (access.json `actingSession`): a key bound to a session
  // speaks for it with or without the header, so the agent of a plain chat
  // on the direct wiring cannot leave the header off and escape its tree's
  // clamp; a header naming another session, or a bound session that no
  // longer resolves as the caller's, is refused.
  const keySessionId = resolved.keySessionId ?? null
  // #EXP-2161: a space's agent key speaks for a session running on the space.
  const space = resolved.space
  if (space) {
    const named = keySessionId ?? headerSessionId
    const onSpace = named ? await assertSessionOnSpace(db, space, named).then(() => true, () => false) : false
    if (!onSpace) return { response: jsonResponse(403, { error: SPACE_KEY_MESSAGE }) }
  }
  let request = incoming
  if (keySessionId) {
    const bound = await actingSessionForRequest(incoming, user.id, keySessionId)
    const verdict = actingSession({ keySessionId, headerSessionId, resolves: bound !== null })
    if (`refused` in verdict) {
      const message = verdict.refused === `mismatch` ? BOUND_KEY_MISMATCH_MESSAGE : BOUND_KEY_UNKNOWN_MESSAGE
      return { response: jsonResponse(403, { error: message }) }
    }
    if (headerSessionId === null) {
      request = await withSessionHeader(incoming, keySessionId)
      shareActingSessions(incoming, request)
    }
  }
  const sessionId = keySessionId ?? headerSessionId
  // #EXP-1383 (defense in depth behind the team agent's scoped key): a
  // request speaking for an AGENT-tree session of the caller's (its root has
  // `agent_id`) is confined to that session's team, whatever credential
  // carried it. A foreign session id changes nothing (the header grants
  // nothing on its own).
  const inSpace = <T>(run: () => T): T => (space ? runInSpace(space, run) : run())
  const access = await inSpace(() => accessForActingSession(resolved, request, user.id, sessionId))
  // EXP-1374: a tool call by a session of the caller's is its sign of life
  // (the stuck rule); best effort, never in the request's way.
  if (sessionId) {
    void touchSessionActivity(
      and(eq(sessions.id, sessionId), or(eq(sessions.userId, user.id), hostedByCondition(user.id)))!
    ).catch(() => {})
  }
  // EXP-660: which conditional tool families this caller gets to see.
  const gates = await inSpace(() => resolveMcpToolGates(user.id, access, sessionId))
  const grant = grantOf({ clientId: resolved.clientId, keyId: resolved.key?.id ?? null })
  return { caller: { user, access, clientId: resolved.clientId, sessionId, gates, grant }, request, ...(space ? { space } : {}) }
}

/** The access of a request speaking for `sessionId`: the credential's,
 *  confined to the session's tree. #EXP-2079: a moved Assistant conversation
 *  still live on the team key it launched with resolves as the Assistant's
 *  scope first (`movedAssistantKeyScope`). */
export async function accessForActingSession(
  resolved: { access: McpAccess; key?: { id: string | null; kind: ApiKeyKind | null; scope: ApiKeyScope } },
  request: Request,
  userId: string,
  sessionId: string | null
): Promise<McpAccess> {
  const moved = resolved.key ? await movedAssistantKeyScope(resolved.key, request, userId, sessionId) : null
  const base = moved ? await resolveMcpAccessForGrant(moved, `agent`) : resolved.access
  return confineMcpAccessToActingSession(base, request, userId, sessionId)
}

/** The team an audit row names: the one team a confined access reaches,
 * else none (a full-access call may touch any of the member's teams). */
export function auditTeamId(access: McpAccess): string | null {
  if (access.full || access.visibleTeamIds.size !== 1) return null
  return [...access.visibleTeamIds][0]!
}

async function actingAuditTeam(request: Request, caller: EmcpCaller): Promise<string | null> {
  if (!caller.gates.inSession || !caller.sessionId) return null
  const acting = await actingSessionForRequest(request, caller.user.id, caller.sessionId)
  if (!acting?.rootAgentId || !caller.access.visibleTeamIds.has(acting.teamId)) return null
  return acting.teamId === (await getPersonalTeamId(caller.user.id)) ? null : acting.teamId
}

/** Gate a POST, serve it through `provider`, audit its tool calls. */
export async function serveMcp(
  incoming: Request,
  provider: ToolProvider,
  options: ServeOptions = {}
): Promise<Response> {
  const gated = await authenticateMcpRequest(incoming)
  if (`response` in gated) return gated.response
  // #EXP-2161: every read and write of the call narrows to its space.
  return gated.space
    ? runInSpace(gated.space, () => serveGated(gated.caller, gated.request, provider, options))
    : serveGated(gated.caller, gated.request, provider, options)
}

/**
 * #NEW-144: one POST through `server`, in the era it speaks. A 2026-07-28
 * request (the per-request `_meta` envelope) goes to the SDK's modern
 * handler: `server/discover`, the envelope checks, `resultType`, MRTR; a
 * 2025 one (`initialize`, and every later request of a 2025-06-18 or
 * 2025-11-25 client) to the stateless Streamable HTTP transport as before.
 * Both answer JSON, never a stream: a modern `subscriptions/listen` (a
 * long-lived stream on a per-request server) is refused until a phase of
 * #NEW-143 serves it; the 2025 GET stream of `/api/mcp/apps` stays.
 */
export async function handleMcpPost(server: McpServer | Server, request: Request, body?: unknown): Promise<Response> {
  if (await isLegacyRequest(request)) {
    const transport = new WebStandardStreamableHTTPServerTransport({
      sessionIdGenerator: undefined,
      enableJsonResponse: true,
    })
    try {
      await server.connect(transport)
      return await transport.handleRequest(request)
    } finally {
      await transport.close().catch(() => {})
    }
  }
  const message = body === undefined ? await request.clone().json().catch(() => null) : body
  if ((message as { method?: unknown } | null)?.method === `subscriptions/listen`) {
    const id = (message as { id?: unknown }).id ?? null
    return jsonResponse(200, { jsonrpc: `2.0`, id, error: { code: -32601, message: `subscriptions/listen is not served by this endpoint` } })
  }
  const modern = createMcpHandler(() => server, { legacy: `reject`, responseMode: `json` })
  try {
    return await modern.fetch(request)
  } finally {
    await modern.close().catch(() => {})
  }
}

async function serveGated(gatedCaller: EmcpCaller, request: Request, provider: ToolProvider, options: ServeOptions): Promise<Response> {

  const body: unknown = await request
    .clone()
    .json()
    .catch(() => null)
  // #NEW-147: every request is audited (`mcp_calls.method`), not only tools/call.
  const audited = requestsOf(body).length > 0
  // #NEW-144: the client's declared capabilities, read ONCE (ctx.client).
  const call = callOf(body)
  const caller: EmcpCaller = { ...gatedCaller, client: clientContextOf(body), ...(call ? { call } : {}) }

  const served = await provider(caller, request)
  if (served instanceof Response) return served
  // `audit` tells the pair apart (an McpServer has a `server` of its own).
  const { server, audit } = `audit` in served ? served : { server: served, audit: options }
  const started = performance.now()
  // #EXP-1720: an Assistant tree's request reaches several teams; its rows
  // name the team its session works in (a child in its target team), never
  // the personal team (null there: a cross-team read).
  const actingTeam = audited ? await actingAuditTeam(request, caller) : null
  try {
    // #VAPP-236: what this call writes or emits names the caller's own
    // session (the agent loop guard of app automations).
    const response = await runAsSession(caller.gates.inSession ? caller.sessionId : null, () => handleMcpPost(server, request, body))
    if (audited) {
      const latencyMs = performance.now() - started
      void response
        .clone()
        .json()
        .catch(() => null)
        .then((answer) =>
          recordMcpCalls(
            auditRows(
              {
                userId: caller.user.id,
                clientId: caller.clientId,
                teamId: audit.teamId !== undefined ? audit.teamId : (auditTeamId(caller.access) ?? actingTeam),
                appId: null,
                // #EXP-1798: the session only when it is the caller's own.
                sessionId: caller.gates.inSession ? caller.sessionId : null,
              },
              body,
              answer,
              latencyMs,
              audit
            )
          )
        )
    }
    return response
  } finally {
    await server.close().catch(() => {})
  }
}
