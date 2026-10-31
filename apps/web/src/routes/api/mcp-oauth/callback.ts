import { createFileRoute } from "@tanstack/react-router"
import { handleMcpOauthCallback } from "@/lib/emcp/upstream/callback"

// EXP-792: anonymous, state-gated redirect target of a member's MCP
// OAuth sign-in. The pipeline lives in lib/emcp/upstream/callback.ts.
export const Route = createFileRoute(`/api/mcp-oauth/callback`)({
  server: {
    handlers: {
      GET: ({ request }) => handleMcpOauthCallback(request),
    },
  },
})
