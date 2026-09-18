import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { EmptyCta, EmptyState, ListEmpty } from "./empty-state"
import { conceptIcon } from "./icons.generated"

// EXP-962: three empties — the page one, the in-list line and the dashed
// call to action that starts the list.

describe(`EmptyState`, () => {
  it(`teaches with a disc, a title and a sentence`, () => {
    const { container } = render(
      <EmptyState
        icon={conceptIcon(`nav-inbox`)}
        title="Inbox zero"
        description="Notifications land here."
      />
    )
    expect(container.querySelector(`[data-slot="icon-disc"]`)).not.toBeNull()
    expect(screen.getByRole(`heading`).textContent).toBe(`Inbox zero`)
    expect(container.textContent).toContain(`Notifications land here.`)
  })
})

describe(`ListEmpty`, () => {
  it(`is one muted centred line`, () => {
    const { container } = render(<ListEmpty>No emoji found</ListEmpty>)
    const line = container.querySelector(`[data-slot="list-empty"]`)!
    expect(line.textContent).toBe(`No emoji found`)
    expect(line.className).toContain(`text-muted-foreground`)
    expect(line.className).toContain(`text-center`)
  })
})

describe(`EmptyCta`, () => {
  it(`is a dashed full-width button carrying the glyph, the title and the nudge`, () => {
    const onClick = vi.fn()
    render(
      <EmptyCta
        icon={conceptIcon(`action-create`)}
        title="No custom actions yet"
        description="Describe one and your agent will build it."
        onClick={onClick}
      />
    )
    const button = screen.getByRole(`button`)
    expect(button.getAttribute(`type`)).toBe(`button`)
    expect(button.getAttribute(`data-slot`)).toBe(`empty-cta`)
    expect(button.className).toContain(`border-dashed`)
    expect(button.className).toContain(`w-full`)
    expect(button.textContent).toContain(`No custom actions yet`)
    expect(button.textContent).toContain(`Describe one and your agent will build it.`)
    expect(button.querySelector(`svg`)).not.toBeNull()
    fireEvent.click(button)
    expect(onClick).toHaveBeenCalledTimes(1)
  })
})
