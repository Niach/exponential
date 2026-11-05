import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/via-app.json" with { type: "json" }
import reporterReply from "../fixtures/reporter-reply.json" with { type: "json" }
import { parseViaAppHeader, viaAppAccepted, viaAppCaption, viaAppClaim, viaAppHeaderValue, withViaApp, type ViaAppCall } from "./via-app"

describe(`via app claim (VAPP-231)`, () => {
  test.each(fixture.claim.claimCases)(`viaAppClaim: $name`, (c) => {
    const claim = viaAppClaim(c.appId, c.publishedVersion)
    expect(claim).toEqual(c.claim)
    expect(claim ? viaAppHeaderValue(claim) : null).toBe(c.header)
  })
  test.each(fixture.claim.parseCases)(`parseViaAppHeader: $name`, (c) => {
    expect(parseViaAppHeader(c.header)).toEqual(c.claim)
  })
  test.each(fixture.claim.verdictCases as (typeof fixture.claim.verdictCases[number] & { fn?: string; tool?: string; op?: string | null })[])(`viaAppAccepted: $name`, (c) => {
    const call: ViaAppCall = c.fn ? { fn: c.fn } : { tool: c.tool!, op: c.op ?? null }
    expect(viaAppAccepted({ version: c.version, publishedVersion: c.publishedVersion, access: c.access, functions: c.functions, call })).toBe(c.accepted)
  })
})

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
