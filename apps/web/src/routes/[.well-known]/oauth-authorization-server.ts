import { createFileRoute } from "@tanstack/react-router"
import { authorizationServerMetadata } from "@/lib/auth/mcp-metadata"

export const Route = createFileRoute(`/.well-known/oauth-authorization-server`)({
  server: {
    handlers: {
      GET: ({ request }) => authorizationServerMetadata(request),
    },
  },
})
