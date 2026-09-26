import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"
import {
  ISSUE_GRAPH_GEOMETRY,
  issueGraphEdgePath,
  type IssueGraph,
} from "@/lib/issue-graph"

import { IssueGraphView } from "./issue-graph"

// EXP-1057: the mini-graph's edges are THE contract's (`issue-graph-
// geometry.json`, locked in `lib/issue-graph.test.ts`): the view hands
// `WaveGraph` the fixture cubic through `pathFor` and the fixture's stroke,
// never the grid's own bow.

vi.mock(`@/lib/collections`, () => ({
  boardCollection: {},
  teamCollection: {},
}))
vi.mock(`@/hooks/use-team-issue-graph`, () => ({
  useTeamIssueGraph: () => ({ relations: [], issues: [] }),
}))
vi.mock(`@tanstack/react-router`, () => ({
  Link: (props: Record<string, unknown>) => <a {...props} />,
}))
vi.mock(`@tanstack/react-db`, () => ({
  eq: () => true,
  useLiveQuery: () => ({ data: [] }),
}))

const issue = (id: string, identifier: string) =>
  ({ id, identifier, title: identifier, boardId: `b` }) as unknown as Issue

describe(`IssueGraphView`, () => {
  it(`draws every edge as the contract's cubic with the contract's stroke`, () => {
    const graph: IssueGraph = {
      nodes: [
        { id: `a`, wave: 0, lane: 0, subject: false },
        { id: `b`, wave: 1, lane: 1, subject: true },
        { id: `c`, wave: 1, lane: 0, subject: false },
      ],
      edges: [
        { from: `a`, to: `b`, cycle: false },
        { from: `c`, to: `a`, cycle: true },
      ],
      hasCycle: true,
      truncated: false,
    }
    const issueById = new Map([
      [`a`, issue(`a`, `EXP-1`)],
      [`b`, issue(`b`, `EXP-2`)],
      [`c`, issue(`c`, `EXP-3`)],
    ])
    const { getByTestId } = render(
      <IssueGraphView
        graph={graph}
        issueById={issueById}
        renderNode={(row) => <span>{row.identifier}</span>}
      />
    )
    const plain = getByTestId(`issue-graph-edge`)
    expect(plain.getAttribute(`d`)).toBe(
      issueGraphEdgePath(graph.nodes[0]!, graph.nodes[1]!, { insetIncluded: false })
    )
    expect(plain.getAttribute(`stroke-width`)).toBe(String(ISSUE_GRAPH_GEOMETRY.edgeStroke))
    const cycle = getByTestId(`issue-graph-cycle-edge`)
    expect(cycle.getAttribute(`d`)).toBe(
      issueGraphEdgePath(graph.nodes[2]!, graph.nodes[0]!, { insetIncluded: false })
    )
    // The grid pads by the inset, so the path (inset-free) plus the padding
    // lands the curve where the contract puts it.
    expect(getByTestId(`issue-graph-node-EXP-2`).style.left).toBe(
      `${ISSUE_GRAPH_GEOMETRY.nodeWidth + ISSUE_GRAPH_GEOMETRY.waveGap}px`
    )
  })
})
