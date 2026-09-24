import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { IssueChipStack } from "./issue-chip"
import { WorkflowGraphView, type WorkflowGraphNode } from "./workflow-graph"
import type { WaveGraphEdge } from "./wave-graph"

// EXP-1033: the workflow graph's drawing half. A node is the ISSUE CHIP box —
// mono identifier, truncated title, the ONE caption trailing INSIDE it — laid
// out on the wave grid, and the whole picture FILLS its column instead of
// hiding behind an inner scrollbar.

const node = (over: Partial<WorkflowGraphNode> = {}): WorkflowGraphNode => ({
  id: `n1`,
  wave: 0,
  lane: 0,
  title: `EXP-1`,
  name: `Ship the thing`,
  caption: `Running`,
  tone: `success`,
  ...over,
})

const view = (
  nodes: WorkflowGraphNode[],
  edges: WaveGraphEdge[] = [],
  finalPr?: { caption: string; url?: string | null }
) => render(<WorkflowGraphView nodes={nodes} edges={edges} finalPr={finalPr} />)

describe(`WorkflowGraphView`, () => {
  it(`draws a node as the issue chip, caption inside it and nothing under it`, () => {
    view([node()])
    const card = screen.getByTestId(`workflow-node-n1-card`)
    // The shared box: `issue-chip` is the 6px rect every client names an
    // issue with (styles.css), not a circle of this screen's own.
    expect(card.className).toContain(`issue-chip`)
    expect(screen.getByTestId(`workflow-node-n1-title`).textContent).toBe(`EXP-1`)
    expect(screen.getByTestId(`workflow-node-n1-name`).textContent).toBe(
      `Ship the thing`
    )
    expect(screen.getByTestId(`workflow-node-n1-caption`).textContent).toBe(
      `Running`
    )
    // One box, one line: the caption is a CHILD of the chip.
    expect(card.contains(screen.getByTestId(`workflow-node-n1-caption`))).toBe(
      true
    )
  })

  it(`places nodes by wave and lane and names each edge by its style`, () => {
    view(
      [node(), node({ id: `n2`, wave: 1, lane: 2, caption: `Landed` })],
      [{ from: `n1`, to: `n2`, style: `landed` }]
    )
    const box = screen.getByTestId(`workflow-node-n2`)
    expect([box.getAttribute(`data-wave`), box.getAttribute(`data-lane`)]).toEqual(
      [`1`, `2`]
    )
    expect(
      screen.getByTestId(`workflow-graph-landed-edge`).getAttribute(`data-style`)
    ).toBe(`landed`)
  })

  it(`stacks a compound node and marks a proposed or cycling one`, () => {
    view([
      node({ id: `n1`, stacked: true, title: `EXP-1 +2` }),
      node({ id: `n2`, wave: 1, proposed: true, onCycle: true }),
    ])
    expect(screen.getByTestId(`workflow-node-n1-stack`)).toBeTruthy()
    expect(screen.queryByTestId(`workflow-node-n2-stack`)).toBeNull()
    const proposed = screen.getByTestId(`workflow-node-n2-card`)
    expect(proposed.getAttribute(`data-proposed`)).toBe(`true`)
    expect(proposed.getAttribute(`data-cycle`)).toBe(`true`)
    // `.issue-chip` is unlayered, so the two outlines are inline.
    expect(proposed.style.borderStyle).toBe(`dashed`)
  })

  it(`never scrolls inside itself: no overflow, no fixed height`, () => {
    view([node(), node({ id: `n2`, wave: 4 })])
    const graph = screen.getByTestId(`workflow-graph`)
    expect(graph.className).not.toContain(`overflow`)
    // The container takes the picture's own height; nothing is clipped.
    expect(graph.style.height).not.toBe(``)
  })

  it(`closes the graph with the final pull request, one wave past the last`, () => {
    view(
      [node(), node({ id: `n2`, wave: 1 })],
      [],
      { caption: `#7 · Open`, url: `https://example.test/pull/7` }
    )
    const chip = screen.getByTestId(`workflow-final-pr`)
    expect(chip.textContent).toBe(`Final pull request#7 · Open`)
    expect(
      screen.getByTestId(`workflow-final-pr-link`).getAttribute(`href`)
    ).toBe(`https://example.test/pull/7`)
    const box = screen.getByTestId(`workflow-node-final-pr`)
    expect([box.getAttribute(`data-wave`), box.getAttribute(`data-lane`)]).toEqual(
      [`2`, `0`]
    )
  })

  it(`draws nothing at all with no nodes`, () => {
    const { container } = view([])
    expect(container.firstChild).toBeNull()
  })
})

describe(`IssueChipStack`, () => {
  it(`puts two ghosts of the same box behind the front chip`, () => {
    const { container } = render(
      <IssueChipStack testId="stack" count={2}>
        <span className="issue-chip">EXP-1</span>
      </IssueChipStack>
    )
    const stack = screen.getByTestId(`stack`)
    const ghosts = [...stack.querySelectorAll(`[aria-hidden]`)]
    expect(ghosts).toHaveLength(2)
    // The SAME box, stepped up and to the right.
    for (const ghost of ghosts) expect(ghost.className).toContain(`issue-chip`)
    expect(ghosts.map((ghost) => (ghost as HTMLElement).style.top)).toEqual([
      `-6px`,
      `-3px`,
    ])
    expect(container.textContent).toBe(`EXP-1+2`)
  })

  it(`says no number when it was given none`, () => {
    const { container } = render(
      <IssueChipStack>
        <span className="issue-chip">EXP-1</span>
      </IssueChipStack>
    )
    expect(container.textContent).toBe(`EXP-1`)
  })
})
