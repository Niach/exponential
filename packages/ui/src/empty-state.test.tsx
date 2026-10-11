import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { EmptyCta, EmptyState, ListEmpty } from "./empty-state"
import { conceptIcon } from "./icons.generated"

// EXP-962 / EXP-1274: ONE empty state in two sizes (page, list), the in-list
// line without a glyph and the dashed call to action that starts the list.

describe(`EmptyState`, () => {
  it(`page: teaches with a disc, a title and a sentence`, () => {
    const { container } = render(
      <EmptyState
        icon={conceptIcon(`nav-inbox`)}
        title="Inbox zero"
        description="Notifications land here."
      />
    )
    const block = container.querySelector(`[data-slot="empty-state"]`)!
    expect(block.getAttribute(`data-size`)).toBe(`page`)
    expect(block.getAttribute(`role`)).toBe(`status`)
    expect(container.querySelector(`[data-slot="icon-disc"]`)).not.toBeNull()
    expect(screen.getByRole(`heading`).textContent).toBe(`Inbox zero`)
    expect(container.textContent).toContain(`Notifications land here.`)
  })

  it(`page: the description and the disc are optional`, () => {
    const { container } = render(<EmptyState title="No boards yet" />)
    expect(container.querySelector(`[data-slot="icon-disc"]`)).toBeNull()
    expect(container.querySelector(`p`)).toBeNull()
    expect(screen.getByRole(`heading`).textContent).toBe(`No boards yet`)
  })

  it(`list: one muted centred line under a bare 20px glyph, no heading`, () => {
    const { container } = render(
      <EmptyState size="list" icon={conceptIcon(`settings-mcp`)} title="No MCP servers yet" />
    )
    const block = container.querySelector(`[data-slot="empty-state"]`)!
    expect(block.getAttribute(`data-size`)).toBe(`list`)
    expect(block.className).toContain(`text-muted-foreground`)
    expect(block.className).toContain(`text-center`)
    expect(block.className).toContain(`py-6`)
    expect(container.querySelector(`[data-slot="icon-disc"]`)).toBeNull()
    expect(block.querySelector(`svg`)!.getAttribute(`class`)).toContain(`size-5`)
    expect(screen.queryByRole(`heading`)).toBeNull()
    expect(block.textContent).toBe(`No MCP servers yet`)
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
