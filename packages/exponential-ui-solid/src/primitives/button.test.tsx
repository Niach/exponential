import { render, screen } from "@solidjs/testing-library"
import { describe, expect, it } from "vitest"

import { Button } from "./button"

// EXP-962: the text button and the inline size (React's button.test.tsx).

describe(`Button`, () => {
  it(`text variant is muted words that brighten, no underline`, () => {
    render(() => (
      <Button variant="text" size="inline">
        Show more
      </Button>
    ))
    const button = screen.getByRole(`button`)
    expect(button.className).toContain(`text-muted-foreground`)
    expect(button.className).toContain(`hover:text-foreground`)
    expect(button.className).not.toContain(`underline`)
    expect(button.getAttribute(`data-variant`)).toBe(`text`)
  })

  it(`inline size has no box: auto height, no padding, 12px`, () => {
    render(() => (
      <Button variant="link" size="inline">
        Continues in a newer run
      </Button>
    ))
    const button = screen.getByRole(`button`)
    expect(button.className).toContain(`h-auto`)
    expect(button.className).toContain(`p-0`)
    expect(button.className).toContain(`text-xs`)
    expect(button.className).not.toContain(`h-9`)
    expect(button.className).not.toContain(`px-4`)
    expect(button.className).toContain(`hover:underline`)
    expect(button.getAttribute(`data-size`)).toBe(`inline`)
  })

  it(`as = React's asChild: the element wears the button`, () => {
    render(() => (
      <Button as="a" href="#docs" variant="outline">
        Docs
      </Button>
    ))
    const link = screen.getByRole(`link`)
    expect(link.localName).toBe(`a`)
    expect(link.getAttribute(`href`)).toBe(`#docs`)
    expect(link.getAttribute(`data-slot`)).toBe(`button`)
    expect(link.className).toContain(`border`)
  })
})
