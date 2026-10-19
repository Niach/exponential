import { describe, expect, it } from "vitest"
import { pickPrIssues, subtreeOf } from "@/lib/sessions/pr-issues"

// EXP-1334: a PR closes its SESSION's share of the thread, not every issue
// on the root.

const node = (id: string, parentId: string | null, over: { dispatch?: boolean; aside?: boolean } = {}) => ({
  id,
  parentId,
  dispatch: over.dispatch ?? false,
  aside: over.aside ?? false,
})
const link = (issueId: string, identifier: string, addedBySessionId: string | null = null) => ({
  issueId,
  identifier,
  addedBySessionId,
})

// root (dispatcher) → a (work) → a1 (follow-up); root → b (work)
const tree = [node(`root`, null, { dispatch: true }), node(`a`, `root`), node(`a1`, `a`), node(`b`, `root`)]
const links = [link(`i-1`, `EXP-1`), link(`i-2`, `EXP-2`, `a1`), link(`i-3`, `EXP-3`, `b`), link(`i-4`, `EXP-4`)]

describe(`pickPrIssues`, () => {
  it(`the root's own PR (or no session) takes the whole thread`, () => {
    for (const sessionId of [`root`, null]) {
      expect(pickPrIssues({ rootId: `root`, sessionId, links, tree, startText: null })).toEqual([
        `i-1`,
        `i-2`,
        `i-3`,
        `i-4`,
      ])
    }
  })

  it(`a child takes its subtree's links plus the issues its start message names`, () => {
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `a`, links, tree, startText: `Fix #exp-1 (see EXP-4)` })
    ).toEqual([`i-1`, `i-2`])
    expect(pickPrIssues({ rootId: `root`, sessionId: `b`, links, tree, startText: `go` })).toEqual([`i-3`])
  })

  it(`an empty share stays empty when other work sessions own every link`, () => {
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: `a1`,
        links: [link(`i-3`, `EXP-3`, `b`), link(`i-2`, `EXP-2`, `a`)],
        tree,
        startText: ``,
      })
    ).toEqual([])
  })

  it(`two workers, empty share: a worker takes the root links no other worker's subtree added`, () => {
    // root (dispatcher) → w (the work) and, later, r (a review child): the
    // start issue and the dispatcher's own link stay w's; r's link is r's.
    const two = [
      node(`root`, null, { dispatch: true }),
      node(`w`, `root`),
      node(`wa`, `w`, { aside: true }),
      node(`r`, `root`),
      node(`r1`, `r`, { aside: true }),
    ]
    const twoLinks = [
      link(`i-1`, `EXP-1`),
      link(`i-2`, `EXP-2`, `root`),
      link(`i-3`, `EXP-3`, `r1`),
      link(`i-4`, `EXP-4`, `r`),
    ]
    expect(pickPrIssues({ rootId: `root`, sessionId: `w`, links: twoLinks, tree: two, startText: `go` })).toEqual([
      `i-1`,
      `i-2`,
    ])
    // A dispatch continuation or an aside never falls back to them.
    const cont = [...two, node(`c`, `root`, { dispatch: true })]
    expect(pickPrIssues({ rootId: `root`, sessionId: `c`, links: twoLinks, tree: cont, startText: `go` })).toEqual([])
    expect(pickPrIssues({ rootId: `root`, sessionId: `wa`, links: twoLinks, tree: two, startText: `go` })).toEqual([])
  })

  it(`the tree's ONLY work session takes the whole thread when its share is empty`, () => {
    const solo = [
      node(`root`, null, { dispatch: true }),
      node(`cont`, `root`, { dispatch: true }),
      node(`w`, `cont`),
      node(`btw`, `w`, { aside: true }),
    ]
    expect(
      pickPrIssues({ rootId: `root`, sessionId: `w`, links: [link(`i-1`, `EXP-1`)], tree: solo, startText: `do it` })
    ).toEqual([`i-1`])
  })
})

describe(`pickPrIssues: EXP-1341 assignment`, () => {
  const assigned = (issueId: string, identifier: string, to: string) => ({
    ...link(issueId, identifier),
    assignedSessionId: to,
  })
  const grouped = [assigned(`i-1`, `EXP-1`, `a`), assigned(`i-2`, `EXP-2`, `a`), assigned(`i-3`, `EXP-3`, `b`)]

  it(`a child takes the issues the dispatcher assigned to its subtree`, () => {
    expect(pickPrIssues({ rootId: `root`, sessionId: `a`, links: grouped, tree, startText: `go` })).toEqual([
      `i-1`,
      `i-2`,
    ])
    expect(pickPrIssues({ rootId: `root`, sessionId: `a1`, links: grouped, tree, startText: `` })).toEqual([])
    expect(pickPrIssues({ rootId: `root`, sessionId: `b`, links: grouped, tree, startText: `` })).toEqual([`i-3`])
  })

  it(`a worker's fallback never takes an issue assigned to another worker`, () => {
    const mixed = [link(`i-0`, `EXP-0`), assigned(`i-3`, `EXP-3`, `b`)]
    expect(pickPrIssues({ rootId: `root`, sessionId: `a`, links: mixed, tree, startText: `` })).toEqual([`i-0`])
  })

  it(`an assigned link belongs to its assignee alone: forwarded work leaves the forwarder's share`, () => {
    // a1 (in a's subtree) linked EXP-5 and forwarded it to b: it is b's.
    const forwarded = { ...link(`i-5`, `EXP-5`, `a1`), assignedSessionId: `b` }
    const links = [link(`i-2`, `EXP-2`, `a1`), forwarded]
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: `a`,
        links,
        tree,
        startText: ``,
      })
    ).toEqual([`i-2`])
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: `a1`,
        links,
        tree,
        startText: ``,
      })
    ).toEqual([`i-2`])
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: `b`,
        links,
        tree,
        startText: ``,
      })
    ).toEqual([`i-5`])
    // Even named in a's start message, the assignment wins.
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: `a`,
        links,
        tree,
        startText: `Fix #EXP-5`,
      })
    ).toEqual([`i-2`])
    // Nor does b's fallback hand it back: a's subtree assigned, b owns it.
    expect(
      pickPrIssues({
        rootId: `root`,
        sessionId: `a`,
        links: [forwarded],
        tree,
        startText: ``,
      })
    ).toEqual([])
  })
})

describe(`subtreeOf`, () => {
  it(`collects the session and every descendant`, () => {
    expect([...subtreeOf(tree, `a`)].sort()).toEqual([`a`, `a1`])
    expect([...subtreeOf(tree, `root`)].sort()).toEqual([`a`, `a1`, `b`, `root`])
  })
})
