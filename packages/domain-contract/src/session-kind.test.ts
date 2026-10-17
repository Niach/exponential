import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/session-kind.json" with { type: "json" }
import { sessionTraits } from "./session-kind"

describe(`session kind traits (EXP-1341)`, () => {
  for (const entry of fixture.cases) {
    test(String(entry.kind), () => {
      expect(sessionTraits(entry.kind)).toEqual(entry.expected)
    })
  }
})
