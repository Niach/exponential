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
})
