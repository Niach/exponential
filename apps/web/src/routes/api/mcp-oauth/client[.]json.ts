import { createFileRoute } from "@tanstack/react-router"
import { handleMcpOauthClientMetadata } from "@/lib/mcp-oauth/client-metadata"

// EXP-792: anonymous OAuth Client ID Metadata Document — the client id a
// device presents to an MCP server's authorization server. Static JSON;
// see lib/mcp-oauth/client-metadata.ts.
export const Route = createFileRoute(`/api/mcp-oauth/client.json`)({
  server: {
    handlers: {
      GET: () => handleMcpOauthClientMetadata(),
    },
  },
})
