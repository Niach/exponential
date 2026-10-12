import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { Spinner } from "./spinner"

// EXP-1275 (feedback-states.json `inline.spinner`): THE inline spinner is a
// 10px glyph by default; a caller (the pending Button) asks for its own size.
describe(`Spinner`, () => {
  it(`is the 10px muted glyph with an accessible name by default`, () => {
    const { container } = render(<Spinner />)
    const glyph = container.querySelector(`[data-slot=spinner]`)!
    expect(glyph.getAttribute(`width`)).toBe(`10`)
    expect(glyph.getAttribute(`height`)).toBe(`10`)
    expect(glyph.getAttribute(`aria-label`)).toBe(`Loading`)
    expect(glyph.getAttribute(`class`)).toContain(`animate-spin`)
    expect(glyph.getAttribute(`class`)).toContain(`text-muted-foreground`)
  })

  it(`takes a size, a label and extra classes`, () => {
    const { container } = render(<Spinner size={12} label="Starting" className="text-current" />)
    const glyph = container.querySelector(`[data-slot=spinner]`)!
    expect(glyph.getAttribute(`width`)).toBe(`12`)
    expect(glyph.getAttribute(`aria-label`)).toBe(`Starting`)
    expect(glyph.getAttribute(`class`)).toContain(`text-current`)
  })
})
