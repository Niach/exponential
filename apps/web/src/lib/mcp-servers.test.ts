import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/settings-mcp.json"
import { EMPTY_MCP_SERVER_DRAFT, type McpDialogField, type McpServerDraft, mcpDialogLine, isTeamMcp, sharedSummary } from "@/lib/mcp-servers"

// FEED-73: shared MCP connections. A team MCP = no sign-in, or at least one
// member shares their connection.

describe(`mcpDialogLine`, () => {
  for (const c of fixture.dialogLineCases) {
    it(c.name, () => {
      const draft = { ...EMPTY_MCP_SERVER_DRAFT, ...c.draft } as McpServerDraft
      const line = mcpDialogLine(draft, new Set(c.touched as McpDialogField[]), c.attempted)
      expect(line !== null).toBe(c.shows)
    })
  }
})

describe(`isTeamMcp`, () => {
  it(`counts a no-sign-in server and a shared one, never an unshared sign-in`, () => {
    expect(isTeamMcp({ auth: `none`, transport: `http`, sharedCount: 0 })).toBe(true)
    expect(isTeamMcp({ auth: `oauth`, transport: `http`, sharedCount: 1 })).toBe(true)
    expect(isTeamMcp({ auth: `secret`, transport: `http`, sharedCount: 2 })).toBe(true)
    expect(isTeamMcp({ auth: `oauth`, transport: `http`, sharedCount: 0 })).toBe(false)
    expect(isTeamMcp({ auth: `secret`, transport: `http`, sharedCount: 0 })).toBe(false)
  })

  it(`ignores shares on a stdio server: its secret stays on the owner's machine`, () => {
    expect(isTeamMcp({ auth: `secret`, transport: `stdio`, sharedCount: 2 })).toBe(false)
    expect(isTeamMcp({ auth: `secret`, transport: `stdio`, sharedCount: 0 })).toBe(false)
    // A stdio server with nothing to sign in to still needs no share.
    expect(isTeamMcp({ auth: `none`, transport: `stdio`, sharedCount: 0 })).toBe(true)
  })
})

describe(`sharedSummary`, () => {
  it(`omits the shared segment at zero`, () => {
    expect(
      sharedSummary({ connectedCount: 1, memberCount: 3, sharedCount: 0 })
    ).toBe(`1 of 3 connected`)
  })

  it(`appends the shared count`, () => {
    expect(
      sharedSummary({ connectedCount: 2, memberCount: 3, sharedCount: 1 })
    ).toBe(`2 of 3 connected · 1 shared`)
  })
})
