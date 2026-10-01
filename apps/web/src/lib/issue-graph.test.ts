import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-graph.json"
import geometry from "@exp/domain-contract/fixtures/issue-graph-geometry.json"
import {
  blockCounts,
  blockGraph,
  blocksBadgeLabel,
  ISSUE_GRAPH_GEOMETRY,
  ISSUE_GRAPH_MAX_NODES,
  issueGraphEdge,
  issueGraphEdgePath,
  issueGraphOrigin,
  issueGraphSize,
  openBlockers,
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

// EXP-1057: the mini-graph's ONE geometry, locked ×4 against the fixture.
describe(`issue graph geometry`, () => {
  it(`matches the contract constants`, () => {
    expect(ISSUE_GRAPH_GEOMETRY).toEqual(geometry.constants)
  })
  for (const entry of geometry.sizes) {
    it(`size: ${entry.name}`, () => {
      const { name: _name, waves, lanes, ...expected } = entry
      expect(issueGraphSize(waves, lanes)).toEqual(expected)
    })
  }
  it(`places node origins`, () => {
    for (const entry of geometry.origins) {
      expect(issueGraphOrigin(entry.wave, entry.lane)).toEqual({ x: entry.x, y: entry.y })
    }
  })
  it(`draws the fixture's edge as the SVG cubic, inset-shifted for a padded grid`, () => {
    const entry = geometry.edges[0]!
    const { start, control1, control2, end } = issueGraphEdge(entry.from, entry.to)
    expect(issueGraphEdgePath(entry.from, entry.to)).toBe(
      `M ${start.x} ${start.y} C ${control1.x} ${control1.y}, ${control2.x} ${control2.y}, ${end.x} ${end.y}`
    )
    const inset = ISSUE_GRAPH_GEOMETRY.inset
    expect(issueGraphEdgePath(entry.from, entry.to, { insetIncluded: false })).toBe(
      `M ${start.x - inset} ${start.y - inset} C ${control1.x - inset} ${control1.y - inset}, ${control2.x - inset} ${control2.y - inset}, ${end.x - inset} ${end.y - inset}`
    )
  })

  for (const entry of geometry.edges) {
    it(`edge: ${entry.name}`, () => {
      const { name: _name, from, to, ...expected } = entry
      expect(issueGraphEdge(from, to)).toEqual(expected)
    })
  }
})

// SLOP-15/16: the hover graph narrows its nodes for the small chip. The
// override moves every wave, the grid and the edges together; the fixture's
// `compact` cases replay the same rules at `compactNodeWidth` on all four
// clients, and the contract constants above are untouched.
describe(`compact hover graph (SLOP-15)`, () => {
  const g = ISSUE_GRAPH_GEOMETRY
  const over = { nodeWidth: g.compactNodeWidth }

  it(`is narrower than the full box`, () => {
    expect(g.compactNodeWidth).toBeLessThan(g.nodeWidth)
  })

  it(`places the lanes by the compact width, the waves by the node height`, () => {
    expect(issueGraphOrigin(0, 1, over)).toEqual({
      x: g.inset + g.compactNodeWidth + g.laneGap,
      y: g.inset,
    })
    expect(issueGraphOrigin(1, 0, over)).toEqual(issueGraphOrigin(1, 0))
    expect(issueGraphOrigin(1, 0, {})).toEqual(issueGraphOrigin(1, 0))
  })

  for (const entry of geometry.compact.sizes) {
    it(`compact size: ${entry.name}`, () => {
      const { name: _name, waves, lanes, ...expected } = entry
      const full = issueGraphSize(waves, lanes)
      const compact = issueGraphSize(waves, lanes, over)
      expect(compact).toEqual(expected)
      // The compact width never scrolls sideways; rows keep the full height.
      expect(compact.viewWidth).toBe(compact.width)
      expect(compact.width).toBeLessThanOrEqual(full.width)
      expect(compact.height).toBe(full.height)
    })
  }

  it(`places compact node origins`, () => {
    for (const entry of geometry.compact.origins) {
      expect(issueGraphOrigin(entry.wave, entry.lane, over)).toEqual({ x: entry.x, y: entry.y })
    }
  })

  for (const entry of geometry.compact.edges) {
    it(`compact edge: ${entry.name}`, () => {
      const { name: _name, from, to, ...expected } = entry
      expect(issueGraphEdge(from, to, over)).toEqual(expected)
    })
  }

  it(`a chain of three waves fits the viewport without scrolling`, () => {
    const entry = geometry.compact.sizes.find((c) => c.name === `three waves stack compact`)!
    const size = issueGraphSize(entry.waves, entry.lanes, over)
    expect(size.width).toBeLessThan(g.maxViewWidth)
    expect(size.viewWidth).toBe(size.width)
    expect(size.viewHeight).toBe(size.height)
  })

  it(`starts an edge at the compact node's bottom-middle, inset-shifted for a padded grid`, () => {
    const entry = geometry.compact.edges[0]!
    const curve = issueGraphEdge(entry.from, entry.to, over)
    const a = issueGraphOrigin(entry.from.wave, entry.from.lane, over)
    const b = issueGraphOrigin(entry.to.wave, entry.to.lane, over)
    expect(curve.start).toEqual({ x: a.x + g.compactNodeWidth / 2, y: a.y + g.nodeHeight })
    expect(curve.end).toEqual({ x: b.x + g.compactNodeWidth / 2, y: b.y })
    expect(
      issueGraphEdgePath(entry.from, entry.to, { ...over, insetIncluded: false })
    ).toBe(
      `M ${curve.start.x - g.inset} ${curve.start.y - g.inset} C ${curve.control1.x - g.inset} ${curve.control1.y - g.inset}, ${curve.control2.x - g.inset} ${curve.control2.y - g.inset}, ${curve.end.x - g.inset} ${curve.end.y - g.inset}`
    )
  })
})

// EXP-897/980: one issue's open blockers — the same three test names ×4.
const blockerIssue = (id: string, status = `backlog`) => ({
  id,
  identifier: id.toUpperCase(),
  status,
})

/** `blocker` blocks `blocked` — the canonical direction (EXP-736). */
const blocksRow = (blocker: string, blocked: string) => ({
  type: `blocks`,
  issueId: blocker,
  relatedIssueId: blocked,
})

describe(`openBlockers`, () => {
  it(`counts only blocked-by relations`, () => {
    const issues = [blockerIssue(`me`), blockerIssue(`lower`), blockerIssue(`upper`)]
    const relations = [
      blocksRow(`lower`, `me`),
      // The other side: `me` blocks `upper`, which is not in `me`'s way.
      blocksRow(`me`, `upper`),
      // A related row is never a blocker.
      { type: `related`, issueId: `upper`, relatedIssueId: `me` },
    ]
    expect(openBlockers(`me`, relations, issues).map((row) => row.id)).toEqual([
      `lower`,
    ])
  })

  it(`drops a blocker that is done, cancelled or a duplicate`, () => {
    const issues = [
      blockerIssue(`me`),
      blockerIssue(`done`, `done`),
      blockerIssue(`cancelled`, `cancelled`),
      blockerIssue(`dupe`, `duplicate`),
      blockerIssue(`open`, `in_progress`),
    ]
    const relations = [
      blocksRow(`done`, `me`),
      blocksRow(`cancelled`, `me`),
      blocksRow(`dupe`, `me`),
      blocksRow(`open`, `me`),
    ]
    expect(openBlockers(`me`, relations, issues).map((row) => row.id)).toEqual([
      `open`,
    ])
  })

  it(`drops a blocker whose issue row is not synced`, () => {
    const issues = [blockerIssue(`me`), blockerIssue(`b`), blockerIssue(`a`)]
    const relations = [
      blocksRow(`gone`, `me`),
      blocksRow(`b`, `me`),
      blocksRow(`a`, `me`),
      // A duplicate row for the same blocker counts once.
      blocksRow(`a`, `me`),
    ]
    // …and the rest are ordered by identifier.
    expect(openBlockers(`me`, relations, issues).map((row) => row.id)).toEqual([
      `a`,
      `b`,
    ])
  })
})
