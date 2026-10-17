import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/session-title.json" with { type: "json" }
import { SESSION_TITLE_COPY, sessionTitle } from "./session-title"

describe(`session title (EXP-1341)`, () => {
  test(`copy comes from the fixture`, () => {
    expect(SESSION_TITLE_COPY.titleMax).toBe(fixture.titleMax)
    expect(SESSION_TITLE_COPY.untitled).toBe(fixture.untitled)
  })
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      const { session, links, issues, messages } = entry.input as never
      expect(sessionTitle(session, links, issues, messages)).toBe(entry.expected)
    })
  }
})
