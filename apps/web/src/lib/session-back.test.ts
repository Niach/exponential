import { describe, expect, it } from "vitest"
import { BACK_STACK_CAP, backParam, backSearch, parseBack, pushBack, sessionTarget, type TargetSession } from "./session-back"

// S-3: the web back stack (`back` in the route search) and where an xref opens.
describe(`session back stack`, () => {
  it(`parses, caps and serialises`, () => {
    expect(parseBack(undefined)).toEqual([])
    expect(parseBack(` a, ,b `)).toEqual([`a`, `b`])
    const long = Array.from({ length: BACK_STACK_CAP + 3 }, (_, i) => `s${i}`)
    expect(parseBack(long.join(`,`))).toEqual(long.slice(-BACK_STACK_CAP))
    expect(backParam([])).toBeUndefined()
    expect(backSearch(``)).toEqual({})
    expect(backSearch(`a,b`)).toEqual({ back: `a,b` })
  })

  it(`pushes the session on show once, never null`, () => {
    expect(pushBack([`a`], `b`)).toEqual([`a`, `b`])
    expect(pushBack([`a`, `b`], `b`)).toEqual([`a`, `b`])
    expect(pushBack([`a`], null)).toEqual([`a`])
    const full = Array.from({ length: BACK_STACK_CAP }, (_, i) => `s${i}`)
    expect(pushBack(full, `x`)).toHaveLength(BACK_STACK_CAP)
    expect(pushBack(full, `x`).at(-1)).toBe(`x`)
  })

  it(`opens a thread child in place, the viewer's agent conversation on the agent page, the rest by id`, () => {
    const rows: Record<string, TargetSession> = {
      t1: { id: `t1`, rootId: `t1`, parentId: null, agentId: null, userId: `u1` },
      c1: { id: `c1`, rootId: `t1`, parentId: `t1`, agentId: null, userId: `u1` },
      ac1: { id: `ac1`, rootId: `ac1`, parentId: null, agentId: `a1`, userId: `u1` },
      ac2: { id: `ac2`, rootId: `ac2`, parentId: null, agentId: `a1`, userId: `u2` },
      ak1: { id: `ak1`, rootId: `ac1`, parentId: `ac1`, agentId: null, userId: `u1` },
    }
    const lookup = (id: string) => rows[id]
    expect(sessionTarget(`t1`, lookup, `u1`)).toEqual({ to: `thread`, threadId: `t1`, session: null })
    expect(sessionTarget(`c1`, lookup, `u1`)).toEqual({ to: `thread`, threadId: `t1`, session: `c1` })
    expect(sessionTarget(`ac1`, lookup, `u1`)).toEqual({ to: `agent`, agentId: `a1` })
    expect(sessionTarget(`ac2`, lookup, `u1`)).toEqual({ to: `session`, sessionId: `ac2` })
    expect(sessionTarget(`ak1`, lookup, `u1`)).toEqual({ to: `session`, sessionId: `ak1` })
    expect(sessionTarget(`ghost`, lookup, `u1`)).toEqual({ to: `session`, sessionId: `ghost` })
  })
})
