import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ChatWaitingChips } from "./waiting-chips"
import { HeaderFoldContext } from "./header-collapse"
import { PrStatusPill } from "./pr-checks"
import { TooltipProvider } from "./tooltip"

// #EXP-1785 (waiting-cards.json CHIPS): the session screen header's waiting
// plans and questions of the tree.
const cards = [
  { id: `p1`, kind: `plan` as const, text: `## Plan\n\n1. Lift the partition` },
  { id: `q1`, kind: `question` as const, text: `Which layout should the header chips use on phones?` },
]

describe(`ChatWaitingChips`, () => {
  it(`wide: one chip per waiting plan and question; a click selects it`, () => {
    const onSelect = vi.fn()
    render(
      <TooltipProvider>
        <ChatWaitingChips cards={cards} phone={false} onSelect={onSelect} />
      </TooltipProvider>
    )
    const chips = screen.getAllByTestId(`waiting-chip`)
    expect(chips.map((chip) => chip.textContent)).toEqual([`Plan`, `Which layout should the…`])
    fireEvent.click(chips[1]!)
    expect(onSelect).toHaveBeenCalledWith(`q1`)
    expect(screen.queryByTestId(`waiting-button`)).toBeNull()
  })

  it(`phones: ONE button with the count; nothing waiting = nothing`, () => {
    const { rerender } = render(
      <TooltipProvider>
        <ChatWaitingChips cards={cards} phone onSelect={() => {}} />
      </TooltipProvider>
    )
    expect(screen.getByTestId(`waiting-button`).getAttribute(`aria-label`)).toBe(`2 waiting`)
    expect(screen.queryByTestId(`waiting-chip`)).toBeNull()
    rerender(
      <TooltipProvider>
        <ChatWaitingChips cards={[]} phone onSelect={() => {}} />
      </TooltipProvider>
    )
    expect(screen.queryByTestId(`waiting-button`)).toBeNull()
  })

  it(`#EXP-2183: a folded header keeps the icons, the text only read out`, () => {
    render(
      <TooltipProvider>
        <HeaderFoldContext.Provider value={new Set([`status`, `primary`] as const)}>
          <ChatWaitingChips cards={cards} phone onSelect={() => {}} />
          <PrStatusPill view={{ label: `Checks running`, glyph: `spinner`, tone: `muted`, primary: false, tap: null } as never} folded />
        </HeaderFoldContext.Provider>
      </TooltipProvider>
    )
    const button = screen.getByTestId(`waiting-button`)
    expect(button.getAttribute(`aria-label`)).toBe(`2 waiting`)
    expect(button.querySelector(`[data-header-fold-label]`)?.className).toContain(`sr-only`)
    const pill = document.querySelector(`[data-header-fold=primary]`)!
    expect(pill.getAttribute(`aria-label`)).toBe(`Checks running`)
    expect(pill.querySelector(`[data-header-fold-label]`)?.className).toContain(`sr-only`)
  })
})
