import { describe, expect, it } from "vitest"
import { DANGER_PERCENT, WARNING_PERCENT } from "@/lib/agent-usage"
import {
  RING_RADIUS,
  RING_SIZE,
  RING_STROKE,
  ringGeometry,
  ringToneClass,
} from "@/lib/context-ring"

// EXP-877: the composer footer's context ring — geometry and tone only.

describe(`ringGeometry`, () => {
  const CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS

  it(`fits inside its 16px box`, () => {
    expect(RING_SIZE).toBe(16)
    expect(RING_STROKE).toBe(2)
    expect(RING_RADIUS * 2 + RING_STROKE).toBeLessThanOrEqual(RING_SIZE)
  })

  it(`maps the percent onto the dash offset`, () => {
    expect(ringGeometry(0)).toEqual({
      circumference: CIRCUMFERENCE,
      dashOffset: CIRCUMFERENCE,
    })
    expect(ringGeometry(100)).toEqual({
      circumference: CIRCUMFERENCE,
      dashOffset: 0,
    })
    expect(ringGeometry(50).dashOffset).toBeCloseTo(CIRCUMFERENCE / 2, 10)
  })

  it(`clamps anything outside 0..100`, () => {
    expect(ringGeometry(-20).dashOffset).toBe(CIRCUMFERENCE)
    expect(ringGeometry(140).dashOffset).toBe(0)
    expect(ringGeometry(Number.NaN).dashOffset).toBe(CIRCUMFERENCE)
  })
})

describe(`ringToneClass`, () => {
  // The thresholds are `severity`'s, never restated: 75 warning, 95 danger.
  it(`follows the shared severity thresholds`, () => {
    expect(ringToneClass(0)).toBe(`text-muted-foreground`)
    expect(ringToneClass(WARNING_PERCENT - 1)).toBe(`text-muted-foreground`)
    expect(ringToneClass(WARNING_PERCENT)).toBe(`text-amber-500`)
    expect(ringToneClass(DANGER_PERCENT - 1)).toBe(`text-amber-500`)
    expect(ringToneClass(DANGER_PERCENT)).toBe(`text-destructive`)
    expect(ringToneClass(100)).toBe(`text-destructive`)
  })
})
