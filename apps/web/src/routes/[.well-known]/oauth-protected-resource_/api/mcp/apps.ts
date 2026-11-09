import { createFileRoute } from "@tanstack/react-router"
import { protectedResourceResponse } from "@/lib/auth/mcp-metadata"
import { mcpAppsResource } from "@/lib/auth/mcp-oauth-config"

// #VAPP-182: RFC 9728 metadata of the apps endpoint. The same authorization
// server as `/api/mcp`; `resource` names `/api/mcp/apps`, so a client sends it
// on authorize and the consent page asks for apps (it rides the signed
// consent query, lib/auth/mcp-resource.ts).
export const Route = createFileRoute(`/.well-known/oauth-protected-resource_/api/mcp/apps`)({
  server: {
    handlers: {
      GET: () => protectedResourceResponse(mcpAppsResource()),
    },
  },
})
