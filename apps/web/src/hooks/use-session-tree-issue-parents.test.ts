import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  issueRelationCollection: {},
  messageCollection: {},
  pullRequestCollection: {},
  sessionCollection: {},
  sessionIssueCollection: {},
  sessionReadsCollection: {},
}))

import { linkedIssueParents } from "@/hooks/use-session-tree"

// EXP-1334: a linked issue's fold parent = its nearest LINKED ancestor over
// the `parent` relations (issue_id = parent, related_issue_id = child).
const issue = (id: string) => ({ id, identifier: id.toUpperCase(), title: id })
const parent = (parentId: string, childId: string) => ({
  type: `parent`,
  issueId: parentId,
  relatedIssueId: childId,
})

describe(`linkedIssueParents`, () => {
  it(`walks past unlinked ancestors to the nearest linked one`, () => {
    const out = linkedIssueParents(
      [issue(`a`), issue(`c`), issue(`d`)],
      [parent(`a`, `b`), parent(`b`, `c`), parent(`c`, `d`), { type: `blocks`, issueId: `d`, relatedIssueId: `a` }]
    )
    expect(out.map((row) => [row.id, row.parentId])).toEqual([
      [`a`, null],
      [`c`, `a`],
      [`d`, `c`],
    ])
  })

  it(`leaves a cycle and an unlinked-only ancestry top-level`, () => {
    const out = linkedIssueParents(
      [issue(`x`), issue(`y`), issue(`z`)],
      [parent(`x`, `y`), parent(`y`, `x`), parent(`q`, `z`)]
    )
    expect(out.map((row) => [row.id, row.parentId])).toEqual([
      [`x`, null],
      [`y`, null],
      [`z`, null],
    ])
  })
})
