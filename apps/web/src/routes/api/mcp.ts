import { createFileRoute } from "@tanstack/react-router"
import { exponentialToolProvider, serveMcp } from "@/lib/emcp/gate"

const methodNotAllowed = () =>
  new Response(
    JSON.stringify({
      jsonrpc: `2.0`,
      error: { code: -32000, message: `Use POST /api/mcp` },
      id: null,
    }),
    {
      status: 405,
      headers: { "content-type": `application/json`, allow: `POST` },
    }
  )

// #EXP-1491: the gate (lib/emcp/gate.ts) authenticates, confines, serves
// and audits; Exponential's 16 tools are its tool provider.
export const Route = createFileRoute(`/api/mcp`)({
  server: {
    handlers: {
      POST: ({ request }) => serveMcp(request, exponentialToolProvider),
      GET: methodNotAllowed,
      DELETE: methodNotAllowed,
    },
  },
})
