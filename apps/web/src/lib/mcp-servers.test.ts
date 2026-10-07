import { describe, expect, it } from "vitest"
import { BUILTIN_CHAT_ID, BUILTIN_TIDY_UP_ID } from "@/lib/builtin-actions"
import {
  isTeamMcp,
  sharedSummary,
  subjectOwnsMcpServers,
} from "@/lib/mcp-servers"

// FEED-73: shared MCP connections. A team MCP = no sign-in, or at least one
// member shares their connection; a real action subject owns its MCP list.

describe(`isTeamMcp`, () => {
  it(`counts a no-sign-in server and a shared one, never an unshared sign-in`, () => {
    expect(isTeamMcp({ auth: `none`, sharedCount: 0 })).toBe(true)
    expect(isTeamMcp({ auth: `oauth`, sharedCount: 1 })).toBe(true)
    expect(isTeamMcp({ auth: `secret`, sharedCount: 2 })).toBe(true)
    expect(isTeamMcp({ auth: `oauth`, sharedCount: 0 })).toBe(false)
    expect(isTeamMcp({ auth: `secret`, sharedCount: 0 })).toBe(false)
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

describe(`subjectOwnsMcpServers`, () => {
  it(`is true only for a real action`, () => {
    expect(subjectOwnsMcpServers({ kind: `action`, id: `a1` })).toBe(true)
    expect(subjectOwnsMcpServers({ kind: `action`, id: BUILTIN_TIDY_UP_ID })).toBe(false)
    expect(subjectOwnsMcpServers({ kind: `action`, id: BUILTIN_CHAT_ID })).toBe(false)
    expect(subjectOwnsMcpServers({ kind: `issues` })).toBe(false)
    expect(subjectOwnsMcpServers(null)).toBe(false)
  })
})
