import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/session-screen.json" with { type: "json" }
import { sessionScreen, sessionScreenKind, sessionScreenStatus, sessionScreenSubtitle } from "./session-screen"

describe(`session-screen.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      expect(sessionScreen(c.input)).toEqual(c.expected as ReturnType<typeof sessionScreen>)
    })
  }
  for (const c of fixture.kindCases) {
    test(`kind: ${c.name}`, () => {
      expect(sessionScreenKind(c.input)).toBe(c.expected as ReturnType<typeof sessionScreenKind>)
    })
  }
  for (const c of fixture.subtitleCases) {
    test(`subtitle: ${c.name}`, () => {
      expect(sessionScreenSubtitle(c.input)).toBe(c.expected)
    })
  }
  for (const c of fixture.statusCases) {
    test(`status: ${c.name}`, () => {
      expect(sessionScreenStatus(c.input)).toBe(c.expected)
    })
  }
})
