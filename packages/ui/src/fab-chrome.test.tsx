import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { FAB_CHROME_CLASS, FAB_CIRCLE_CLASS, FabButton } from "./fab-chrome"
import { MOBILE_WORK_CIRCLE_CLASS } from "./mobile-work-bar"

// EXP-962: the 52px floating circle is ONE button now.

describe(`FabButton`, () => {
  it(`is a 52px circle on the floating chrome with a secondary glyph`, () => {
    const onClick = vi.fn()
    render(
      <FabButton aria-label="Start coding" onClick={onClick}>
        <svg />
      </FabButton>
    )
    const button = screen.getByRole(`button`, { name: `Start coding` })
    expect(button.getAttribute(`type`)).toBe(`button`)
    expect(button.getAttribute(`data-slot`)).toBe(`fab-button`)
    expect(button.className).toContain(`size-[52px]`)
    expect(button.className).toContain(`rounded-full`)
    for (const cls of FAB_CHROME_CLASS.split(` `)) expect(button.className).toContain(cls)
    expect(button.className).toContain(`text-foreground/70`)
    expect(button.getAttribute(`data-emphasis`)).toBe(`secondary`)
    fireEvent.click(button)
    expect(onClick).toHaveBeenCalledTimes(1)
  })

  it(`goes full white for the primary emphasis`, () => {
    render(
      <FabButton aria-label="New issue" emphasis="primary">
        <svg />
      </FabButton>
    )
    const button = screen.getByRole(`button`)
    expect(button.className).toContain(`text-foreground`)
    expect(button.className).not.toContain(`text-foreground/70`)
    expect(button.getAttribute(`data-emphasis`)).toBe(`primary`)
  })

  it(`renders as its child link when asked`, () => {
    render(
      <FabButton asChild className="relative">
        <a href="/agent" aria-label="Start chat">
          <svg />
        </a>
      </FabButton>
    )
    const link = screen.getByRole(`link`, { name: `Start chat` })
    expect(link.getAttribute(`data-slot`)).toBe(`fab-button`)
    expect(link.className).toContain(`size-[52px]`)
    expect(link.className).toContain(`relative`)
    expect(link.getAttribute(`type`)).toBeNull()
  })

  it(`is the same circle the work bar's non-button slots wear`, () => {
    expect(MOBILE_WORK_CIRCLE_CLASS).toBe(FAB_CIRCLE_CLASS)
  })
})
