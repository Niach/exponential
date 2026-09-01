import { describe, expect, it, vi } from "vitest"
import { z } from "zod"

// EXP-705/EXP-707: drift gates for the API conventions (lib/api-conventions.ts).
// Every rule here exists because its violation once shipped silently — a tool
// added with a bare raw shape strips unknown keys, an unbounded list tool
// dumps a whole table, a hand-copied enum drifts from the contract.

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
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrLifecycleStatusInTx: vi.fn(),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
}))
vi.mock(`@/lib/widget/agent-report`, () => ({
  createAgentBugReport: vi.fn(),
}))

// Measure the worst-case surface (cloud-only report_bug included).
vi.stubEnv(`CLOUD_INSTANCE`, `true`)

import { registerExponentialTools } from "@/lib/mcp/tools"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import { ALL_MCP_TOOL_GATES } from "@/lib/mcp/gates"
import type { McpUser } from "@/lib/mcp/server"

type ToolDef = { description?: string; inputSchema?: z.ZodType }

function collectDefs(): Map<string, ToolDef> {
  const defs = new Map<string, ToolDef>()
  const fakeServer = {
    registerTool: (name: string, def: ToolDef) => {
      defs.set(name, def)
    },
  }
  registerExponentialTools(
    fakeServer as never,
    { id: `u` } as unknown as McpUser,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS,
    null,
    ALL_MCP_TOOL_GATES
  )
  return defs
}

const defs = collectDefs()

function jsonSchema(def: ToolDef): Record<string, unknown> {
  return z.toJSONSchema(def.inputSchema ?? z.strictObject({}), {
    io: `input`,
    target: `draft-7`,
  }) as Record<string, unknown>
}

describe(`strict inputs (EXP-705)`, () => {
  it(`every tool rejects unknown keys and publishes additionalProperties:false`, () => {
    expect(defs.size).toBeGreaterThan(0)
    for (const [name, def] of defs) {
      const schema = jsonSchema(def)
      expect(schema.additionalProperties, name).toBe(false)
      const parsed = def.inputSchema?.safeParse({
        __exp_bogus_key__: true,
      })
      expect(parsed?.success, name).toBe(false)
    }
  })
})
