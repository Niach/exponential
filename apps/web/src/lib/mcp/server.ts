import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { users } from "@/db/auth-schema"
import { registerExponentialTools } from "./tools"
import { MCP_SERVER_INSTRUCTIONS } from "./instructions"
import type { McpAccess } from "./scope"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "./gates"

export type McpUser = typeof users.$inferSelect

// `sessionId` (EXP-637) is the coding_sessions row this request runs inside,
// parsed from the launcher-injected X-Exp-Session-Id header. Null for every
// caller that is not a launched agent (a human's MCP client, a bare api key);
// tools that need it say so instead of guessing. `gates` (EXP-660) is what
// the route resolved for this caller (resolveMcpToolGates); the default is
// the full surface.
export function createExponentialMcpServer(
  user: McpUser,
  request: Request,
  access: McpAccess,
  sessionId: string | null = null,
  gates: McpToolGates = ALL_MCP_TOOL_GATES
) {
  const server = new McpServer(
    {
      name: `exponential`,
      version: `0.1.0`,
    },
    // Loaded up front by every client even when tool definitions are
    // deferred behind tool search — see instructions.ts for the byte budget.
    { instructions: MCP_SERVER_INSTRUCTIONS }
  )
  registerExponentialTools(server, user, request, access, sessionId, gates)
  return server
}
