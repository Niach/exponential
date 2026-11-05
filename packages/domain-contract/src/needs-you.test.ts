// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/needs-you.json" with { type: "json" }
import { needsYou } from "./needs-you"

describe(`needsYou`, () => {
  for (const entry of [...fixture.cases, ...fixture.prCases]) {
    test(entry.name, () => {
      expect(needsYou(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})
