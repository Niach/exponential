import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { users } from "@/db/auth-schema"
import { registerExponentialTools } from "./tools"
import type { McpAccess } from "./scope"

export type McpUser = typeof users.$inferSelect

export function createExponentialMcpServer(
  user: McpUser,
  request: Request,
  access: McpAccess
) {
  const server = new McpServer({
    name: `exponential`,
    version: `0.1.0`,
  })
  registerExponentialTools(server, user, request, access)
  return server
}
