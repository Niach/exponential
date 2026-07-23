import { describe, expect, it } from "vitest"
import {
  resolveAssignee,
  resolveBoard,
  resolveIssue,
  resolveLabels,
  serializeComment,
  serializeIssue,
} from "@/lib/webmcp/resolve"

const boards = [
  { slug: `exponential`, name: `Exponential` },
  { slug: `marketing`, name: `Marketing` },
]

describe(`resolveBoard`, () => {
  it(`resolves by slug case-insensitively`, () => {
    expect(resolveBoard(`EXPONENTIAL`, null, boards).slug).toBe(`exponential`)
  })

  it(`falls back to matching by exact name`, () => {
    expect(resolveBoard(`Marketing`, null, boards).slug).toBe(`marketing`)
  })

  it(`defaults to the currently open board`, () => {
    expect(resolveBoard(undefined, `marketing`, boards).slug).toBe(`marketing`)
  })

  it(`throws with the available slugs when nothing matches`, () => {
    expect(() => resolveBoard(`nope`, null, boards)).toThrow(
      /exponential, marketing/
    )
  })

  it(`throws when no ref is given and no board is open`, () => {
    expect(() => resolveBoard(undefined, null, boards)).toThrow(
      /No board specified/
    )
  })
})

describe(`resolveIssue`, () => {
  const issues = [{ identifier: `EXP-1` }, { identifier: `EXP-42` }]

  it(`resolves by identifier case-insensitively`, () => {
    expect(resolveIssue(`exp-42`, null, issues).identifier).toBe(`EXP-42`)
  })

  it(`defaults to the issue open on screen`, () => {
    expect(resolveIssue(undefined, `EXP-1`, issues).identifier).toBe(`EXP-1`)
  })

  it(`throws on an unknown identifier`, () => {
    expect(() => resolveIssue(`EXP-999`, null, issues)).toThrow(/EXP-999/)
  })

  it(`throws when no ref is given and no issue is open`, () => {
    expect(() => resolveIssue(undefined, null, issues)).toThrow(
      /No issue specified/
    )
  })
})

describe(`resolveAssignee`, () => {
  const users = [
    { id: `u1`, name: `Danny`, email: `danny@example.com` },
    { id: `u2`, name: `Alex`, email: `alex@example.com` },
    { id: `u3`, name: `Alex`, email: `alex2@example.com` },
  ]

  it(`passes null through as unassign`, () => {
    expect(resolveAssignee(null, users)).toBeNull()
  })

  it(`matches email exactly (case-insensitive)`, () => {
    expect(resolveAssignee(`Danny@Example.com`, users)).toBe(`u1`)
  })

  it(`matches a unique display name`, () => {
    expect(resolveAssignee(`danny`, users)).toBe(`u1`)
  })

  it(`throws on an ambiguous display name`, () => {
    expect(() => resolveAssignee(`Alex`, users)).toThrow(/ambiguous/)
  })

  it(`throws listing members when nothing matches`, () => {
    expect(() => resolveAssignee(`nobody`, users)).toThrow(
      /danny@example\.com/
    )
  })
})

describe(`resolveLabels`, () => {
  const labels = [
    { id: `l1`, name: `Bug` },
    { id: `l2`, name: `Feature` },
  ]

  it(`resolves multiple names case-insensitively`, () => {
    expect(resolveLabels([`bug`, `FEATURE`], labels).map((l) => l.id)).toEqual([
      `l1`,
      `l2`,
    ])
  })

  it(`throws listing available labels on a miss`, () => {
    expect(() => resolveLabels([`nope`], labels)).toThrow(/Bug, Feature/)
  })
})

describe(`serializeIssue`, () => {
  it(`resolves the assignee name and converts dates to ISO`, () => {
    const serialized = serializeIssue(
      {
        identifier: `EXP-7`,
        title: `Fix it`,
        status: `in_progress`,
        priority: `high`,
        assigneeId: `u1`,
        dueDate: `2026-08-01`,
        prUrl: null,
        prState: null,
        createdAt: new Date(`2026-07-01T10:00:00Z`),
        updatedAt: new Date(`2026-07-02T10:00:00Z`),
        source: `user`,
      },
      [`Bug`],
      new Map([[`u1`, `Danny`]])
    )
    expect(serialized).toEqual({
      identifier: `EXP-7`,
      title: `Fix it`,
      status: `in_progress`,
      priority: `high`,
      source: `user`,
      assignee: `Danny`,
      dueDate: `2026-08-01`,
      labels: [`Bug`],
      prUrl: null,
      prState: null,
      createdAt: `2026-07-01T10:00:00.000Z`,
      updatedAt: `2026-07-02T10:00:00.000Z`,
    })
  })

  it(`keeps a null assignee null and falls back to the raw id for unknown users`, () => {
    const base = {
      identifier: `EXP-8`,
      title: `t`,
      status: `backlog`,
      priority: `none`,
      dueDate: null,
      prUrl: null,
      prState: null,
      createdAt: new Date(0),
      updatedAt: new Date(0),
      source: `widget`,
    } as const
    expect(
      serializeIssue({ ...base, assigneeId: null }, [], new Map()).assignee
    ).toBeNull()
    expect(
      serializeIssue({ ...base, assigneeId: `gone` }, [], new Map()).assignee
    ).toBe(`gone`)
  })
})

describe(`serializeComment`, () => {
  it(`resolves the author name`, () => {
    expect(
      serializeComment(
        {
          id: `c1`,
          authorId: `u1`,
          body: `hello`,
          createdAt: new Date(`2026-07-03T10:00:00Z`),
        },
        new Map([[`u1`, `Danny`]])
      )
    ).toEqual({
      id: `c1`,
      author: `Danny`,
      body: `hello`,
      createdAt: `2026-07-03T10:00:00.000Z`,
    })
  })
})
