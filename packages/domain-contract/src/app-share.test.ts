import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/app-share.json" with { type: "json" }
import { linkState, shareByline, shareFields, shareRowSubtitle, underShareCaps, type ShareLinkState, type ShareMode, type ShareSchema } from "./app-share"

describe(`app-share.json`, () => {
  test.each(fixture.stateCases)(`linkState: $name`, (c) => expect(linkState(c.link, c.declared, c.now)).toBe(c.state as ShareLinkState))
  test.each(fixture.capCases)(`caps: $name`, (c) => expect(underShareCaps(c.ipCount, c.linkCount)).toBe(c.allowed))
  test.each(fixture.fieldCases)(`shareFields: $name`, (c) => {
    expect(shareFields(c.schema as ShareSchema, c.fields)).toEqual(c.submit)
    expect(shareFields(c.schema as ShareSchema, c.fields)).toEqual(c.view)
  })
  test.each(fixture.bylineCases)(`shareByline: $name`, (c) => expect(shareByline(c.shareName, c.byline)).toBe(c.text))
  test.each(fixture.rowCases)(`shareRowSubtitle: $name`, (c) => expect(shareRowSubtitle(c.mode as ShareMode, c.state as ShareLinkState, c.expiresAt)).toBe(c.text))
})
