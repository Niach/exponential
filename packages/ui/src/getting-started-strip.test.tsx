import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import spec from "@exp/domain-contract/fixtures/getting-started.json"

import { GettingStartedStrip, gettingStartedCardTestId } from "./getting-started-strip"

// EXP-1270: the block's face draws the model's entries as they come, in the
// fixture's words: the title, the progress, the Docs link, one card per
// entry (check when done, the 1-based number otherwise, NEXT highlighted).

const CASE = spec.cases[1]!

describe(`GettingStartedStrip`, () => {
  it(`draws the header and one card per entry, in order`, () => {
    render(
      <GettingStartedStrip
        entries={CASE.expected.entries}
        done={CASE.expected.done}
        total={CASE.expected.total}
        progress={CASE.expected.progress}
      />
    )
    expect(screen.getByTestId(spec.testIds.block)).toBeTruthy()
    expect(screen.getByText(spec.title)).toBeTruthy()
    expect(screen.getByText(CASE.expected.progress)).toBeTruthy()
    expect(screen.getByTestId(spec.testIds.docs).getAttribute(`href`)).toBe(spec.docs.url)
    const cards = document.querySelectorAll(`[data-slot=getting-started-card]`)
    expect([...cards].map((card) => card.getAttribute(`data-testid`))).toEqual(
      CASE.expected.entries.map((entry) => gettingStartedCardTestId(entry.key))
    )
  })

  it(`checks done cards, numbers the rest, highlights the next one`, () => {
    render(
      <GettingStartedStrip
        entries={CASE.expected.entries}
        done={CASE.expected.done}
        total={CASE.expected.total}
        progress={CASE.expected.progress}
      />
    )
    const board = screen.getByTestId(gettingStartedCardTestId(`board`))
    expect(board.getAttribute(`data-done`)).toBe(`true`)
    expect(board.querySelector(`[aria-label=Done]`)).toBeTruthy()
    const invite = screen.getByTestId(gettingStartedCardTestId(`invite`))
    expect(invite.getAttribute(`data-next`)).toBe(`true`)
    expect(invite.textContent).toContain(`5`)
    expect(invite.textContent).toContain(`Invite your team`)
    expect(document.querySelectorAll(`[data-next=true]`)).toHaveLength(1)
  })

  it(`a click hands the card's key back, done cards included`, () => {
    const onSelect = vi.fn()
    render(
      <GettingStartedStrip
        entries={CASE.expected.entries}
        done={CASE.expected.done}
        total={CASE.expected.total}
        progress={CASE.expected.progress}
        onSelect={onSelect}
      />
    )
    fireEvent.click(screen.getByTestId(gettingStartedCardTestId(`github`)))
    fireEvent.click(screen.getByTestId(gettingStartedCardTestId(`widget`)))
    expect(onSelect.mock.calls).toEqual([[`github`], [`widget`]])
  })
})
