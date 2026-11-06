import { describe, expect, it } from "vitest"
import { pickPrIssues, prDefaultIssueIds, subtreeOf } from "@/lib/sessions/pr-issues"

// EXP-1334/NEW-1: a PR closes its SESSION's share of the thread: the rows
// its subtree HOLDS (a row sits on the session holding the issue) plus the
// tree's issues its start message names; a child with nothing of its own
// links nothing (#EXP-1402, FEED-166).

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
  it(`the root's own PR (or no session) takes the root's rows nobody else holds, never the thread (EXP-1706)`, () => {
    for (const sessionId of [`root`, null]) {
      expect(pickPrIssues({ rootId: `root`, sessionId, links, tree, startText: null })).toEqual([`i-1`, `i-4`])
    }
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: null,
        links: [...links, link(`i-1`, `EXP-1`, `b`)],
        tree,
        startText: null,
      })
    ).toEqual([`i-4`])
  })

  it(`a child takes the rows its subtree holds plus the issues its start message names`, () => {
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `a`, links, tree, startText: `Fix #exp-1 (see EXP-4)` })
    ).toEqual([`i-1`, `i-2`])
    expect(pickPrIssues({ rootId: `root`, sessionId: `b`, links, tree, startText: `go` })).toEqual([`i-3`])
    expect(pickPrIssues({ rootId: `root`, sessionId: `a1`, links, tree, startText: `` })).toEqual([`i-2`])
  })

  it(`FEED-166: a child with no share links nothing, never the thread's issues`, () => {
    // The thread holds EXP-1416, NEW-64, NEW-65 on the root; child d19dcb89
    // holds none and names none: its PR links none of them.
    const thread = [link(`i-1416`, `EXP-1416`, `root`), link(`i-n64`, `NEW-64`, `root`), link(`i-n65`, `NEW-65`, `b`)]
    expect(pickPrIssues({ rootId: `root`, sessionId: `a`, links: thread, tree, startText: `Write the summary` })).toEqual([])
    // Ended siblings free nothing either.
    const ended = [node(`root`, null), node(`a`, `root`), node(`b`, `root`, `ended`)]
    expect(pickPrIssues({ rootId: `root`, sessionId: `a`, links: thread, tree: ended, startText: `` })).toEqual([])
    // Naming one takes exactly that one.
    expect(pickPrIssues({ rootId: `root`, sessionId: `a`, links: thread, tree, startText: `Fix #NEW-64` })).toEqual([
      `i-n64`,
    ])
  })
})

describe(`#EXP-1402: a continuation's share is its lineage's`, () => {
  // root holds EXP-5; a was started with "implement #EXP-5" and ended; its
  // continuation c ("fix the lint") opens the PR.
  const lineage = [
    node(`root`, null),
    { ...node(`a`, `root`, `ended`), continuedFromId: null },
    { ...node(`c`, `root`, `running`), continuedFromId: `a` },
    node(`b`, `root`),
    node(`a-kid`, `a`, `ended`),
  ]
  const held = [
    link(`i-5`, `EXP-5`, `root`),
    link(`i-6`, `EXP-6`, `root`),
    link(`i-7`, `EXP-7`, `b`),
    link(`i-8`, `EXP-8`, `a-kid`),
  ]

  it(`takes what the predecessor named and its subtree held, never the whole thread`, () => {
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: `c`,
        links: held,
        tree: lineage,
        startText: `fix the lint`,
        lineageStarts: [{ text: `implement #EXP-5` }],
      })
    ).toEqual([`i-5`, `i-8`])
    // Without the lineage brief it still takes the predecessor's subtree rows.
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `c`, links: held, tree: lineage, startText: `fix the lint` })
    ).toEqual([`i-8`])
  })

  it(`a moved root in the lineage adds only its own rows`, () => {
    const moved = [node(`root`, null, `ended`), { ...node(`c`, `root`), continuedFromId: `root` }, node(`b`, `root`)]
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `c`, links: held, tree: moved, startText: `go` })
    ).toEqual([`i-5`, `i-6`])
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

describe(`#EXP-1402: prDefaultIssueIds reads the lineage's start messages`, () => {
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

  it(`the continuation inherits its ended predecessor's brief`, async () => {
    const lineage = [node(`root`, null), node(`a`, `root`, `ended`), { ...node(`c`, `root`), continuedFromId: `a` }]
    const ids = await prDefaultIssueIds(
      reader([
        lineage,
        [link(`i-5`, `EXP-5`, `root`), link(`i-6`, `EXP-6`, `root`)],
        [{ body: `fix the lint`, refIssueIds: null }],
        [{ body: `implement #EXP-5`, refIssueIds: null }],
        [],
      ]),
      { rootId: `root`, sessionId: `c` }
    )
    expect(ids).toEqual([`i-5`])
  })
})

describe(`subtreeOf`, () => {
  it(`collects the session and every descendant`, () => {
    expect([...subtreeOf(tree, `a`)].sort()).toEqual([`a`, `a1`])
    expect([...subtreeOf(tree, `root`)].sort()).toEqual([`a`, `a1`, `b`, `root`])
  })
})
