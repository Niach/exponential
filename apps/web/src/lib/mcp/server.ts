import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { users } from "@/db/auth-schema"
import { registerExponentialTools } from "./tools"

export type McpUser = typeof users.$inferSelect

export function createExponentialMcpServer(user: McpUser, request: Request) {
  const server = new McpServer({
    name: `exponential`,
    version: `0.1.0`,
  })
  registerExponentialTools(server, user, request)
  return server
}
