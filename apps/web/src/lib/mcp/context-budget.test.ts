import { expect, it, vi } from "vitest"
import { existsSync, readFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { z } from "zod"

// EXP-353: the serialized MCP tool definitions land verbatim in every MCP
// client's context window, and Claude Code warns above ~25k. Keep the surface
// lean: uuid params use the compact `uuidString` (never z.uuid()'s 155-char
// pattern), icon params never inline the 60-name enum, descriptions stay
// terse. Same story for CLAUDE.md — Claude Code warns above 40k chars.

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

import { registerExponentialTools } from "@/lib/mcp/tools"
import { FULL_ACCESS } from "@/lib/mcp/scope"
import type { McpUser } from "@/lib/mcp/server"

type ToolDef = {
  description?: string
  inputSchema?: Record<string, z.ZodType>
}

function serializeToolDefs() {
  const defs: Array<Record<string, unknown>> = []
  const fakeServer = {
    registerTool: (name: string, def: ToolDef) => {
      // Mirror the MCP SDK's tools/list serialization (name + description +
      // JSON-schema'd input; draft-7 target like zod-json-schema-compat).
      defs.push({
        name,
        description: def.description,
        inputSchema: z.toJSONSchema(z.object(def.inputSchema ?? {}), {
          io: `input`,
          target: `draft-7`,
        }),
      })
    },
  }
  registerExponentialTools(
    fakeServer as never,
    { id: `u` } as unknown as McpUser,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS
  )
  return defs
}

it(`keeps the serialized MCP tool context within budget`, () => {
  const defs = serializeToolDefs()
  const total = JSON.stringify(defs).length
  // 23.2k as of EXP-353. If this trips, trim schemas/descriptions — do not
  // just raise the ceiling: Claude Code's large-MCP-context warning is ~25k.
  expect(total).toBeLessThan(24_000)
  for (const def of defs) {
    // No single tool may reintroduce a fat inline enum or a novella.
    expect(JSON.stringify(def).length, `${def.name}`).toBeLessThan(1_800)
  }
})

it(`keeps CLAUDE.md under Claude Code's 40k-char performance warning`, () => {
  // Walk up from the vitest cwd (apps/web) to the repo root.
  let dir = process.cwd()
  while (!existsSync(join(dir, `CLAUDE.md`)) && dirname(dir) !== dir) {
    dir = dirname(dir)
  }
  const claudeMd = readFileSync(join(dir, `CLAUDE.md`), `utf8`)
  expect(claudeMd.length).toBeLessThan(40_000)
})
