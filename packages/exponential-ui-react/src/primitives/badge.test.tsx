import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Badge } from "./badge"

// EXP-962: the 16px count badge the sidebar rail drew by hand.

const badge = (container: HTMLElement) =>
  container.querySelector(`[data-slot="badge"]`) as HTMLElement | null

describe(`Badge`, () => {
  it(`is a 16px muted capsule carrying the count`, () => {
    const { container } = render(<Badge count={3} />)
    const el = badge(container)!
    expect(el.textContent).toBe(`3`)
    expect(el.className).toContain(`h-4`)
    expect(el.className).toContain(`min-w-4`)
    expect(el.className).toContain(`rounded-full`)
    expect(el.className).toContain(`bg-muted`)
    expect(el.getAttribute(`data-tone`)).toBe(`muted`)
  })

  it(`renders nothing at zero`, () => {
    const { container } = render(<Badge count={0} />)
    expect(badge(container)).toBeNull()
  })

  it(`caps at max`, () => {
    const { container } = render(<Badge count={140} />)
    expect(badge(container)!.textContent).toBe(`99+`)
    const { container: nine } = render(<Badge count={12} max={9} />)
    expect(badge(nine)!.textContent).toBe(`9+`)
  })

  it(`takes the accent fill for the primary tone and the caller's placement`, () => {
    const { container } = render(
      <Badge count={5} tone="primary" className="absolute -right-0.5 -top-0.5" />
    )
    const el = badge(container)!
    expect(el.className).toContain(`bg-primary`)
    expect(el.className).toContain(`absolute`)
    expect(el.getAttribute(`data-tone`)).toBe(`primary`)
  })
})
