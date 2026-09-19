import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { TreeGuides } from "./tree-guides-view"
import { treeGuideCentre, treeGuides } from "./tree-guides"

// EXP-965: the painter. The RULE is `tree-guides.test.ts`; what is checked
// here is the GAP BRIDGE — a spaced list would otherwise break every line at
// the gap, because a row can only paint inside itself.

const guideAt = (depths: number[], index: number) => treeGuides(depths)[index]!

const bridge = (container: HTMLElement) =>
  container.querySelector(`[data-testid="tree-guides-bridge"]`)

const xs = (node: Element) =>
  [...node.querySelectorAll(`line`)].map((line) => Number(line.getAttribute(`x1`)))

describe(`TreeGuides`, () => {
  it(`draws nothing for a root row`, () => {
    const { container } = render(
      <TreeGuides guide={guideAt([0, 0], 0)} gap={7} />
    )
    expect(container.firstChild).toBeNull()
  })

  it(`has no bridge layer in a gapless list`, () => {
    const { container } = render(<TreeGuides guide={guideAt([0, 1], 1)} />)
    expect(container.querySelector(`[data-testid="tree-guides"]`)).toBeTruthy()
    expect(bridge(container)).toBeNull()
  })

  it(`bridges the elbow and every pass-through, one gap above the row`, () => {
    // root ▸ a ▸ (a1, a2) ▸ b — row 2 elbows at level 1 and passes level 0.
    const { container } = render(
      <TreeGuides guide={guideAt([0, 1, 2, 2, 1], 2)} gap={7} />
    )
    const layer = bridge(container)!
    expect(layer.getAttribute(`height`)).toBe(`7`)
    expect((layer as HTMLElement).style.top).toBe(`-7px`)
    // Both levels the row starts at its own top edge: the ancestor's line and
    // the elbow's vertical.
    expect(xs(layer).sort()).toEqual(
      [treeGuideCentre(0), treeGuideCentre(1)].sort()
    )
  })

  it(`never bridges a tee — its lower half starts at the centre`, () => {
    // Row 1 of [0,1,1] tees, and elbows at level 0: exactly ONE bridged line.
    const { container } = render(
      <TreeGuides guide={guideAt([0, 1, 1], 1)} gap={7} />
    )
    expect(xs(bridge(container)!)).toEqual([treeGuideCentre(0)])
  })

  it(`follows the base offset the list nests from`, () => {
    const { container } = render(
      <TreeGuides guide={guideAt([0, 1], 1)} base={0} gap={6} />
    )
    expect(xs(bridge(container)!)).toEqual([treeGuideCentre(0, 0)])
  })
})
