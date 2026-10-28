// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/stories.json" with { type: "json" }
import { stories } from "./stories"

describe(`stories`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(stories(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})
