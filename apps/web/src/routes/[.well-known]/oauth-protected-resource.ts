import { createFileRoute } from "@tanstack/react-router"
import { protectedResourceResponse } from "@/lib/auth/mcp-metadata"
import { mcpResource } from "@/lib/auth/mcp-oauth-config"

// RFC 9728 metadata of the builtin MCP endpoint (`/api/mcp`).
export const Route = createFileRoute(`/.well-known/oauth-protected-resource`)({
  server: {
    handlers: {
      GET: () => protectedResourceResponse(mcpResource()),
    },
  },
})
