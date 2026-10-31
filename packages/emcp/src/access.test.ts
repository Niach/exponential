// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/access.json" with { type: "json" }
import { effectiveAccess, grantErrors, memberPolicy, toolVisible, type AccessInput, type Grant, type Level } from "./access"

describe(`grantErrors`, () => {
  for (const entry of fixture.grantCases) {
    test(entry.name, () => expect(grantErrors(entry.grant as Grant)).toEqual(entry.expected))
  }
})

describe(`memberPolicy`, () => {
  for (const entry of fixture.policyCases) {
    test(String(entry.row), () => expect(memberPolicy(entry.row as Level | null)).toBe(entry.expected as Level))
  }
})

describe(`effectiveAccess`, () => {
  for (const entry of fixture.accessCases) {
    test(entry.name, () => expect(effectiveAccess(entry.input as AccessInput)).toBe(entry.expected as Level))
  }
})

describe(`toolVisible`, () => {
  for (const entry of fixture.visibleCases) {
    test(`${entry.level} sees ${entry.access}`, () => expect(toolVisible(entry.level as Level, entry.access as `read`)).toBe(entry.expected))
  }
})
