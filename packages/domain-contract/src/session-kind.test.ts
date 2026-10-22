import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/session-kind.json" with { type: "json" }
import { isPersistent, sessionKindOf, sessionTraits } from "./session-kind"

describe(`session kind traits (EXP-1341)`, () => {
  for (const entry of fixture.cases) {
    test(String(entry.kind), () => {
      expect(sessionTraits(entry.kind)).toEqual(entry.expected)
    })
  }
})

describe(`session kind of a row (NEW-2)`, () => {
  for (const entry of fixture.kindOfCases) {
    test(JSON.stringify(entry.session), () => {
      expect(sessionKindOf(entry.session)).toBe(entry.expected)
      expect(isPersistent(entry.session)).toBe(entry.expected === `agent`)
    })
  }
})
