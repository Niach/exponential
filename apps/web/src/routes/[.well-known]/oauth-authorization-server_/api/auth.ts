import { createFileRoute } from "@tanstack/react-router"
import { authorizationServerMetadata } from "@/lib/auth/mcp-metadata"

// RFC 8414 §3.1: the metadata of the issuer `<origin>/api/auth`, path-inserted.
export const Route = createFileRoute(`/.well-known/oauth-authorization-server_/api/auth`)({
  server: {
    handlers: {
      GET: ({ request }) => authorizationServerMetadata(request),
    },
  },
})
