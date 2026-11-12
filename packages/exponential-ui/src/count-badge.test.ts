import { describe, expect, test } from "bun:test"
import { countBadgeLabel, countBadgeTone } from "./count-badge"

describe(`countBadgeLabel`, () => {
  test(`nav-badges.json labelCases`, () => {
    for (const [count, expected] of [[0, ``], [-3, ``], [1, `1`], [99, `99`], [100, `99+`], [1234, `99+`]] as const) expect(countBadgeLabel(count)).toBe(expected)
  })
  test(`max, fractions and junk`, () => {
    expect(countBadgeLabel(120, 9)).toBe(`9+`)
    expect(countBadgeLabel(9, 9)).toBe(`9`)
    expect(countBadgeLabel(2.7)).toBe(`2`)
    expect(countBadgeLabel(0.5)).toBe(``)
    expect(countBadgeLabel(Number.NaN)).toBe(``)
    expect(countBadgeLabel(`3`)).toBe(``)
    expect(countBadgeLabel(120, 0)).toBe(`99+`)
    expect(countBadgeTone(`attention`)).toBe(`attention`)
    expect(countBadgeTone(`primary`)).toBe(`muted`)
  })
})
