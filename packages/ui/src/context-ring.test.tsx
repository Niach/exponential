import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import {
  ContextRing,
  RING_TONE_CLASS,
  ringGeometry,
  RING_RADIUS,
  RING_SIZE,
  RING_STROKE,
} from "./context-ring"

// EXP-877: the composer footer's context ring — geometry, tone and the two
// circles it draws. The percent and the tone come IN: the package holds no
// threshold of its own.

const arc = () =>
  screen
    .getByTestId(`session-context-ring`)
    .querySelectorAll(`circle`)[1] as SVGCircleElement | undefined

describe(`ringGeometry`, () => {
  const CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS

  it(`fits inside its 16px box`, () => {
    expect(RING_SIZE).toBe(16)
    expect(RING_STROKE).toBe(2)
    expect(RING_RADIUS * 2 + RING_STROKE).toBeLessThanOrEqual(RING_SIZE)
  })

  it(`maps the percent onto the dash offset, clamped`, () => {
    expect(ringGeometry(0).dashOffset).toBe(CIRCUMFERENCE)
    expect(ringGeometry(100).dashOffset).toBe(0)
    expect(ringGeometry(50).dashOffset).toBeCloseTo(CIRCUMFERENCE / 2, 10)
    expect(ringGeometry(-20).dashOffset).toBe(CIRCUMFERENCE)
    expect(ringGeometry(140).dashOffset).toBe(0)
    expect(ringGeometry(Number.NaN).dashOffset).toBe(CIRCUMFERENCE)
  })
})

describe(`ContextRing`, () => {
  it(`renders nothing without a measurement`, () => {
    const { container } = render(<ContextRing percent={null} />)
    expect(container.innerHTML).toBe(``)
  })

  it(`showEmpty draws the track alone, muted`, () => {
    render(<ContextRing percent={null} showEmpty />)
    const button = screen.getByTestId(`session-context-ring`)
    expect(button.querySelectorAll(`circle`).length).toBe(1)
    expect(button.className).toContain(RING_TONE_CLASS.normal)
  })

  it(`draws the arc at the geometry's dash offset`, () => {
    render(<ContextRing percent={50} />)
    const { circumference, dashOffset } = ringGeometry(50)
    expect(arc()?.getAttribute(`stroke-dasharray`)).toBe(String(circumference))
    expect(arc()?.getAttribute(`stroke-dashoffset`)).toBe(String(dashOffset))
  })

  it(`paints the tone it is handed`, () => {
    const { rerender } = render(<ContextRing percent={80} tone="warning" />)
    const button = () => screen.getByTestId(`session-context-ring`)
    expect(button().className).toContain(RING_TONE_CLASS.warning)
    rerender(<ContextRing percent={98} tone="danger" />)
    expect(button().className).toContain(RING_TONE_CLASS.danger)
    rerender(<ContextRing percent={10} />)
    expect(button().className).toContain(RING_TONE_CLASS.normal)
  })
})
