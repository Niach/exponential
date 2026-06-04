import { createFileRoute } from "@tanstack/react-router"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { resolveSessionUserId } from "@/lib/auth/resolve-bearer"
import { jsonResponse } from "@/lib/mcp/helpers"
import { createExponentialMcpServer } from "@/lib/mcp/server"

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

async function handle(request: Request) {
  // Accepts MCP OAuth2 access tokens (agent credential + human MCP clients),
  // legacy `expk_` api keys, session cookies, and bearer session tokens — all
  // via the shared resolveSession chokepoint.
  const userId = await resolveSessionUserId(request)

  if (!userId) {
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
    .where(eq(users.id, userId))
    .limit(1)

  if (!user) {
    return jsonResponse(401, { error: `User not found for token` })
  }

  const server = createExponentialMcpServer(user, request)
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
