import { createFileRoute } from "@tanstack/react-router"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import { jsonResponse } from "@/lib/mcp/helpers"
import { createExponentialMcpServer } from "@/lib/mcp/server"
import {
  FULL_ACCESS,
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

// Session cookies, bearer session tokens, and personal `expu_` api keys are
// the user's own credentials → full membership access. OAuth2 access tokens
// (human MCP clients like Claude) resolve through their consent grant and are
// confined to the workspaces/projects selected on the consent screen.
async function resolveMcpRequest(
  request: Request
): Promise<{ userId: string; access: McpAccess } | null> {
  const sessionUserId = await resolveSessionUserId(request)
  if (sessionUserId) return { userId: sessionUserId, access: FULL_ACCESS }

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

  const server = createExponentialMcpServer(user, request, resolved.access)
  const transport = new WebStandardStreamableHTTPServerTransport({
    sessionIdGenerator: undefined,
    enableJsonResponse: true,
  })

  try {
    await server.connect(transport)
    return await transport.handleRequest(request)
  } finally {
    await transport.close().catch(() => {})
    await server.close().catch(() => {})
  }
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
