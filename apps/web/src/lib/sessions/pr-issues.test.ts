import { describe, expect, it } from "vitest"
import { pickPrIssues, subtreeOf } from "@/lib/sessions/pr-issues"

// EXP-1334/NEW-1: a PR closes its SESSION's share of the thread: the rows
// its subtree HOLDS (a row sits on the session holding the issue) plus the
// tree's issues its start message names; a child with nothing of its own
// takes the rows no other live subtree holds (the root's).

const node = (id: string, parentId: string | null, status = `idle`) => ({ id, parentId, status })
const link = (issueId: string, identifier: string, sessionId: string) => ({
  issueId,
  identifier,
  sessionId,
})

// root (works) → a → a1 (follow-up); root → b
const tree = [node(`root`, null), node(`a`, `root`), node(`a1`, `a`), node(`b`, `root`)]
const links = [
  link(`i-1`, `EXP-1`, `root`),
  link(`i-2`, `EXP-2`, `a1`),
  link(`i-3`, `EXP-3`, `b`),
  link(`i-4`, `EXP-4`, `root`),
]

describe(`pickPrIssues`, () => {
  it(`the root's own PR (or no session) takes the whole thread, each issue once`, () => {
    for (const sessionId of [`root`, null]) {
      expect(pickPrIssues({ rootId: `root`, sessionId, links, tree, startText: null })).toEqual([
        `i-1`,
        `i-2`,
        `i-3`,
        `i-4`,
      ])
    }
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: null,
        links: [...links, link(`i-1`, `EXP-1`, `b`)],
        tree,
        startText: null,
      })
    ).toEqual([`i-1`, `i-2`, `i-3`, `i-4`])
  })

  it(`a child takes the rows its subtree holds plus the issues its start message names`, () => {
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `a`, links, tree, startText: `Fix #exp-1 (see EXP-4)` })
    ).toEqual([`i-1`, `i-2`])
    expect(pickPrIssues({ rootId: `root`, sessionId: `b`, links, tree, startText: `go` })).toEqual([`i-3`])
    expect(pickPrIssues({ rootId: `root`, sessionId: `a1`, links, tree, startText: `` })).toEqual([`i-2`])
  })

  it(`an empty share falls back to the rows no OTHER live subtree holds`, () => {
    const spare = [link(`i-1`, `EXP-1`, `root`), link(`i-3`, `EXP-3`, `b`)]
    // a holds nothing: the root's row is free, b's is b's.
    expect(pickPrIssues({ rootId: `root`, sessionId: `a`, links: spare, tree, startText: `` })).toEqual([
      `i-1`,
    ])
    // Once b ended, its row is free too.
    const ended = [node(`root`, null), node(`a`, `root`), node(`b`, `root`, `ended`)]
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `a`, links: spare, tree: ended, startText: `` })
    ).toEqual([`i-1`, `i-3`])
  })

  it(`the fallback never takes a row a sibling's subtree holds`, () => {
    const grouped = [link(`i-2`, `EXP-2`, `b`), link(`i-5`, `EXP-5`, `a`)]
    expect(pickPrIssues({ rootId: `root`, sessionId: `a1`, links: grouped, tree, startText: `` })).toEqual(
      []
    )
  })
})

describe(`subtreeOf`, () => {
  it(`collects the session and every descendant`, () => {
    expect([...subtreeOf(tree, `a`)].sort()).toEqual([`a`, `a1`])
    expect([...subtreeOf(tree, `root`)].sort()).toEqual([`a`, `a1`, `b`, `root`])
  })
})
