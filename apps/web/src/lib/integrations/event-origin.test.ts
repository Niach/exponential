import { describe, expect, it } from "vitest"
import { eventOrigin } from "@/lib/integrations/event-origin"

const RUN = `0b7c6a3e-1f2d-4c5b-9a8e-7d6c5b4a3f21`

function request(headers: Record<string, string> = {}) {
  return new Request(`https://x.test/api/mcp`, { headers })
}

describe(`eventOrigin (FEED-87)`, () => {
  it(`stamps an MCP call as an agent with its run`, () => {
    expect(
      eventOrigin({ viaMcp: true, request: request({ "X-Exp-Session-Id": RUN }) })
    ).toEqual({ viaMcp: true, sessionId: RUN })
  })

  it(`keeps a run-less MCP call an agent without a session`, () => {
    expect(eventOrigin({ viaMcp: true, request: request() })).toEqual({
      viaMcp: true,
      sessionId: null,
    })
  })

  it(`never lets a person's app request speak for a run`, () => {
    expect(
      eventOrigin({ request: request({ "X-Exp-Session-Id": RUN }) })
    ).toEqual({ viaMcp: false, sessionId: null })
  })
})
