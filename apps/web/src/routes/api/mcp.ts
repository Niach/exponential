import { createFileRoute } from "@tanstack/react-router"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { resolveMcpCredential } from "@/lib/auth/resolve-bearer"
import { jsonResponse } from "@/lib/mcp/helpers"
import { createExponentialMcpServer } from "@/lib/mcp/server"
import { resolveMcpToolGates } from "@/lib/mcp/gates"
import { parseMcpSessionHeader } from "@/lib/mcp/session-header"
import { isSessionWait, withKeepalive } from "@/lib/mcp/session-wait-stream"
import {
  FULL_ACCESS,
  resolveMcpAccessForGrant,
  resolveMcpTokenAccess,
  type McpAccess,
} from "@/lib/mcp/scope"

const methodNotAllowed = () =>
  new Response(
    JSON.stringify({
      jsonrpc: `2.0`,
      error: { code: -32000, message: `Use POST /api/mcp` },
      id: null,
    }),
    {
      status: 405,
      headers: { "content-type": `application/json`, allow: `POST` },
    }
  )

// Session cookies, bearer session tokens, and UNSCOPED personal `expu_` api
// keys are the user's own credentials → full membership access. A SCOPED key
// (FEED-76) is confined to the teams/boards chosen at mint, exactly like an
// OAuth2 access token (human MCP clients like Claude) is confined to the
// consent grant it resolves through.
async function resolveMcpRequest(
  request: Request
): Promise<{ userId: string; access: McpAccess } | null> {
  const { session, keyScope } = await resolveMcpCredential(request)
  if (session?.user) {
    const userId = session.user.id
    if (!keyScope) return { userId, access: FULL_ACCESS }
    return { userId, access: await resolveMcpAccessForGrant(keyScope, `apiKey`) }
  }

  const authz = request.headers.get(`authorization`)
  const bearer = authz?.match(/^Bearer\s+(.+)$/i)?.[1]
  if (!bearer) return null

  const token = await resolveMcpTokenAccess(bearer)
  if (!token) return null
  return { userId: token.userId, access: token.access }
}

async function handle(request: Request) {
  const resolved = await resolveMcpRequest(request)

  if (!resolved) {
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

  const [user] = await db
    .select()
    .from(users)
    .where(eq(users.id, resolved.userId))
    .limit(1)

  if (!user) {
    return jsonResponse(401, { error: `User not found for token` })
  }

  // EXP-637: parsed once per request; ownership is enforced per tool.
  const sessionId = parseMcpSessionHeader(request)
  // EXP-660: which conditional tool families this caller gets to see.
  const gates = await resolveMcpToolGates(user.id, resolved.access, sessionId)
  const server = createExponentialMcpServer(
    user,
    request,
    resolved.access,
    sessionId,
    gates
  )
  let parsedBody: unknown
  try {
    parsedBody = await request.clone().json()
  } catch {
    parsedBody = undefined
  }
  // FEED-83: a waiting sessions_get answers over SSE so keepalives (and its
  // progress notifications) flow while it holds; everything else stays JSON.
  const streaming = isSessionWait(parsedBody)
  const transport = new WebStandardStreamableHTTPServerTransport({
    sessionIdGenerator: undefined,
    enableJsonResponse: !streaming,
  })
  const closeAll = async () => {
    await transport.close().catch(() => {})
    await server.close().catch(() => {})
  }

  let response: Response
  try {
    await server.connect(transport)
    response = await transport.handleRequest(request, {
      parsedBody: parsedBody ?? undefined,
    })
  } catch (e) {
    await closeAll()
    throw e
  }
  if (
    !streaming ||
    !response.body ||
    !response.headers.get(`content-type`)?.includes(`text/event-stream`)
  ) {
    await closeAll()
    return response
  }
  return new Response(withKeepalive(response.body, closeAll), {
    status: response.status,
    headers: response.headers,
  })
}

export const Route = createFileRoute(`/api/mcp`)({
  server: {
    handlers: {
      POST: ({ request }) => handle(request),
      GET: methodNotAllowed,
      DELETE: methodNotAllowed,
    },
  },
})
