import { render } from "@solidjs/testing-library"
import { describe, expect, it } from "vitest"
import { Spinner } from "./spinner"

// EXP-1275 (feedback-states.json `inline.spinner`), React's spinner.test.tsx.
describe(`Spinner`, () => {
  it(`is the 10px muted glyph with an accessible name by default`, () => {
    const { container } = render(() => <Spinner />)
    const glyph = container.querySelector(`[data-slot=spinner]`)!
    expect(glyph.getAttribute(`width`)).toBe(`10`)
    expect(glyph.getAttribute(`height`)).toBe(`10`)
    expect(glyph.getAttribute(`aria-label`)).toBe(`Loading`)
    expect(glyph.getAttribute(`class`)).toContain(`animate-spin`)
    expect(glyph.getAttribute(`class`)).toContain(`text-muted-foreground`)
  })

  it(`takes a size, a label and extra classes`, () => {
    const { container } = render(() => <Spinner size={12} label="Starting" class="text-current" />)
    const glyph = container.querySelector(`[data-slot=spinner]`)!
    expect(glyph.getAttribute(`width`)).toBe(`12`)
    expect(glyph.getAttribute(`aria-label`)).toBe(`Starting`)
    expect(glyph.getAttribute(`class`)).toContain(`text-current`)
  })
})
