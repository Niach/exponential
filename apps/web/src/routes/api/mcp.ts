import { createFileRoute } from "@tanstack/react-router"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { users } from "@/db/auth-schema"
import { auth } from "@/lib/auth"
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

// Resolve a user from the request, accepting both Better Auth's MCP OAuth2
// access tokens (issued via the `mcp` plugin) and `apiKey` plugin bearer
// tokens (which mock a session when `enableSessionForAPIKeys: true`).
async function resolveMcpUserId(request: Request): Promise<string | null> {
  // OAuth path: existing MCP clients that completed the OAuth flow.
  const mcpSession = await auth.api
    .getMcpSession({ request, headers: request.headers, asResponse: false })
    .catch(() => null)
  if (mcpSession?.userId) return mcpSession.userId

  // api-key path: agent companions authenticating with `Authorization: Bearer
  // expk_...`. The api-key plugin's `customAPIKeyGetter` (configured in
  // src/lib/auth.ts) pulls the key from either `x-api-key` or that header.
  const session = await auth.api
    .getSession({ headers: request.headers })
    .catch(() => null)
  return session?.user?.id ?? null
}

async function handle(request: Request) {
  const userId = await resolveMcpUserId(request)

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
