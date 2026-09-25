import { describe, expect, it } from "vitest"
import { railNode, railWidth, RAIL_GUTTER, RAIL_NODE_WIDTH } from "./issue-rail"

// EXP-1057: the blocks rail = one dot per row in an open `blocks` relation
// (mirrored by the desktop's `domain::issue_rail`).

describe(`railNode`, () => {
  it(`rings a blocked row, fills a row that only blocks, skips the rest`, () => {
    expect(railNode({ blockedBy: 1, blocking: 2 })).toBe(`blocked`)
    expect(railNode({ blockedBy: 0, blocking: 1 })).toBe(`blocking`)
    expect(railNode({ blockedBy: 0, blocking: 0 })).toBeNull()
    expect(railNode(undefined)).toBeNull()
  })
})

describe(`railWidth`, () => {
  it(`is the gutter plus the node column once any row has a dot`, () => {
    const counts = new Map([[`b`, { blockedBy: 1, blocking: 0 }]])
    expect(railWidth([`a`, `b`], counts)).toBe(RAIL_GUTTER + RAIL_NODE_WIDTH)
    expect(RAIL_GUTTER + RAIL_NODE_WIDTH).toBe(32)
  })

  it(`is 0 when no rendered row has a dot`, () => {
    const counts = new Map([[`z`, { blockedBy: 1, blocking: 0 }]])
    expect(railWidth([`a`, `b`], counts)).toBe(0)
  })
})
