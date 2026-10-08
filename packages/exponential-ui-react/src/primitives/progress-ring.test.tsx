import { render } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { ringGeometry } from "./context-ring"
import { ProgressRing, progressRingPercent } from "./progress-ring"

// EXP-1097: the Sub-issues band's completion ring — ContextRing's geometry,
// filled done/total, painted in the colour it is handed.

describe(`progressRingPercent`, () => {
  it(`is done over total, clamped, and empty with no total`, () => {
    expect(progressRingPercent(2, 5)).toBe(40)
    expect(progressRingPercent(5, 5)).toBe(100)
    expect(progressRingPercent(7, 5)).toBe(100)
    expect(progressRingPercent(0, 0)).toBe(0)
    expect(progressRingPercent(-1, 3)).toBe(0)
  })
})

describe(`ProgressRing`, () => {
  it(`draws the track and the done arc off ringGeometry`, () => {
    const { container } = render(
      <ProgressRing done={2} total={5} colorClass="text-blue-500" />
    )
    const svg = container.querySelector(`svg`)!
    expect(svg.getAttribute(`class`)).toContain(`text-blue-500`)
    expect(svg.getAttribute(`width`)).toBe(`14`)
    const arc = container.querySelector(`[data-slot=progress-ring-arc]`)!
    expect(Number(arc.getAttribute(`stroke-dashoffset`))).toBeCloseTo(
      ringGeometry(40).dashOffset
    )
  })

  it(`draws the track alone while nothing is done, and takes a hex`, () => {
    const { container } = render(
      <ProgressRing done={0} total={3} colorHex="#ff0000" />
    )
    expect(container.querySelectorAll(`circle`)).toHaveLength(1)
    expect(container.querySelector(`svg`)!.getAttribute(`style`)).toContain(
      `color`
    )
  })
})
