import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { IssueRailGap, IssueRailLayer, RAIL_ROOT_ATTR, railCurve } from "./issue-rail"
import { issueRail, railLaneX, railWidth } from "@/lib/issue-rail"
import type { GraphIssue, GraphRelation } from "@/lib/issue-graph"

// EXP-998: the rail's painter. The RULE is `lib/issue-rail.test.ts`; what is
// checked here is that a row paints its lanes + node, a gap its lanes only,
// and that hovering a segment lights the whole edge up across rows.

vi.mock(`@/components/issue-blocks-badge`, () => ({
  IssueBlocksPopover: ({ trigger }: { trigger: React.ReactElement }) => trigger,
}))

const issues: GraphIssue[] = [
  { id: `a`, identifier: `EXP-1`, status: `in_progress` },
  { id: `b`, identifier: `EXP-2`, status: `backlog` },
]
const relations: GraphRelation[] = [
  { type: `blocks`, issueId: `a`, relatedIssueId: `b` },
]

function List() {
  const rail = issueRail(
    [{ kind: `row`, id: `a` }, { kind: `gap` }, { kind: `row`, id: `b` }],
    relations,
    issues
  )
  const width = railWidth(rail)
  return (
    <div {...{ [RAIL_ROOT_ATTR]: `` }}>
      <div className="relative" data-testid="row-a">
        <IssueRailLayer row={rail.entries[0]} width={width} issueId="a" teamId="t" />
      </div>
      <div className="relative" data-testid="gap">
        <IssueRailGap row={rail.entries[1]} width={width} />
      </div>
      <div className="relative" data-testid="row-b">
        <IssueRailLayer row={rail.entries[2]} width={width} issueId="b" teamId="t" />
      </div>
    </div>
  )
}

describe(`IssueRailLayer`, () => {
  it(`draws nothing for a row with no relation`, () => {
    const rail = issueRail([{ kind: `row`, id: `a` }], [], issues)
    const { container } = render(
      <IssueRailLayer row={rail.entries[0]} width={railWidth(rail)} issueId="a" teamId="t" />
    )
    expect(container.firstChild).toBeNull()
  })

  it(`paints the blocker's dot, the gap's pass-through and the blocked row's ring + arrow`, () => {
    const { getByTestId } = render(<List />)
    const a = getByTestId(`row-a`)
    expect(a.querySelector(`[data-testid="issue-rail-node"]`)!.getAttribute(`data-kind`)).toBe(
      `blocking`
    )
    expect(a.querySelector(`[data-testid="issue-rail-node"]`)!.getAttribute(`aria-label`)).toBe(
      `Blocking 1`
    )
    // The blocker's slice: one curve out of the node to the lane at the
    // row's bottom edge, nothing above.
    const aLane = a.querySelector(`path[data-testid="issue-rail-lane"]`)!
    expect(aLane.getAttribute(`d`)).toBe(railCurve(railLaneX(0), 20))
    expect(a.querySelector(`[data-testid="issue-rail-arrow"]`)).toBeNull()

    const gap = getByTestId(`gap`)
    expect(gap.querySelector(`[data-testid="issue-rail-node"]`)).toBeNull()
    expect(gap.querySelector(`path[data-testid="issue-rail-lane"]`)!.getAttribute(`d`)).toBe(
      `M ${railLaneX(0)} -1000 V 1000`
    )

    const b = getByTestId(`row-b`)
    const node = b.querySelector(`[data-testid="issue-rail-node"]`)!
    expect(node.getAttribute(`data-kind`)).toBe(`blocked`)
    expect(node.getAttribute(`aria-label`)).toBe(`Blocked by 1`)
    expect(b.querySelector(`[data-testid="issue-rail-arrow"]`)).toBeTruthy()
    // …and the blocked row's: the curve in from the top edge.
    expect(b.querySelector(`path[data-testid="issue-rail-lane"]`)!.getAttribute(`d`)).toBe(
      railCurve(railLaneX(0), -20)
    )
    expect(b.querySelector(`title`)!.textContent).toBe(`EXP-1 blocks EXP-2`)
  })

  it(`lights the whole edge up while a segment or a node is hovered`, () => {
    const { container, getByTestId } = render(<List />)
    const segments = () => [...container.querySelectorAll(`[data-rail-edge~="a:b"]`)]
    expect(segments().length).toBeGreaterThan(2)
    expect(segments().every((node) => !node.hasAttribute(`data-hot`))).toBe(true)

    const group = getByTestId(`gap`).querySelector(`g`)!
    fireEvent.mouseEnter(group)
    expect(segments().every((node) => node.hasAttribute(`data-hot`))).toBe(true)
    fireEvent.mouseLeave(group)
    expect(segments().every((node) => !node.hasAttribute(`data-hot`))).toBe(true)

    const node = getByTestId(`row-b`).querySelector(`[data-testid="issue-rail-node"]`)!
    fireEvent.mouseEnter(node)
    expect(segments().every((node) => node.hasAttribute(`data-hot`))).toBe(true)
  })
})
