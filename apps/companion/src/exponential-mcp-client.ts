import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js"
import type { CompanionConfig } from "./config"
import { readBotToken } from "./credentials"

export interface ExponentialMcpClient {
  createComment(args: { issueId: string; bodyText: string }): Promise<unknown>
  updateIssueStatus(args: {
    issueId: string
    status: `in_progress` | `done` | `cancelled`
  }): Promise<unknown>
  close(): Promise<void>
}

export async function connectExponentialMcp(
  config: CompanionConfig
): Promise<ExponentialMcpClient> {
  const token = await readBotToken()
  const url = new URL(
    `${config.exponential.baseUrl.replace(/\/$/, ``)}/api/mcp`
  )
  const transport = new StreamableHTTPClientTransport(url, {
    requestInit: {
      headers: { Authorization: `Bearer ${token}` },
    },
  })
  const client = new Client(
    { name: `exponential-companion`, version: `0.1.0` },
    { capabilities: {} }
  )
  await client.connect(transport)

  return {
    createComment: async ({ issueId, bodyText }) =>
      client.callTool({
        name: `exponential_comments_create`,
        arguments: { issueId, bodyText },
      }),
    updateIssueStatus: async ({ issueId, status }) =>
      client.callTool({
        name: `exponential_issues_update`,
        arguments: { id: issueId, status },
      }),
    close: async () => {
      await client.close()
    },
  }
}
