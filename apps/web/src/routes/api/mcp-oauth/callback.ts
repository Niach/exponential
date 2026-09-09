import { createFileRoute } from "@tanstack/react-router"
import { handleMcpOauthCallback } from "@/lib/mcp-oauth/callback"

// EXP-792: anonymous, state-gated redirect target of a device-executed MCP
// OAuth sign-in. The pipeline lives in lib/mcp-oauth/callback.ts.
export const Route = createFileRoute(`/api/mcp-oauth/callback`)({
  server: {
    handlers: {
      GET: ({ request }) => handleMcpOauthCallback(request),
    },
  },
})
