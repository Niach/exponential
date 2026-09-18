import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { DisclosureHeader } from "./disclosure-header"

// EXP-962: the ONE fold toggle the steer feed and the workflow card open
// their rows with.

const chevron = (container: HTMLElement) =>
  container.querySelector(`[data-slot="disclosure-chevron"]`) as SVGElement

describe(`DisclosureHeader`, () => {
  it(`is a button stating its fold that toggles on click`, () => {
    const onToggle = vi.fn()
    render(
      <DisclosureHeader open={false} onToggle={onToggle}>
        Read 3 files
      </DisclosureHeader>
    )
    const button = screen.getByRole(`button`, { expanded: false })
    expect(button.getAttribute(`type`)).toBe(`button`)
    expect(button.textContent).toContain(`Read 3 files`)
    fireEvent.click(button)
    expect(onToggle).toHaveBeenCalledTimes(1)
  })

  it(`points the chevron right when folded and down when open`, () => {
    const { container, rerender } = render(
      <DisclosureHeader open={false} onToggle={vi.fn()}>
        Lane
      </DisclosureHeader>
    )
    expect(chevron(container).getAttribute(`class`)).toContain(`chevron-right`)
    rerender(
      <DisclosureHeader open onToggle={vi.fn()}>
        Lane
      </DisclosureHeader>
    )
    expect(chevron(container).getAttribute(`class`)).toContain(`chevron-down`)
    expect(screen.getByRole(`button`).getAttribute(`aria-expanded`)).toBe(`true`)
  })

  it(`leads with the chevron by default and parks it at the far edge when trailing`, () => {
    const { container, rerender } = render(
      <DisclosureHeader open={false} onToggle={vi.fn()}>
        <span>Label</span>
      </DisclosureHeader>
    )
    let button = screen.getByRole(`button`)
    expect(button.getAttribute(`data-chevron`)).toBe(`leading`)
    expect(button.firstElementChild).toBe(chevron(container))
    rerender(
      <DisclosureHeader open={false} onToggle={vi.fn()} chevron="trailing">
        <span>Label</span>
      </DisclosureHeader>
    )
    button = screen.getByRole(`button`)
    expect(button.getAttribute(`data-chevron`)).toBe(`trailing`)
    expect(button.lastElementChild).toBe(chevron(container))
    // The spacer pushes it to the edge.
    expect(button.children[1]!.className).toContain(`flex-1`)
  })

  it(`is muted at rest and brightens on hover`, () => {
    render(
      <DisclosureHeader open onToggle={vi.fn()} className="pl-0.5">
        Label
      </DisclosureHeader>
    )
    const button = screen.getByRole(`button`)
    expect(button.className).toContain(`text-muted-foreground`)
    expect(button.className).toContain(`hover:text-foreground`)
    expect(button.className).toContain(`pl-0.5`)
  })
})
