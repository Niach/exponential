import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/swipe.json" with { type: "json" }
import { SWIPE, swipeBlocks, swipeIndicator, swipeLock, swipeOffset, swipeProgress, swipeSettle, swipeStep } from "./swipe"

describe(`FEED-200 fixtures/swipe.json`, () => {
  test(`constants`, () => expect({ ...SWIPE } as Record<string, number>).toEqual(fixture.constants))
  test(`lock`, () => {
    for (const c of fixture.lock) expect(swipeLock(c.dx, c.dy), c.name).toBe(c.lock as never)
  })
  test(`step`, () => {
    for (const c of fixture.step) expect(swipeStep(c.offset, c.rtl), c.name).toBe(c.step as never)
  })
  test(`offset`, () => {
    for (const c of fixture.offset) expect(swipeOffset(c.index, c.count, c.offset, c.width, c.rtl), c.name).toBeCloseTo(c.drawn, 6)
  })
  test(`settle`, () => {
    for (const c of fixture.settle) expect(swipeSettle(c.index, c.count, c.offset, c.velocity, c.width, c.rtl), c.name).toBe(c.settle)
  })
  test(`progress`, () => {
    for (const c of fixture.progress) expect(swipeProgress(c.offset, c.width), c.name).toBeCloseTo(c.progress, 6)
  })
  test(`indicator`, () => {
    for (const c of fixture.indicator) {
      const got = swipeIndicator(c.from, c.to, c.progress)
      expect(got.x, c.name).toBeCloseTo(c.indicator.x, 6)
      expect(got.w, c.name).toBeCloseTo(c.indicator.w, 6)
    }
  })
  test(`blocks`, () => {
    for (const c of fixture.blocks) expect(swipeBlocks(c.component, c.direction, c.overflowX), c.name).toBe(c.blocks)
  })
})
