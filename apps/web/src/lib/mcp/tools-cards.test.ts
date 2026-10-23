import { describe, expect, it, vi } from "vitest"

// EXP-1280: `exponential_plan_propose` / `exponential_sessions_ask` post a
// card the session answers itself, so they register only inside a session.
// Release train 2026-10-23: the host-version `cards` gate is gone (every
// desktop/CLI at the 0.14.75 floor feeds a session its own cards).

vi.mock(`@/routes/api/trpc/$`, () => ({
  appRouter: { createCaller: vi.fn(() => ({})) },
}))
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  resolveTeamAccess: vi.fn(),
  assertTeamMember: vi.fn(),
  getIssueTeamContext: vi.fn(),
  getBoardTeamId: vi.fn(),
  getAttachmentTeamContext: vi.fn(),
  getUserTeamIds: vi.fn(),
  getPublicTeamIds: vi.fn(),
}))
vi.mock(`@/lib/storage`, () => ({
  uploadObject: vi.fn(),
  deleteObject: vi.fn(),
  getObject: vi.fn(),
}))
vi.mock(`@/lib/storage/image-dimensions`, () => ({
  getImageDimensions: vi.fn(),
}))
vi.mock(`@/lib/billing`, () => ({ assertWithinStorageLimit: vi.fn() }))
vi.mock(`@/lib/integrations/github-pr`, () => ({ createPullRequest: vi.fn() }))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoInstallationToken: vi.fn(),
  resolveRepoInstallationTokenInfo: vi.fn(),
  githubOAuthClient: () => null,
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrLifecycleStatusInTx: vi.fn(),
}))
vi.mock(`@/lib/pr-stacks`, () => ({
  inferBaseBranch: vi.fn(),
  ensureGithubStack: vi.fn(),
  mergeThrough: vi.fn(),
}))
vi.mock(`@/lib/run-pr-body`, () => ({
  runPrBody: vi.fn(),
  runHasReportBody: vi.fn(),
  syncRunPrBody: vi.fn(),
  stampRunResultsPrUrl: vi.fn(async () => false),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
}))
vi.mock(`@/lib/widget/agent-report`, () => ({
  createAgentBugReport: vi.fn(),
}))

import { registerExponentialTools } from "@/lib/mcp/tools"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "@/lib/mcp/gates"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import type { McpUser } from "@/lib/mcp/server"

const USER = {
  id: `user-1`,
  email: `u@example.com`,
  name: `User One`,
} as unknown as McpUser
const SESSION = `11111111-1111-4111-8111-111111111111`

function registered(gates: McpToolGates): Set<string> {
  const names = new Set<string>()
  const fakeServer = { registerTool: (name: string) => names.add(name) }
  registerExponentialTools(
    fakeServer as never,
    USER,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS,
    SESSION,
    gates
  )
  return names
}

const ASK_TOOLS = [`exponential_plan_propose`, `exponential_sessions_ask`]

describe(`the ask tools' registration (EXP-1280)`, () => {
  it(`registers both inside a session`, () => {
    const names = registered(ALL_MCP_TOOL_GATES)
    for (const tool of ASK_TOOLS) expect(names.has(tool), tool).toBe(true)
  })

  it(`registers neither outside a session`, () => {
    const names = registered({ ...ALL_MCP_TOOL_GATES, inSession: false })
    for (const tool of ASK_TOOLS) expect(names.has(tool), tool).toBe(false)
  })
})
