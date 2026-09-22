import { describe, expect, it } from "vitest"
import {
  issueRail,
  railEdgeLabel,
  railLaneX,
  railRowIsEmpty,
  railWidth,
  RAIL_LANE_PITCH,
  RAIL_NODE_WIDTH,
  type RailEntry,
  type RailLane,
} from "./issue-rail"
import type { GraphIssue, GraphRelation } from "./issue-graph"

// EXP-998: the blocks rail's lane rule (mirrored by the desktop's
// `domain::issue_rail`).

const issue = (id: string, status = `backlog`): GraphIssue => ({
  id,
  identifier: `EXP-${id.toUpperCase()}`,
  status,
})
const blocks = (from: string, to: string): GraphRelation => ({
  type: `blocks`,
  issueId: from,
  relatedIssueId: to,
})
const row = (id: string): RailEntry => ({ kind: `row`, id })
const gap: RailEntry = { kind: `gap` }

/** A lane's shape as a compact tuple: [lane, top, bottom, into, out]. */
const shape = (lane: RailLane) => [
  lane.lane,
  lane.top,
  lane.bottom,
  lane.into,
  lane.out,
]

describe(`issueRail`, () => {
  it(`draws nothing for a list with no open blocks relation`, () => {
    const rail = issueRail(
      [row(`a`), row(`b`)],
      [{ type: `parent`, issueId: `a`, relatedIssueId: `b` }],
      [issue(`a`), issue(`b`)]
    )
    expect(rail.hasNodes).toBe(false)
    expect(rail.laneCount).toBe(0)
    expect(railWidth(rail)).toBe(0)
    expect(rail.entries.every(railRowIsEmpty)).toBe(true)
  })

  it(`runs one lane from the blocker down into the blocked row`, () => {
    const rail = issueRail(
      [row(`a`), gap, row(`b`)],
      [blocks(`a`, `b`)],
      [issue(`a`), issue(`b`)]
    )
    expect(rail.laneCount).toBe(1)
    expect(rail.hasNodes).toBe(true)
    expect(railWidth(rail)).toBe(RAIL_NODE_WIDTH + RAIL_LANE_PITCH)
    const [a, header, b] = rail.entries
    expect(a!.node).toBe(`blocking`)
    expect(a!.lanes.map(shape)).toEqual([[0, false, true, false, true]])
    // The header only carries the line through.
    expect(header!.node).toBeNull()
    expect(header!.lanes.map(shape)).toEqual([[0, true, true, false, false]])
    expect(b!.node).toBe(`blocked`)
    expect(b!.counts).toEqual({ blockedBy: 1, blocking: 0 })
    expect(b!.lanes.map(shape)).toEqual([[0, true, false, true, false]])
    expect(b!.lanes[0]!.edges).toEqual([{ key: `a:b`, label: `EXP-A blocks EXP-B` }])
  })

  it(`points the arrow up when the blocker sits below`, () => {
    const rail = issueRail(
      [row(`b`), row(`a`)],
      [blocks(`a`, `b`)],
      [issue(`a`), issue(`b`)]
    )
    const [b, a] = rail.entries
    expect(b!.lanes.map(shape)).toEqual([[0, false, true, true, false]])
    expect(a!.lanes.map(shape)).toEqual([[0, true, false, false, true]])
  })

  it(`gives overlapping edges their own lanes, shortest innermost`, () => {
    // a blocks b and c: [0,1] and [0,2] overlap at row 0.
    const rail = issueRail(
      [row(`a`), row(`b`), row(`c`)],
      [blocks(`a`, `b`), blocks(`a`, `c`)],
      [issue(`a`), issue(`b`), issue(`c`)]
    )
    expect(rail.laneCount).toBe(2)
    const [a, b, c] = rail.entries
    expect(a!.lanes.map(shape)).toEqual([
      [0, false, true, false, true],
      [1, false, true, false, true],
    ])
    expect(b!.lanes.map(shape)).toEqual([
      [0, true, false, true, false],
      [1, true, true, false, false],
    ])
    expect(c!.lanes.map(shape)).toEqual([[1, true, false, true, false]])
  })

  it(`shares a lane along a chain, with an arrowhead at every blocked node`, () => {
    const rail = issueRail(
      [row(`a`), row(`b`), row(`c`)],
      [blocks(`a`, `b`), blocks(`b`, `c`)],
      [issue(`a`), issue(`b`), issue(`c`)]
    )
    expect(rail.laneCount).toBe(1)
    const [, b] = rail.entries
    expect(b!.node).toBe(`blocked`)
    expect(b!.lanes.map(shape)).toEqual([[0, true, true, true, true]])
    expect(b!.lanes[0]!.edges.map((edge) => edge.key)).toEqual([`a:b`, `b:c`])
  })

  it(`keeps a node for an edge whose other end is not in the list`, () => {
    const rail = issueRail(
      [row(`b`)],
      [blocks(`a`, `b`), blocks(`b`, `z`)],
      [issue(`a`), issue(`b`), issue(`z`)]
    )
    expect(rail.laneCount).toBe(0)
    expect(rail.hasNodes).toBe(true)
    expect(railWidth(rail)).toBe(RAIL_NODE_WIDTH)
    expect(rail.entries[0]!.node).toBe(`blocked`)
    expect(rail.entries[0]!.counts).toEqual({ blockedBy: 1, blocking: 1 })
    expect(rail.entries[0]!.lanes).toEqual([])
  })

  it(`drops a closed end, like the counts badge`, () => {
    const rail = issueRail(
      [row(`a`), row(`b`)],
      [blocks(`a`, `b`)],
      [issue(`a`, `done`), issue(`b`)]
    )
    expect(rail.hasNodes).toBe(false)
  })

  it(`marks a blocking cycle on every edge of it`, () => {
    const rail = issueRail(
      [row(`a`), row(`b`), row(`c`)],
      [blocks(`a`, `b`), blocks(`b`, `a`), blocks(`b`, `c`)],
      [issue(`a`), issue(`b`), issue(`c`)]
    )
    const [a, b, c] = rail.entries
    expect(a!.lanes.map((lane) => lane.cycle)).toEqual([true, true])
    expect(b!.lanes.map((lane) => lane.cycle)).toEqual([true, true])
    // b → c is not on the cycle.
    expect(c!.lanes.map((lane) => lane.cycle)).toEqual([false])
  })

  it(`lays the lanes out beside the node column`, () => {
    expect(railLaneX(0)).toBe(RAIL_NODE_WIDTH + RAIL_LANE_PITCH / 2)
    expect(railLaneX(2)).toBe(RAIL_NODE_WIDTH + RAIL_LANE_PITCH * 2.5)
    expect(railEdgeLabel(`EXP-1`, `EXP-2`)).toBe(`EXP-1 blocks EXP-2`)
  })
})
