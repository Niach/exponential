import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-share.json" with { type: "json" }
import { historyRows } from "./app-records"
import { linkState, shareByline, shareFields, shareRowSubtitle, shareSwitchOn, underShareCaps, type ShareLinkState, type ShareMode, type ShareSchema } from "./app-share"

describe(`app-share.json`, () => {
  test(`expiryChoices: whole days within a package's expires, never last`, () => {
    const days = fixture.expiryChoices.map((c) => c.days)
    expect(days.at(-1)).toBeNull()
    for (const d of days.slice(0, -1)) expect(Number.isInteger(d) && d! >= 1 && d! <= 3650).toBe(true)
    expect(new Set(fixture.expiryChoices.map((c) => c.label)).size).toBe(days.length)
  })
  test.each(fixture.stateCases)(`linkState: $name`, (c) => expect(linkState(c.link, c.declared, c.now)).toBe(c.state as ShareLinkState))
  test.each(fixture.capCases)(`caps: $name`, (c) => expect(underShareCaps(c.ipCount, c.linkCount)).toBe(c.allowed))
  test.each(fixture.fieldCases)(`shareFields: $name`, (c) => {
    expect(shareFields(c.schema as ShareSchema, c.fields)).toEqual(c.submit)
    expect(shareFields(c.schema as ShareSchema, c.fields)).toEqual(c.view)
  })
  test.each(fixture.bylineCases)(`shareByline: $name`, (c) => expect(shareByline(c.shareName, c.byline)).toBe(c.text))
  test.each(fixture.historyCases)(`history: $name`, (c) => {
    const event = { id: `e`, op: `create` as const, before: null, patch: {}, actor: c.actor, via: c.via, createdAt: `2026-11-04T00:00:00Z` }
    expect(historyRows([event], c.names, `?`)[0]!.who).toBe(c.who)
  })
  test.each(fixture.switchCases)(`shareSwitchOn: $state`, (c) => expect(shareSwitchOn(c.state as ShareLinkState)).toBe(c.on))
  test.each(fixture.rowCases)(`shareRowSubtitle: $name`, (c) => expect(shareRowSubtitle(c.mode as ShareMode, c.state as ShareLinkState, c.expiresAt)).toBe(c.text))
})
