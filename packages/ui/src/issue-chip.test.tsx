import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { IssueChip, tabChipMinWidth } from "./issue-chip"
import { categoryStatusIcon } from "./status-icons"

// SLOP-15: the chip's SMALL mode — the same box, glyph · identifier, no
// title. It is what the hover graph's nodes, the phone header's badge and
// the badge overlay's rows draw, so a surface that names many issues stays a
// glance. The title still reaches the reader through the tooltip.

const status = { icon: categoryStatusIcon(`backlog`, 0, 1), colorClass: `text-muted-foreground` }

describe(`IssueChip size (SLOP-15)`, () => {
  it(`draws glyph, identifier and title by default`, () => {
    render(<IssueChip identifier="EXP-1" title="The title" status={status} testId="chip" />)
    const chip = screen.getByTestId(`chip`)
    expect(chip.getAttribute(`data-size`)).toBe(`md`)
    expect(chip.textContent).toBe(`EXP-1The title`)
  })

  it(`drops the title in the small mode and keeps it in the tooltip`, () => {
    render(
      <IssueChip identifier="EXP-1" title="The title" status={status} size="sm" testId="chip" />
    )
    const chip = screen.getByTestId(`chip`)
    expect(chip.getAttribute(`data-size`)).toBe(`sm`)
    expect(chip.textContent).toBe(`EXP-1`)
    expect(chip.getAttribute(`title`)).toBe(`EXP-1 · The title`)
    // The same box: the paint class and the glyph are untouched.
    expect(chip.classList.contains(`issue-chip`)).toBe(true)
    expect(chip.querySelector(`svg`)).toBeTruthy()
  })

  it(`keeps its open affordance and its ✕ in the small mode`, () => {
    render(
      <IssueChip
        identifier="EXP-1"
        title="The title"
        status={status}
        size="sm"
        onClick={() => {}}
        onRemove={() => {}}
        testId="chip"
      />
    )
    expect(screen.getByRole(`button`, { name: `Open EXP-1` })).toBeTruthy()
    expect(screen.getByRole(`button`, { name: `Remove EXP-1` })).toBeTruthy()
  })
})

// EXP-1316: the TAB size = the thread strip's tab box, off the unlayered
// `.issue-chip` paint so the active wash wins.
describe(`IssueChip size="tab" (EXP-1316)`, () => {
  it(`wears the strip tab box, glyph · identifier · title`, () => {
    const { rerender } = render(
      <IssueChip identifier="EXP-1" title="The title" status={status} size="tab" testId="chip" />
    )
    const chip = screen.getByTestId(`chip`)
    expect(chip.className).not.toContain(`issue-chip`)
    expect(chip.className).toContain(`h-8`)
    expect(chip.className).toContain(`border-glass-stroke`)
    expect(chip.textContent).toBe(`EXP-1The title`)
    rerender(
      <IssueChip identifier="EXP-1" title="The title" status={status} size="tab" active testId="chip" />
    )
    expect(screen.getByTestId(`chip`).className).toContain(`bg-glass-active`)
    expect(screen.getByTestId(`chip`).getAttribute(`data-active`)).toBe(`true`)
  })
})

describe(`IssueChip size="pager" (EXP-1316)`, () => {
  it(`is the tab box without the title`, () => {
    render(<IssueChip identifier="EXP-2" title="Hidden" status={status} size="pager" testId="chip" />)
    const chip = screen.getByTestId(`chip`)
    expect(chip.className).toContain(`h-8`)
    expect(chip.textContent).toBe(`EXP-2`)
  })
})

// EXP-1316 (the IDE's strip rule): on md+ a tab chip shrinks first, its
// title at most 8.75rem with an ellipsis, the chip floored by
// `tabChipMinWidth` (a short title never shrinks).
describe(`IssueChip size="tab" shrinking`, () => {
  it(`may shrink down to its title floor`, () => {
    const { rerender } = render(
      <IssueChip
        identifier="EXP-3"
        title="Syntax highlighting ships every language"
        status={status}
        size="tab"
        testId="chip"
      />
    )
    const chip = screen.getByTestId(`chip`)
    expect(chip.className).toContain(`shrink-[4]`)
    expect(chip.className).not.toContain(`shrink-0`)
    // frame 2.625rem + title floor 2.5rem + 5 mono characters × 0.45rem
    expect(tabChipMinWidth(`EXP-3`, `Syntax highlighting ships every language`)).toBe(
      `calc(7.375rem + 2px)`
    )
    expect(chip.style.minWidth).toContain(`7.375rem`)
    const title = chip.querySelector(`[data-slot="issue-chip-title"]`)!
    expect(title.className).toContain(`max-w-[8.75rem]`)
    expect(title.className).toContain(`truncate`)
    // A short title never shrinks.
    rerender(<IssueChip identifier="EXP-3" title="Short" status={status} size="tab" testId="chip" />)
    expect(screen.getByTestId(`chip`).style.minWidth).toBe(`max-content`)
  })
})
