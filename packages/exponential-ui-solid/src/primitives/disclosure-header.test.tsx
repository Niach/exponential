import { fireEvent, render, screen } from "@solidjs/testing-library"
import { createSignal } from "solid-js"
import { describe, expect, it, vi } from "vitest"

import { DisclosureHeader } from "./disclosure-header"

// EXP-962: the ONE fold toggle (React's disclosure-header.test.tsx).

const chevron = (container: HTMLElement) => container.querySelector(`[data-slot="disclosure-chevron"]`) as SVGElement

describe(`DisclosureHeader`, () => {
  it(`is a button stating its fold that toggles on click`, () => {
    const onToggle = vi.fn()
    render(() => (
      <DisclosureHeader open={false} onToggle={onToggle}>
        Read 3 files
      </DisclosureHeader>
    ))
    const button = screen.getByRole(`button`, { expanded: false })
    expect(button.getAttribute(`type`)).toBe(`button`)
    expect(button.textContent).toContain(`Read 3 files`)
    fireEvent.click(button)
    expect(onToggle).toHaveBeenCalledTimes(1)
  })

  it(`points the chevron right when folded and down when open`, () => {
    const [open, setOpen] = createSignal(false)
    const { container } = render(() => (
      <DisclosureHeader open={open()} onToggle={vi.fn()}>
        Lane
      </DisclosureHeader>
    ))
    expect(chevron(container).getAttribute(`class`)).toContain(`chevron-right`)
    setOpen(true)
    expect(chevron(container).getAttribute(`class`)).toContain(`chevron-down`)
    expect(screen.getByRole(`button`).getAttribute(`aria-expanded`)).toBe(`true`)
  })

  it(`leads with the chevron by default and parks it at the far edge when trailing`, () => {
    const [side, setSide] = createSignal<`leading` | `trailing` | undefined>(undefined)
    const { container } = render(() => (
      <DisclosureHeader open={false} onToggle={vi.fn()} chevron={side()}>
        <span>Label</span>
      </DisclosureHeader>
    ))
    let button = screen.getByRole(`button`)
    expect(button.getAttribute(`data-chevron`)).toBe(`leading`)
    expect(button.firstElementChild).toBe(chevron(container))
    setSide(`trailing`)
    button = screen.getByRole(`button`)
    expect(button.getAttribute(`data-chevron`)).toBe(`trailing`)
    expect(button.lastElementChild).toBe(chevron(container))
    // EXP-1191: `ml-auto` pushes it to the edge.
    expect(chevron(container).getAttribute(`class`)).toContain(`ml-auto`)
  })

  it(`is muted at rest and brightens on hover`, () => {
    render(() => (
      <DisclosureHeader open onToggle={vi.fn()} class="pl-0.5">
        Label
      </DisclosureHeader>
    ))
    const button = screen.getByRole(`button`)
    expect(button.className).toContain(`text-muted-foreground`)
    expect(button.className).toContain(`hover:text-foreground`)
    expect(button.className).toContain(`pl-0.5`)
  })
})
