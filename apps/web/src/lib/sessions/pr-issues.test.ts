import { describe, expect, it } from "vitest"
import { pickPrIssues, prDefaultIssueIds, subtreeOf } from "@/lib/sessions/pr-issues"

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

describe(`EXP-1458: the start message's ref_issue_ids`, () => {
  it(`names issues beside (or instead of) the parsed #IDENTs`, () => {
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `b`, links, tree, startText: `go`, startRefIssueIds: [`i-4`] })
    ).toEqual([`i-3`, `i-4`])
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `a`, links, tree, startText: `Fix #EXP-1`, startRefIssueIds: [`i-4`] })
    ).toEqual([`i-1`, `i-2`, `i-4`])
  })

  // A fake reader: each select() answers the next queued rows (tree, links,
  // start message, merged links), whatever the chain.
  const reader = (answers: unknown[][]) => {
    const queue = [...answers]
    return {
      select: () => {
        const rows = queue.shift() ?? []
        const chain: Record<string, unknown> = {
          then: (ok: (value: unknown) => unknown, fail?: (err: unknown) => unknown) => Promise.resolve(rows).then(ok, fail),
        }
        for (const method of [`from`, `where`, `innerJoin`, `orderBy`, `limit`]) chain[method] = () => chain
        return chain
      },
    } as never
  }
  const sealed = `exp-sealed:v1:1:${`A`.repeat(80)}`

  it(`a sealed start message is never parsed: only its ref_issue_ids count`, async () => {
    // `#EXP-1` inside the envelope does not count: the body is opaque.
    const ids = await prDefaultIssueIds(
      reader([tree, links, [{ body: `${sealed} #EXP-1`, refIssueIds: [`i-4`] }], []]),
      { rootId: `root`, sessionId: `b` }
    )
    expect(ids).toEqual([`i-3`, `i-4`])
  })

  it(`a plaintext start message parses its #IDENTs and honours ref_issue_ids`, async () => {
    const ids = await prDefaultIssueIds(
      reader([tree, links, [{ body: `Fix #EXP-1`, refIssueIds: [`i-4`] }], [{ issueId: `i-4` }]]),
      { rootId: `root`, sessionId: `b` }
    )
    // i-4 was closed by a merged PR: done work is never re-linked.
    expect(ids).toEqual([`i-1`, `i-3`])
  })
})

describe(`subtreeOf`, () => {
  it(`collects the session and every descendant`, () => {
    expect([...subtreeOf(tree, `a`)].sort()).toEqual([`a`, `a1`])
    expect([...subtreeOf(tree, `root`)].sort()).toEqual([`a`, `a1`, `b`, `root`])
  })
})
