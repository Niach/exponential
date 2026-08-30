import { describe, expect, it } from "vitest"
import { shouldDismissSheet } from "@/hooks/use-sheet-drag"

// EXP-687 — the decision half of drag-to-dismiss. Kept pure precisely so the
// thresholds are testable without a pointer: a slow long drag closes, a fast
// short flick closes, a nudge springs back, and an upward drag never closes.
describe(`shouldDismissSheet`, () => {
  it(`closes on a long drag regardless of speed`, () => {
    expect(shouldDismissSheet(120, 2000)).toBe(true)
    expect(shouldDismissSheet(81, 5000)).toBe(true)
  })

  it(`closes on a fast flick that never travels far`, () => {
    // 40px in 50ms = 0.8px/ms, over the 0.6 threshold.
    expect(shouldDismissSheet(40, 50)).toBe(true)
  })

  it(`springs back on a slow short drag`, () => {
    expect(shouldDismissSheet(40, 400)).toBe(false)
    expect(shouldDismissSheet(80, 1000)).toBe(false)
  })

  it(`ignores a flick too small to be intentional`, () => {
    // Fast, but only 20px — under the 24px floor, so it is a tap wobble.
    expect(shouldDismissSheet(20, 10)).toBe(false)
  })

  it(`never closes on an upward drag`, () => {
    expect(shouldDismissSheet(-200, 50)).toBe(false)
    expect(shouldDismissSheet(0, 10)).toBe(false)
  })

  it(`does not divide by a zero-length gesture`, () => {
    expect(shouldDismissSheet(40, 0)).toBe(false)
  })
})
