// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/access.json" with { type: "json" }
import { assistantScope, effectiveAccess, grantErrors, memberPolicy, toolVisible, type AccessInput, type Grant, type Level } from "./access"

describe(`grantErrors`, () => {
  for (const entry of fixture.grantCases) {
    test(entry.name, () => expect(grantErrors(entry.grant as Grant)).toEqual(entry.expected))
  }
})

describe(`memberPolicy`, () => {
  for (const entry of fixture.policyCases) {
    const e = entry as { row: Level | null; default?: Level; exempt?: boolean; expected: Level }
    const name = `${e.row} ${e.default ?? `-`} ${e.exempt ?? `-`}`
    const context = `default` in e ? { default: e.default, exempt: e.exempt } : undefined
    test(name, () => expect(memberPolicy(e.row, context)).toBe(e.expected))
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

describe(`assistantScope`, () => {
  for (const entry of fixture.assistantScopeCases) {
    test(entry.name, () => expect(assistantScope(entry.input)).toEqual(entry.expected))
  }
})
