// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"

import fixture from "../fixtures/automations.json" with { type: "json" }
import {
  automationClientId,
  automationTool,
  matchAutomations,
  rateAllows,
  resolveAutomationRun,
  resolveSponsor,
  validateAutomations,
  type AutomationContext,
  type FiredBy,
} from "./automations"
import type { ExposeRun } from "./exposes"

test(`every error code is listed`, () => {
  const source = readFileSync(new URL(`./automations.ts`, import.meta.url), `utf8`)
  const used = new Set([...source.matchAll(/fail\(`([a-zA-Z]+)`\)|code: `([a-zA-Z]+)`|return `([a-zA-Z]+)`/g)].map((m) => m[1] ?? m[2] ?? m[3]))
  for (const code of used) expect(fixture.errors).toContain(code!)
})

describe(`validateAutomations`, () => {
  for (const entry of fixture.validateCases) {
    test(entry.name, () => expect(validateAutomations(entry.automations, entry.context as AutomationContext)).toEqual(entry.expected))
  }
  test(`too many`, () => {
    const one = fixture.validateCases[0]!.automations as unknown[]
    const many = Array.from({ length: fixture.tooManyCase.count }, (_, i) => ({ ...(one[0] as object), name: `a${i}` }))
    expect(validateAutomations(many, { kinds: [`rotation`], upstreams: [] })).toEqual(fixture.tooManyCase.expected)
  })
})

describe(`matchAutomations`, () => {
  for (const entry of fixture.matchCases) {
    test(entry.name, () => expect(matchAutomations(entry.automations, entry.trigger as FiredBy, entry.fromAutomation)).toEqual(entry.expected))
  }
})

describe(`resolveSponsor`, () => {
  for (const entry of fixture.sponsorCases) {
    test(entry.name, () => expect(resolveSponsor(entry.input)).toEqual(entry.expected as never))
  }
})

test(`rate`, () => {
  for (const entry of fixture.rateCases) expect(rateAllows(entry.runsLastMinute)).toBe(entry.allowed)
})

describe(`resolveAutomationRun`, () => {
  for (const entry of fixture.resolveCases) {
    test(entry.name, () => expect(resolveAutomationRun(entry.run as ExposeRun, entry.input)).toEqual(entry.expected as never))
  }
})

test(`audit`, () => {
  for (const entry of fixture.auditCases) {
    expect(automationClientId(entry.name)).toBe(entry.clientId)
    expect(automationTool(entry.run as ExposeRun)).toBe(entry.tool)
  }
})
