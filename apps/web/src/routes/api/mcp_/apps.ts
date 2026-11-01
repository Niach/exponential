import { createFileRoute } from "@tanstack/react-router"
import { authenticateMcpRequest, serveMcp } from "@/lib/emcp/gate"
import { appsListChangedStream, appsToolProvider } from "@/lib/emcp/apps-broker"

const methodNotAllowed = () =>
  new Response(
    JSON.stringify({ jsonrpc: `2.0`, error: { code: -32000, message: `Use POST /api/mcp/apps` }, id: null }),
    { status: 405, headers: { "content-type": `application/json`, allow: `GET, POST` } }
  )

// #VAPP-181: the ONE global endpoint of every app's exposed tools. POST =
// the gate (lib/emcp/gate.ts) with the app broker as its tool provider; GET
// = the server-to-client stream that sends notifications/tools/list_changed
// when what the caller is shown changes (a new head version, a trashed app).
export const Route = createFileRoute(`/api/mcp_/apps`)({
  server: {
    handlers: {
      POST: ({ request }) => serveMcp(request, appsToolProvider),
      GET: async ({ request }) => {
        const gated = await authenticateMcpRequest(request)
        if (`response` in gated) return gated.response
        return appsListChangedStream(gated.caller, request)
      },
      DELETE: methodNotAllowed,
    },
  },
})
