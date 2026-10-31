// #EXP-1491: eMCP's ONE inbound gate, token → member → grant → access →
// audit, shared by `/api/mcp` and the coming `/api/mcp/apps` (#VAPP-181).
// An endpoint is a TOOL PROVIDER: given the authenticated caller it builds
// the MCP server for this request (stateless transport, rebuilt per POST);
// Exponential's 16 tools are one (`exponentialToolProvider`). The gate owns
// everything around it: credentials, the 401 with the protected-resource
// hint, the acting session's confinement and sign of life, the transport,
// and the `mcp_calls` row of every `tools/call` (audit.ts).
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { and, eq, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { sessions } from "@/db/schema"
import { resolveMcpCredential } from "@/lib/auth/resolve-bearer"
import { jsonResponse } from "@/lib/emcp/helpers"
import { auditRows, recordMcpCalls, toolCallsOf } from "@/lib/emcp/audit"
import { resolveMcpToolGates, type McpToolGates } from "@/lib/emcp/gates"
import { parseMcpSessionHeader } from "@/lib/emcp/session-header"
import { createExponentialMcpServer, type McpUser } from "@/lib/emcp/server"
import {
  FULL_ACCESS,
  resolveMcpAccessForGrant,
  resolveMcpTokenAccess,
  type McpAccess,
} from "@/lib/emcp/scope"
import { confineMcpAccessToActingSession } from "@/lib/sessions/acting"
import { hostedByCondition } from "@/lib/sessions/host"
import { touchSessionActivity } from "@/lib/sessions/watchdog"

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
}

/** Builds the MCP server one request talks to. */
export type ToolProvider = (caller: EmcpCaller, request: Request) => McpServer

export const exponentialToolProvider: ToolProvider = (caller, request) =>
  createExponentialMcpServer(caller.user, request, caller.access, caller.sessionId, caller.gates)

// Session cookies, bearer session tokens, and UNSCOPED personal `expu_` api
// keys are the user's own credentials → full membership access. A SCOPED key
// (FEED-76) is confined to the teams/boards chosen at mint, exactly like an
// OAuth2 access token (human MCP clients like Claude) is confined to the
// consent grant it resolves through.
async function resolveCredential(
  request: Request
): Promise<{ userId: string; clientId: string | null; access: McpAccess } | null> {
  const { session, keyScope, keyKind } = await resolveMcpCredential(request)
  if (session?.user) {
    const userId = session.user.id
    if (!keyScope) return { userId, clientId: null, access: FULL_ACCESS }
    // #EXP-1383: a team agent's key (one team) words its denials as such.
    return {
      userId,
      clientId: null,
      access: await resolveMcpAccessForGrant(keyScope, keyKind === `agent` ? `agent` : `apiKey`),
    }
  }

  const authz = request.headers.get(`authorization`)
  const bearer = authz?.match(/^Bearer\s+(.+)$/i)?.[1]
  if (!bearer) return null

  const token = await resolveMcpTokenAccess(bearer)
  if (!token) return null
  return { userId: token.userId, clientId: token.clientId, access: token.access }
}

function unauthorized(): Response {
  const baseURL = process.env.BETTER_AUTH_URL?.replace(/\/$/, ``) ?? ``
  const wwwAuthenticate = baseURL
    ? `Bearer resource_metadata="${baseURL}/api/auth/.well-known/oauth-protected-resource"`
    : `Bearer`
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

/** The caller, or the Response that refuses the request. */
export async function authenticateMcpRequest(
  request: Request
): Promise<{ caller: EmcpCaller } | { response: Response }> {
  const resolved = await resolveCredential(request)
  if (!resolved) return { response: unauthorized() }

  const [user] = await db.select().from(users).where(eq(users.id, resolved.userId)).limit(1)
  if (!user) return { response: jsonResponse(401, { error: `User not found for token` }) }

  // EXP-637: parsed once per request; ownership is enforced per tool.
  const sessionId = parseMcpSessionHeader(request)
  // #EXP-1383 (defense in depth behind the team agent's scoped key): a
  // request speaking for an AGENT-tree session of the caller's (its root has
  // `agent_id`) is confined to that session's team, whatever credential
  // carried it. A foreign session id changes nothing (the header grants
  // nothing on its own).
  const access = await confineMcpAccessToActingSession(resolved.access, request, user.id, sessionId)
  // EXP-1374: a tool call by a session of the caller's is its sign of life
  // (the stuck rule); best effort, never in the request's way.
  if (sessionId) {
    void touchSessionActivity(
      and(eq(sessions.id, sessionId), or(eq(sessions.userId, user.id), hostedByCondition(user.id)))!
    ).catch(() => {})
  }
  // EXP-660: which conditional tool families this caller gets to see.
  const gates = await resolveMcpToolGates(user.id, access, sessionId)
  return { caller: { user, access, clientId: resolved.clientId, sessionId, gates } }
}

/** The team an audit row names: the one team a confined access reaches,
 * else none (a full-access call may touch any of the member's teams). */
export function auditTeamId(access: McpAccess): string | null {
  if (access.full || access.visibleTeamIds.size !== 1) return null
  return [...access.visibleTeamIds][0]!
}

/** Gate a POST, serve it through `provider`, audit its tool calls. */
export async function serveMcp(
  request: Request,
  provider: ToolProvider,
  options: { appId?: string | null } = {}
): Promise<Response> {
  const gated = await authenticateMcpRequest(request)
  if (`response` in gated) return gated.response
  const { caller } = gated

  const body: unknown = await request
    .clone()
    .json()
    .catch(() => null)
  const audited = toolCallsOf(body).length > 0

  const server = provider(caller, request)
  const transport = new WebStandardStreamableHTTPServerTransport({
    sessionIdGenerator: undefined,
    enableJsonResponse: true,
  })
  const started = performance.now()
  try {
    await server.connect(transport)
    const response = await transport.handleRequest(request)
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
                teamId: auditTeamId(caller.access),
                appId: options.appId ?? null,
              },
              body,
              answer,
              latencyMs
            )
          )
        )
    }
    return response
  } finally {
    await transport.close().catch(() => {})
    await server.close().catch(() => {})
  }
}
