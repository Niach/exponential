// #EXP-2204: the web painting helpers of LogLines / JobGraph / CodeBlock.
import { describe, expect, it } from "bun:test"
import { ciColumns, ciCompact, ciRevealOffset, ciSegments } from "./ci"

describe(`ciSegments`, () => {
  it(`splits tokens at the marks (code points), one box per match, current flagged`, () => {
    const tokens = [{ kind: `error`, text: `(fail)` }, { kind: `plain`, text: ` store ✓ store` }]
    const marks = [{ line: 1, start: 5, end: 9 }, { line: 1, start: 15, end: 20 }]
    expect(ciSegments(tokens, marks, 3, 4)).toEqual([
      { mark: null, tokens: [{ kind: `error`, text: `(fail` }] },
      { mark: `match`, index: 3, tokens: [{ kind: `error`, text: `)` }, { kind: `plain`, text: ` st` }] },
      { mark: null, tokens: [{ kind: `plain`, text: `ore ✓ ` }] },
      { mark: `current`, index: 4, tokens: [{ kind: `plain`, text: `store` }] },
    ])
    expect(ciSegments(tokens, [], 0, -1)).toEqual([{ mark: null, tokens }])
  })
})

describe(`geometry`, () => {
  it(`columns count escape-free code points, a tab as 2`, () => {
    expect(ciColumns([`\u001b[31mab\u001b[0m`, `\tx`, `✓✓✓✓`])).toBe(4)
  })
  it(`compact below md`, () => {
    const bp = { sm: 640, md: 768, lg: 1024 }
    expect([null, `sm`, `md`, `lg`].map((b) => ciCompact(b, bp))).toEqual([true, true, false, false])
  })
  it(`reveal: the top third, clamped; a visible row stays unless always`, () => {
    expect(ciRevealOffset(600, 20, 0, 300, 2000, true)).toBe(500)
    expect(ciRevealOffset(100, 20, 0, 300, 2000, false)).toBe(0)
    expect(ciRevealOffset(100, 20, 0, 300, 2000, true)).toBe(0)
    expect(ciRevealOffset(1990, 10, 0, 300, 2000, false)).toBe(1700)
  })
})
