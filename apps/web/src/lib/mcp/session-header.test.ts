import { describe, expect, it } from "vitest"
import {
  MCP_SESSION_HEADER,
  parseMcpSessionHeader,
} from "@/lib/mcp/session-header"

// EXP-637: the launcher-injected header naming the coding_sessions row a tool
// call runs inside. A malformed value must read as ABSENT — the pre-EXP-637
// behaviour — never as a failed request.
function request(headers: Record<string, string>): Request {
  return new Request(`https://x.test/api/mcp`, { headers })
}

const ID = `11111111-1111-4111-8111-111111111111`

describe(`parseMcpSessionHeader`, () => {
  it(`reads the launcher's casing and normalizes the id`, () => {
    expect(parseMcpSessionHeader(request({ "X-Exp-Session-Id": ID }))).toBe(ID)
    expect(parseMcpSessionHeader(request({ [MCP_SESSION_HEADER]: ID }))).toBe(
      ID
    )
    expect(
      parseMcpSessionHeader(request({ "X-Exp-Session-Id": ` ${ID.toUpperCase()} ` }))
    ).toBe(ID)
  })

  it(`treats anything that is not a uuid as absent`, () => {
    for (const value of [
      ``,
      `   `,
      `not-a-uuid`,
      `${ID}-extra`,
      `${ID} ${ID}`,
      `' OR 1=1 --`,
    ]) {
      expect(
        parseMcpSessionHeader(request({ "X-Exp-Session-Id": value })),
        value
      ).toBeNull()
    }
    expect(parseMcpSessionHeader(request({}))).toBeNull()
  })
})
