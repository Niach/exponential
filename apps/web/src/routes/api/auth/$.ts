import { createFileRoute } from "@tanstack/react-router"
import { auth } from "@/lib/auth"
import { guardMcpAuthorize } from "@/lib/auth/mcp-authorize-guard"

// MCP OAuth authorize requests get a pre-flight (stale-client error page,
// forced prompt=consent) before better-auth handles them.
async function handleGet(request: Request) {
  if (new URL(request.url).pathname === `/api/auth/mcp/authorize`) {
    const guarded = await guardMcpAuthorize(request)
    if (`response` in guarded) return guarded.response
    return auth.handler(guarded.request)
  }
  return auth.handler(request)
}

export const Route = createFileRoute(`/api/auth/$`)({
  server: {
    handlers: {
      GET: ({ request }) => handleGet(request),
      POST: ({ request }) => auth.handler(request),
    },
  },
})
