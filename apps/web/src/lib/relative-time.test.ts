import { describe, expect, it } from "vitest"
import { compactRelativeTime } from "./relative-time"

describe(`compactRelativeTime`, () => {
  const now = Date.parse(`2026-10-09T12:00:00Z`)
  const ago = (ms: number) => new Date(now - ms)
  it(`reads just now · 5m · 2h · 3d`, () => {
    expect(compactRelativeTime(ago(20_000), now)).toBe(`just now`)
    expect(compactRelativeTime(ago(5 * 60_000), now)).toBe(`5m`)
    expect(compactRelativeTime(ago(2 * 3_600_000), now)).toBe(`2h`)
    expect(compactRelativeTime(ago(3 * 86_400_000), now)).toBe(`3d`)
  })
  it(`is empty for an invalid date`, () => {
    expect(compactRelativeTime(`nope`, now)).toBe(``)
  })
})
