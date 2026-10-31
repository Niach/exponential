import { createFileRoute } from "@tanstack/react-router"
import { oAuthProtectedResourceMetadata } from "better-auth/plugins"
import { auth } from "@/lib/auth"

const handler = oAuthProtectedResourceMetadata(auth)

// #VAPP-182: RFC 9728 metadata of the apps endpoint. The same authorization
// server as `/api/mcp`; `resource` names `/api/mcp/apps`, so a client sends it
// on authorize and the consent page asks for apps (mcp-authorize-guard.ts).
export const Route = createFileRoute(`/.well-known/oauth-protected-resource_/api/mcp/apps`)({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const response = await handler(request)
        const metadata = (await response.json()) as Record<string, unknown>
        const base = (process.env.BETTER_AUTH_URL ?? new URL(request.url).origin).replace(/\/$/, ``)
        const headers = new Headers(response.headers)
        headers.delete(`content-length`)
        return new Response(JSON.stringify({ ...metadata, resource: `${base}/api/mcp/apps` }), { status: response.status, headers })
      },
    },
  },
})
