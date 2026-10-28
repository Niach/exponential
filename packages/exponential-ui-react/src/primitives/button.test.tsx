import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Button } from "./button"

// EXP-962: the text button and the inline size the "Show more" folds and the
// session bands' links are made of.

describe(`Button`, () => {
  it(`text variant is muted words that brighten, no underline`, () => {
    render(
      <Button variant="text" size="inline">
        Show more
      </Button>
    )
    const button = screen.getByRole(`button`)
    expect(button.className).toContain(`text-muted-foreground`)
    expect(button.className).toContain(`hover:text-foreground`)
    expect(button.className).not.toContain(`underline`)
    expect(button.getAttribute(`data-variant`)).toBe(`text`)
  })

  it(`inline size has no box: auto height, no padding, 12px`, () => {
    render(
      <Button variant="link" size="inline">
        Continues in a newer run
      </Button>
    )
    const button = screen.getByRole(`button`)
    expect(button.className).toContain(`h-auto`)
    expect(button.className).toContain(`p-0`)
    expect(button.className).toContain(`text-xs`)
    expect(button.className).not.toContain(`h-9`)
    expect(button.className).not.toContain(`px-4`)
    expect(button.className).toContain(`hover:underline`)
    expect(button.getAttribute(`data-size`)).toBe(`inline`)
  })

  // S-3 (voice-call.json): the phone call's round buttons.
  it(`icon-lg is a 56px circle with a 24px glyph that presses`, () => {
    render(
      <Button variant="destructive" size="icon-lg" aria-label="End call">
        <svg />
      </Button>
    )
    const button = screen.getByRole(`button`, { name: `End call` })
    expect(button.className).toContain(`size-14`)
    expect(button.className).toContain(`rounded-full`)
    expect(button.className).toContain(`[&_svg:not([class*='size-'])]:size-6`)
    expect(button.className).toContain(`motion-press`)
    expect(button.getAttribute(`data-size`)).toBe(`icon-lg`)
  })
})
