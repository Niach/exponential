import { createFileRoute } from "@tanstack/react-router"
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js"
import { mcpAuthenticate } from "@/lib/mcp/middleware"
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

const handle = async ({ request }: { request: Request }) => {
  const auth = await mcpAuthenticate(request)
  if (`errorResponse` in auth) return auth.errorResponse

  const server = createExponentialMcpServer(auth.user, request)
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
      POST: handle,
      GET: methodNotAllowed,
      DELETE: methodNotAllowed,
    },
  },
})
