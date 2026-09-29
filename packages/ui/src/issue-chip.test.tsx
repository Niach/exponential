import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { IssueChip } from "./issue-chip"
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
