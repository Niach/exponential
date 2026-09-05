import { describe, expect, it } from "vitest"
import { countThreadedComments, threadComments } from "@/lib/comment-threads"

// EXP-741: the ONE grouping rule every client's activity feed applies.
describe(`threadComments`, () => {
  const row = (id: string, parentId: string | null = null) => ({
    id,
    parentId,
  })

  it(`keeps top-level order and groups replies under their parent in order`, () => {
    const threads = threadComments([
      row(`a`),
      row(`a1`, `a`),
      row(`b`),
      row(`a2`, `a`),
      row(`b1`, `b`),
    ])
    expect(threads.topLevel.map((c) => c.id)).toEqual([`a`, `b`])
    expect(threads.repliesByParent.get(`a`)?.map((c) => c.id)).toEqual([
      `a1`,
      `a2`,
    ])
    expect(threads.repliesByParent.get(`b`)?.map((c) => c.id)).toEqual([`b1`])
    expect(countThreadedComments(threads)).toBe(5)
  })

  it(`surfaces a reply whose parent is missing as a top-level row`, () => {
    const threads = threadComments([row(`orphan`, `gone`), row(`c`)])
    expect(threads.topLevel.map((c) => c.id)).toEqual([`orphan`, `c`])
    expect(threads.repliesByParent.size).toBe(0)
  })

  it(`never nests a row under itself`, () => {
    const threads = threadComments([row(`self`, `self`)])
    expect(threads.topLevel.map((c) => c.id)).toEqual([`self`])
  })
})
