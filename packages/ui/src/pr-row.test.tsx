import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { contract } from "@exp/domain-contract"
import listItem from "@exp/domain-contract/fixtures/list-item.json"
import { PrList, PrNode, PrRow, StackRail } from "./pr-row"

// EXP-1248: THE PR row, its ring lead, PR trees and the stack rail.

describe(`PrNode`, () => {
  it.each(listItem.prNodes)(`$name`, ({ state, ring, filled }) => {
    const { container } = render(<PrNode state={state as `open` | `current` | `base`} />)
    const node = container.querySelector(`[data-slot="pr-node"]`)!
    expect(node.getAttribute(`data-state`)).toBe(state)
    const ringEl = node.firstElementChild!
    expect(ringEl.className).toContain(ring === `emerald` ? `border-emerald-500` : `border-muted-foreground`)
    expect(node.querySelector(`[data-slot="pr-node-fill"]`) != null).toBe(filled)
  })
})

describe(`PrRow`, () => {
  it(`is one line: node, mono identifier, title, the quiet word`, () => {
    render(<PrRow data-testid="row" identifier="VAPP-100" title="SwiftUI parity" word="stack" />)
    const row = screen.getByTestId(`row`)
    expect(row.textContent).toBe(`VAPP-100SwiftUI paritystack`)
    expect(row.className).toContain(`h-10`)
    expect(row.className).toContain(`md:h-9`)
    expect(listItem.geometry.prRow).toBe(36)
    expect(listItem.geometry.prRowPhone).toBe(40)
    expect(row.querySelector(`button`)).toBeNull()
  })

  it(`indents a tree child by depth`, () => {
    render(<PrRow data-testid="row" identifier="EXP-2" title="child" depth={1} />)
    expect(screen.getByTestId(`row`).style.paddingLeft).toBe(`26px`)
  })
})

describe(`PrList`, () => {
  it(`nests a tree with guides on the children only`, () => {
    const { container } = render(
      <PrList
        rows={[
          { key: `a`, identifier: `EXP-1250`, title: `parent` },
          { key: `b`, identifier: `EXP-1252`, title: `child one`, depth: 1 },
          { key: `c`, identifier: `EXP-1253`, title: `child two`, depth: 1 },
        ]}
      />
    )
    expect(container.querySelectorAll(`[data-pr-row]`).length).toBe(3)
    expect(container.querySelectorAll(`[data-testid="tree-guides"]`).length).toBe(2)
  })
})

describe(`StackRail`, () => {
  const members = [
    { key: `100`, identifier: `VAPP-100`, title: `top`, current: true },
    { key: `98`, identifier: `VAPP-98`, title: `middle` },
    { key: `88`, identifier: `VAPP-88`, title: `bottom` },
  ]

  it(`draws the stack top-first down to the base branch on one rail`, () => {
    const { container } = render(<StackRail members={members} baseBranch="master" word="stack" />)
    const rows = Array.from(container.querySelectorAll(`[data-pr-row]`))
    expect(rows.map((row) => row.getAttribute(`data-pr-row`))).toEqual([`current`, `open`, `open`, `base`])
    expect(rows[3]!.textContent).toBe(`master`)
    // The rail: nothing above the top node, nothing below the base node.
    expect(rows[0]!.querySelector(`[data-slot="pr-rail-above"]`)).toBeNull()
    expect(rows[0]!.querySelector(`[data-slot="pr-rail-below"]`)).not.toBeNull()
    expect(rows[3]!.querySelector(`[data-slot="pr-rail-above"]`)).not.toBeNull()
    expect(rows[3]!.querySelector(`[data-slot="pr-rail-below"]`)).toBeNull()
    // The current member wears the active wash; only the top row has the word.
    expect(rows[0]!.className).toContain(`bg-glass-active`)
    expect(container.querySelectorAll(`[data-slot="pr-row-word"]`).length).toBe(1)
  })

  it(`offers Merge through here on the hovered row only`, () => {
    const onMerge = vi.fn()
    const onOpen = vi.fn()
    const { container } = render(
      <StackRail members={members} baseBranch="master" onMergeThrough={onMerge} onOpen={onOpen} />
    )
    expect(screen.queryByText(contract.diffUi.mergeThrough)).toBeNull()
    const rows = container.querySelectorAll(`[data-pr-row]`)
    fireEvent.mouseEnter(rows[1]!)
    const ghosts = screen.getAllByText(contract.diffUi.mergeThrough)
    expect(ghosts.length).toBe(1)
    expect(rows[1]!.contains(ghosts[0]!)).toBe(true)
    fireEvent.click(ghosts[0]!)
    expect(onMerge).toHaveBeenCalledWith(members[1])
    expect(onOpen).not.toHaveBeenCalled()
    fireEvent.mouseLeave(rows[1]!)
    expect(screen.queryByText(contract.diffUi.mergeThrough)).toBeNull()
  })

  it(`never offers the ghost without a handler, nor on the base row`, () => {
    const { container } = render(<StackRail members={members} baseBranch="master" defaultHoveredKey="98" />)
    expect(screen.queryByText(contract.diffUi.mergeThrough)).toBeNull()
    const base = container.querySelector(`[data-pr-row="base"]`)!
    fireEvent.mouseEnter(base)
    expect(screen.queryByText(contract.diffUi.mergeThrough)).toBeNull()
  })

  it(`opens a member on click`, () => {
    const onOpen = vi.fn()
    const { container } = render(<StackRail members={members} baseBranch="master" onOpen={onOpen} />)
    fireEvent.click(container.querySelectorAll(`[data-pr-row]`)[2]!)
    expect(onOpen).toHaveBeenCalledWith(members[2])
  })
})
