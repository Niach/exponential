import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Alert, AlertDescription, AlertTitle } from "./alert"
import { conceptIcon } from "./icons.generated"

// EXP-941: the admin console carried this banner twice, byte for byte.

describe(`Alert`, () => {
  it(`is an alert with a title and a description`, () => {
    render(
      <Alert>
        <AlertTitle>Heads up</AlertTitle>
        <AlertDescription>Nothing to do yet.</AlertDescription>
      </Alert>
    )
    const alert = screen.getByRole(`alert`)
    expect(alert.getAttribute(`data-slot`)).toBe(`alert`)
    expect(alert.textContent).toContain(`Heads up`)
    expect(
      alert.querySelector(`[data-slot=alert-description]`)!.textContent
    ).toBe(`Nothing to do yet.`)
  })

  it(`paints the destructive variant the way the admin banners did`, () => {
    render(<Alert variant="destructive">Could not load teams</Alert>)
    const alert = screen.getByRole(`alert`)
    expect(alert.className).toContain(`text-destructive`)
    expect(alert.className).toContain(`border-destructive/50`)
    expect(alert.className).toContain(`bg-destructive/10`)
  })

  it(`opens a glyph column only when it has a glyph`, () => {
    const WarningGlyph = conceptIcon(`ui-warning`)
    const { container, rerender } = render(<Alert>No glyph</Alert>)
    expect(container.querySelector(`svg`)).toBeNull()
    rerender(
      <Alert variant="destructive">
        <WarningGlyph aria-hidden />
        <AlertTitle>Failed</AlertTitle>
      </Alert>
    )
    expect(container.querySelector(`svg`)).toBeTruthy()
    expect(screen.getByRole(`alert`).className).toContain(`has-[>svg]:`)
  })
})
