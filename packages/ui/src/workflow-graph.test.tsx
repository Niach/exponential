import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { IssueChipStack } from "./issue-chip"
import { WorkflowGraphView, type WorkflowGraphNode } from "./workflow-graph"
import type { WaveGraphEdge } from "./wave-graph"

// EXP-1033: the workflow graph's drawing half. A node is the ISSUE CHIP box —
// mono identifier, truncated title, the ONE caption trailing INSIDE it — laid
// out on the wave grid, and the whole picture FILLS its column instead of
// hiding behind an inner scrollbar.
//
// EXP-1014: the glyph SLOT has one rule — the caller's `glyph` (a live dot, a
// workflow state) if there is one, else the ISSUE's own resolved status, else
// nothing at all. And the fill rule is measured here rather than assumed:
// jsdom reports `clientWidth` 0 on everything, so the width the scale is
// computed from is stubbed for the test that is about it.

/** The grid's metrics, mirrored from the module: one node is 200×28, waves are
 *  48 apart, and the picture keeps 8px of air on every side. */
const NODE_W = 200
const WAVE_GAP = 48
const PAD = 8

/** Pin `clientWidth` for the duration of one render — the ONLY input the fill
 *  rule has, and 0 in jsdom (which silently pins `scale` to 1). */
function withContainerWidth<T>(width: number, run: () => T): T {
  const original = Object.getOwnPropertyDescriptor(
    HTMLElement.prototype,
    `clientWidth`
  )
  Object.defineProperty(HTMLElement.prototype, `clientWidth`, {
    configurable: true,
    get: () => width,
  })
  try {
    return run()
  } finally {
    if (original) {
      Object.defineProperty(HTMLElement.prototype, `clientWidth`, original)
    } else {
      delete (HTMLElement.prototype as unknown as Record<string, unknown>)
        .clientWidth
    }
  }
}

/** The chip's glyph slot — always drawn, empty when nothing resolves into
 *  it (an unsynced issue). */
const glyphSlot = (id: string) =>
  screen.queryByTestId(`workflow-node-${id}-glyph`)

/** The scaled picture's own box, inside the measured container. */
const picture = () =>
  screen.getByTestId(`workflow-graph`).firstElementChild as HTMLElement

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

  // The FILL rule, measured: two waves are 200 + 48 + 200 = 448 wide plus
  // 8px of air on each side, so the picture is naturally 464 × 44.
  const naturalWidth = NODE_W * 2 + WAVE_GAP + PAD * 2
  const naturalHeight = 28 + PAD * 2

  it(`scales the whole picture DOWN to a narrower container`, () => {
    withContainerWidth(naturalWidth / 2, () => {
      view([node(), node({ id: `n2`, wave: 1 })])
      const graph = screen.getByTestId(`workflow-graph`)
      // The measurement really happened: the stub is what `scale` read.
      expect(graph.clientWidth).toBe(naturalWidth / 2)
      // ONE transform on the picture, which keeps its natural width, and the
      // container takes the SCALED height — nothing is clipped, no inner
      // scroller appears.
      expect(picture().style.transform).toBe(`scale(0.5)`)
      expect(picture().style.width).toBe(`${naturalWidth}px`)
      expect(graph.style.height).toBe(`${naturalHeight / 2}px`)
      expect(graph.className).not.toContain(`overflow`)
    })
  })

  it(`measures a container that mounts AFTER the nodes arrive (a cold load)`, () => {
    // The page reads the workflow row one shape before its nodes, so the view
    // first renders NOTHING and the measured div mounts on a later commit. The
    // measurement has to happen then — not on a first-render effect that saw
    // no element and never ran again.
    withContainerWidth(naturalWidth / 2, () => {
      const { rerender } = view([])
      expect(screen.queryByTestId(`workflow-graph`)).toBeNull()
      rerender(
        <WorkflowGraphView nodes={[node(), node({ id: `n2`, wave: 1 })]} edges={[]} />
      )
      const graph = screen.getByTestId(`workflow-graph`)
      expect(graph.clientWidth).toBe(naturalWidth / 2)
      expect(picture().style.transform).toBe(`scale(0.5)`)
      expect(graph.style.height).toBe(`${naturalHeight / 2}px`)
    })
  })

  it(`never scales UP: a small graph keeps its true size in a wide column`, () => {
    withContainerWidth(naturalWidth * 4, () => {
      view([node(), node({ id: `n2`, wave: 1 })])
      const graph = screen.getByTestId(`workflow-graph`)
      expect(graph.clientWidth).toBe(naturalWidth * 4)
      // No transform at all, and the container takes the picture's TRUE
      // height — a graph narrower than its column is not blown up.
      expect(picture().style.transform).toBe(``)
      expect(graph.style.height).toBe(`${naturalHeight}px`)
    })
  })

  // EXP-1014: the ONE glyph-slot rule.
  it(`falls back to the issue's status glyph, and empties the slot without one`, () => {
    view([
      node({ id: `n1`, status: { icon: `circle-dashed`, colorClass: `text-yellow-500` } }),
      node({
        id: `n2`,
        wave: 1,
        glyph: <span data-testid="live-dot" />,
        status: { icon: `circle-dashed`, colorClass: `text-yellow-500` },
      }),
      node({ id: `n3`, wave: 2 }),
    ])
    // No `glyph`: the issue's own status paints the slot.
    expect(glyphSlot(`n1`)!.querySelector(`svg`)!.getAttribute(`class`)).toContain(
      `text-yellow-500`
    )
    // A `glyph` always wins — a live run says how it is going, not the issue —
    // and the slot paints it in the node's TONE (the caption's colour).
    expect(glyphSlot(`n2`)!.querySelector(`svg`)).toBeNull()
    expect(screen.getByTestId(`live-dot`)).toBeTruthy()
    expect(glyphSlot(`n2`)!.className).toContain(`text-emerald-500`)
    // The issue's status paints itself, whatever the tone around it.
    expect(glyphSlot(`n1`)!.querySelector(`svg`)!.getAttribute(`class`)).toContain(
      `text-yellow-500`
    )
    // Neither: an unsynced issue leaves the slot EMPTY — it is still there,
    // so every chip's identifier starts at the same place.
    expect(glyphSlot(`n3`)!.childElementCount).toBe(0)
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
