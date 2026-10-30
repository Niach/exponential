import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/inbox-route.json" with { type: "json" }
import { inboxRowTarget, sessionTarget } from "./inbox-route"

describe(`inbox row target (EXP-1456)`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(inboxRowTarget(entry.row)).toEqual(entry.expected as never)
    })
  }
})

describe(`session target (EXP-1456)`, () => {
  for (const entry of fixture.sessionCases) {
    test(entry.name, () => {
      expect(sessionTarget(entry.session, entry.viewerId)).toEqual(entry.expected as never)
    })
  }
})
