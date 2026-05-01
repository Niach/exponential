import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { registerExponentialTools } from "./tools"
import type { McpUser } from "./middleware"

export function createExponentialMcpServer(user: McpUser, request: Request) {
  const server = new McpServer({
    name: `exponential`,
    version: `0.1.0`,
  })
  registerExponentialTools(server, user, request)
  return server
}
