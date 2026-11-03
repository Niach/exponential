import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/via-app.json" with { type: "json" }
import reporterReply from "../fixtures/reporter-reply.json" with { type: "json" }
import { viaAppCaption, withViaApp } from "./via-app"

describe(`via app (VAPP-229)`, () => {
  test.each(fixture.captionCases)(`viaAppCaption: $name`, (c) => {
    expect(viaAppCaption(c.viaAppName)).toBe(c.text)
  })
  test.each(fixture.appendCases)(`withViaApp: $name`, (c) => {
    expect(withViaApp(c.text, c.viaAppName)).toBe(c.expected)
  })
  test(`one string with the comments' caption (#VAPP-186)`, () => {
    expect(fixture.copy.viaApp).toBe(reporterReply.captions.viaApp)
    expect(fixture.copy.separator).toBe(reporterReply.captions.separator)
  })
})
