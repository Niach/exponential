// EXP-1275: the mark's timeline is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/feedback-states.json" with { type: "json" }
import { markStripePoint, markStripeSpan } from "./feedback-mark"

describe(`markStripeSpan`, () => {
  test(`the loop starts and ends on the plain disc`, () => {
    expect([0, 1, 2].map((i) => markStripeSpan(i, 0.9999))).toEqual([null, null, [1, 1]])
    expect([1, 2].map((i) => markStripeSpan(i, 0))).toEqual([null, null])
  })

  test(`the cubic ends where the logo's path does`, () => {
    expect(markStripePoint(0, 0)).toEqual([-5.87, 62.01])
    expect(markStripePoint(2, 1)).toEqual([96.98, -3.98])
  })

  for (const entry of fixture.mark.cases) {
    test(`phase ${entry.phase}`, () => {
      expect([0, 1, 2].map((i) => markStripeSpan(i, entry.phase))).toEqual(entry.spans as never)
    })
  }
})
