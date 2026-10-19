import { describe, expect, it, vi } from "vitest"

// EXP-1280: `exponential_plan_propose` / `exponential_sessions_ask` post a
// card the session answers itself. A desktop/CLI before 0.14.73 feeds a
// session only its pending inputs and open cards — never a card it authored
// — so an agent that posts one and ends its turn waits forever. The gates'
// `cards` (resolved per request) drops the registration; `hostFeedsOwnCards`
// is the version rule the handlers refuse on either way.

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

import { hostFeedsOwnCards, registerExponentialTools } from "@/lib/mcp/tools"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "@/lib/mcp/gates"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import type { McpUser } from "@/lib/mcp/server"

const USER = {
  id: `user-1`,
  email: `u@example.com`,
  name: `User One`,
} as unknown as McpUser
const SESSION = `11111111-1111-4111-8111-111111111111`

function registered(gates: McpToolGates & { cards?: boolean }): Set<string> {
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

describe(`the ask tools' registration (EXP-1280 cards gate)`, () => {
  it(`registers both by default and when the host feeds its own cards`, () => {
    for (const gates of [
      ALL_MCP_TOOL_GATES,
      { ...ALL_MCP_TOOL_GATES, cards: true },
    ]) {
      const names = registered(gates)
      for (const tool of ASK_TOOLS) expect(names.has(tool), tool).toBe(true)
    }
  })

  it(`registers neither on a host that cannot deliver a card's answer`, () => {
    const names = registered({ ...ALL_MCP_TOOL_GATES, cards: false })
    for (const tool of ASK_TOOLS) expect(names.has(tool), tool).toBe(false)
    // Only those two go: the rest of the in-session surface stays.
    expect(names.has(`exponential_guide_write`)).toBe(true)
    expect(names.has(`exponential_sessions_send`)).toBe(true)
  })
})

describe(`hostFeedsOwnCards`, () => {
  it(`a reported build before 0.14.73 does not; 0.14.73 and later do`, () => {
    expect(hostFeedsOwnCards(`0.14.72`)).toBe(false)
    expect(hostFeedsOwnCards(`0.13.99`)).toBe(false)
    expect(hostFeedsOwnCards(`0.14.73`)).toBe(true)
    expect(hostFeedsOwnCards(`0.14.73-staging`)).toBe(true)
    expect(hostFeedsOwnCards(`0.15.0`)).toBe(true)
    expect(hostFeedsOwnCards(`1.0.0`)).toBe(true)
  })

  it(`an unreported or unparseable version fails open`, () => {
    expect(hostFeedsOwnCards(null)).toBe(true)
    expect(hostFeedsOwnCards(undefined)).toBe(true)
    expect(hostFeedsOwnCards(``)).toBe(true)
    expect(hostFeedsOwnCards(`dev`)).toBe(true)
  })
})
