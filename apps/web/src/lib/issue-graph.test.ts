import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-graph.json"
import {
  blockCounts,
  blockGraph,
  blocksBadgeLabel,
  ISSUE_GRAPH_MAX_NODES,
  openBlockersOfSet,
  type BlockCounts,
  type GraphIssue,
  type GraphRelation,
  type IssueGraph,
} from "./issue-graph"

// EXP-980: the blocks-graph rule, locked ×4 (Android IssueGraphTest, iOS
// IssueGraphTests, desktop domain::issue_graph) against the ONE contract
// fixture — same cases, same test names.
interface FixtureCase {
  name: string
  issues: GraphIssue[]
  relations: GraphRelation[]
  expectedCounts?: Record<string, BlockCounts>
  picked?: string[]
  expectedSetBlockers?: string[]
  subjects?: string[]
  expectedGraph?: IssueGraph
}

const cases = fixture as unknown as FixtureCase[]

describe(`issue graph (contract fixture)`, () => {
  for (const c of cases) {
    it(c.name, () => {
      if (c.expectedCounts) {
        expect(Object.fromEntries(blockCounts(c.relations, c.issues))).toEqual(
          c.expectedCounts
        )
      }
      if (c.expectedSetBlockers) {
        expect(
          openBlockersOfSet(c.picked ?? [], c.relations, c.issues).map((i) => i.id)
        ).toEqual(c.expectedSetBlockers)
      }
      if (c.expectedGraph) {
        expect(blockGraph(c.subjects ?? [], c.relations, c.issues)).toEqual(
          c.expectedGraph
        )
      }
    })
  }
})

describe(`blocks badge label`, () => {
  it(`names the side that has a count`, () => {
    expect(blocksBadgeLabel({ blockedBy: 2, blocking: 0 })).toBe(`Blocked by 2`)
    expect(blocksBadgeLabel({ blockedBy: 0, blocking: 1 })).toBe(`Blocking 1`)
    expect(blocksBadgeLabel({ blockedBy: 2, blocking: 1 })).toBe(
      `Blocked by 2, blocking 1`
    )
  })
})

describe(`issue graph node cap`, () => {
  it(`cuts the closure at the node cap, nearest blockers first`, () => {
    const total = ISSUE_GRAPH_MAX_NODES + 5
    const issues = Array.from({ length: total }, (_, i) => ({
      id: `n${i}`,
      identifier: `EXP-${String(1000 + i)}`,
      status: `backlog`,
    }))
    // A chain n(total-1) → … → n1 → n0; the subject is the most blocked end.
    const relations = issues.slice(1).map((issue, i) => ({
      type: `blocks`,
      issueId: issue.id,
      relatedIssueId: `n${i}`,
    }))
    const graph = blockGraph([`n0`], relations, issues)
    expect(graph.truncated).toBe(true)
    expect(graph.nodes).toHaveLength(ISSUE_GRAPH_MAX_NODES)
    expect(graph.nodes.at(-1)).toEqual({
      id: `n0`,
      wave: ISSUE_GRAPH_MAX_NODES - 1,
      lane: 0,
      subject: true,
    })
  })
})
