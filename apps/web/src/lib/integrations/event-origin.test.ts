import { describe, expect, it, vi } from "vitest"

// #EXP-1383: the acting agent comes from the caller's OWN session (verified
// in lib/sessions/acting.ts); stubbed here per request.
const acting = vi.hoisted(() => ({ rootAgentId: null as string | null, calls: 0 }))
vi.mock(`@/lib/sessions/acting`, () => ({
  actingSessionForRequest: async (_request: Request, userId: string, sessionId: string | null) => {
    acting.calls += 1
    return sessionId && userId === `u1`
      ? { id: sessionId, teamId: `t1`, rootId: sessionId, userId, rootAgentId: acting.rootAgentId }
      : null
  },
}))

const { eventOrigin } = await import(`@/lib/integrations/event-origin`)

const RUN = `0b7c6a3e-1f2d-4c5b-9a8e-7d6c5b4a3f21`
const session = (id: string) => ({ user: { id } }) as never

function request(headers: Record<string, string> = {}) {
  return new Request(`https://x.test/api/mcp`, { headers })
}

describe(`eventOrigin (FEED-87)`, () => {
  it(`stamps an MCP call as an agent with its run`, async () => {
    acting.rootAgentId = null
    expect(
      await eventOrigin({ viaMcp: true, session: session(`u1`), request: request({ "X-Exp-Session-Id": RUN }) })
    ).toEqual({ viaMcp: true, sessionId: RUN, agentId: null })
  })

  it(`keeps a run-less MCP call an agent without a session`, async () => {
    expect(await eventOrigin({ viaMcp: true, session: session(`u1`), request: request() })).toEqual({
      viaMcp: true,
      sessionId: null,
      agentId: null,
    })
  })

  it(`never lets a person's app request speak for a run`, async () => {
    expect(
      await eventOrigin({ session: session(`u1`), request: request({ "X-Exp-Session-Id": RUN }) })
    ).toEqual({ viaMcp: false, sessionId: null, agentId: null })
  })

  it(`#EXP-1383: names the acting agent of the caller's own agent-tree session`, async () => {
    acting.rootAgentId = `agent-1`
    expect(
      await eventOrigin({ viaMcp: true, session: session(`u1`), request: request({ "X-Exp-Session-Id": RUN }) })
    ).toEqual({ viaMcp: true, sessionId: RUN, agentId: `agent-1` })
  })

  it(`#EXP-1383: a session that is not the caller's acts as nobody`, async () => {
    acting.rootAgentId = `agent-1`
    expect(
      await eventOrigin({ viaMcp: true, session: session(`u2`), request: request({ "X-Exp-Session-Id": RUN }) })
    ).toEqual({ viaMcp: true, sessionId: RUN, agentId: null })
  })
})
