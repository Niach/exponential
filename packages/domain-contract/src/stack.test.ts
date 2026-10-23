// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import toastFixture from "../fixtures/toast-stack.json" with { type: "json" }
import { stackLayout, TOAST_STACK } from "./stack"

describe(`stackLayout (toast-stack.json geometry)`, () => {
  for (const entry of toastFixture.geometry) {
    test(entry.name, () => {
      expect(
        stackLayout(entry.heights, { expanded: entry.expanded, anchoredBottom: entry.anchoredBottom }, TOAST_STACK)
      ).toEqual({ height: entry.height, items: entry.items })
    })
  }
})
