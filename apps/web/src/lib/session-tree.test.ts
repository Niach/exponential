import { describe, expect, it } from "vitest"
import { descendantIds, nestSessions } from "./session-tree"

// EXP-818 — the session tree's four rules. Every `it` name here is mirrored by
// iOS SessionTreeTests, Android SessionTreeTest and the desktop
// `nest_sessions_*` tests; a change on one side without the others is a
// cross-client drift.

const row = (id: string, parent: string | null = null, startedAt = `2026-09-10T10:00:00Z`) => ({
  id,
  parentSessionId: parent,
  startedAt,
})

const shape = (rows: ReturnType<typeof nestSessions>) =>
  rows.map((r) => `${r.session.id}@${r.depth}${r.hasChildren ? `+` : ``}`)

describe(`nestSessions`, () => {
  it(`keeps the caller's root order`, () => {
    expect(shape(nestSessions([row(`b`), row(`a`), row(`c`)]))).toEqual([`b@0`, `a@0`, `c@0`])
  })

  it(`nests a child only under a parent that is listed`, () => {
    expect(shape(nestSessions([row(`p`), row(`c`, `p`), row(`orphan`, `gone`)]))).toEqual([
      `p@0+`,
      `c@1`,
      `orphan@0`,
    ])
  })

  it(`lists children right after their parent, oldest first, recursively`, () => {
    const rows = nestSessions([
      row(`late`, `p`, `2026-09-10T12:00:00Z`),
      row(`p`),
      row(`grand`, `early`, `2026-09-10T13:00:00Z`),
      row(`early`, `p`, `2026-09-10T11:00:00Z`),
      row(`z`),
    ])
    expect(shape(rows)).toEqual([`p@0+`, `early@1+`, `grand@2`, `late@1`, `z@0`])
    expect(descendantIds(rows, `p`)).toEqual([`early`, `grand`, `late`])
    expect(descendantIds(rows, `early`)).toEqual([`grand`])
    expect(descendantIds(rows, `z`)).toEqual([])
  })

  it(`breaks a cycle where it first appears`, () => {
    expect(shape(nestSessions([row(`a`, `b`), row(`b`, `a`), row(`self`, `self`)]))).toEqual([
      `self@0`,
      `a@0+`,
      `b@1`,
    ])
  })
})
