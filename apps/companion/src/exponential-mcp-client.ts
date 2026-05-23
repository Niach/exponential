import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js"
import type { CompanionConfig } from "./config"
import { readBotToken } from "./credentials"

export interface ExponentialProject {
  id: string
  workspaceId: string
  name: string
  slug: string
  prefix: string
  githubRepo: string | null
}

export type CommentKind = `regular` | `question` | `plan` | `activity`
export type AgentPlanState =
  | `drafting`
  | `awaiting_approval`
  | `awaiting_answer`
  | `approved`
  | null

export interface ExponentialIssueComment {
  id: string
  authorId: string
  body: { text?: string } | unknown
  kind: CommentKind
  answeredAt: string | null
  createdAt: string
  editedAt: string | null
}

export interface ExponentialIssueDetail {
  id: string
  projectId: string
  title: string
  identifier: string
  description: unknown
  agentPlanState: AgentPlanState
  agentPlanRevision: number
  agentPlanApprovedAt: string | null
  agentPlanApprovedBy: string | null
  agentLastCommentSeenAt: string | null
  recentComments: ExponentialIssueComment[]
}

export interface ExponentialMcpClient {
  createComment(args: {
    issueId: string
    bodyText: string
    kind?: CommentKind
  }): Promise<unknown>
  updateIssueStatus(args: {
    issueId: string
    status: `in_progress` | `done` | `cancelled`
  }): Promise<unknown>
  getProject(projectId: string): Promise<ExponentialProject | null>
  getIssue(issueId: string): Promise<ExponentialIssueDetail | null>
  submitAgentPlan(args: {
    issueId: string
    plan: string
    state: `awaiting_approval` | `awaiting_answer`
  }): Promise<unknown>
  resetAgentPlan(args: { issueId: string }): Promise<unknown>
  close(): Promise<void>
}

interface McpCallResult {
  content?: Array<{ type?: string; text?: string }>
  isError?: boolean
}

function parseToolPayload(raw: unknown): unknown {
  const result = raw as McpCallResult | null | undefined
  if (!result || result.isError) return null
  const text = result.content?.find((c) => c.type === `text`)?.text
  if (!text) return null
  try {
    return JSON.parse(text)
  } catch {
    return null
  }
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
    createComment: async ({ issueId, bodyText, kind }) =>
      client.callTool({
        name: `exponential_comments_create`,
        arguments: { issueId, bodyText, ...(kind ? { kind } : {}) },
      }),
    updateIssueStatus: async ({ issueId, status }) =>
      client.callTool({
        name: `exponential_issues_update`,
        arguments: { id: issueId, status },
      }),
    getProject: async (projectId) => {
      const raw = await client.callTool({
        name: `exponential_projects_get`,
        arguments: { id: projectId },
      })
      const parsed = parseToolPayload(raw) as ExponentialProject | null
      return parsed
    },
    getIssue: async (issueId) => {
      const raw = await client.callTool({
        name: `exponential_issues_get`,
        arguments: { id: issueId },
      })
      const parsed = parseToolPayload(raw) as ExponentialIssueDetail | null
      return parsed
    },
    submitAgentPlan: async ({ issueId, plan, state }) =>
      client.callTool({
        name: `exponential_agent_plan_submit`,
        arguments: { issueId, plan, state },
      }),
    resetAgentPlan: async ({ issueId }) =>
      client.callTool({
        name: `exponential_agent_plan_reset`,
        arguments: { issueId },
      }),
    close: async () => {
      await client.close()
    },
  }
}
