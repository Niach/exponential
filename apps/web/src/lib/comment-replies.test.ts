import { describe, expect, it } from "vitest"
import { countNestedComments, nestComments } from "@/lib/comment-replies"

// EXP-741: the ONE grouping rule every client's activity feed applies.
describe(`nestComments`, () => {
  const row = (id: string, parentId: string | null = null) => ({
    id,
    parentId,
  })

  it(`keeps top-level order and groups replies under their parent in order`, () => {
    const nested = nestComments([
      row(`a`),
      row(`a1`, `a`),
      row(`b`),
      row(`a2`, `a`),
      row(`b1`, `b`),
    ])
    expect(nested.topLevel.map((c) => c.id)).toEqual([`a`, `b`])
    expect(nested.repliesByParent.get(`a`)?.map((c) => c.id)).toEqual([
      `a1`,
      `a2`,
    ])
    expect(nested.repliesByParent.get(`b`)?.map((c) => c.id)).toEqual([`b1`])
    expect(countNestedComments(nested)).toBe(5)
  })

  it(`surfaces a reply whose parent is missing as a top-level row`, () => {
    const nested = nestComments([row(`orphan`, `gone`), row(`c`)])
    expect(nested.topLevel.map((c) => c.id)).toEqual([`orphan`, `c`])
    expect(nested.repliesByParent.size).toBe(0)
  })

  it(`never nests a row under itself`, () => {
    const nested = nestComments([row(`self`, `self`)])
    expect(nested.topLevel.map((c) => c.id)).toEqual([`self`])
  })
})
