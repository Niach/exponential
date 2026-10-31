// packages/ecore/fixtures/ecore.json, replayed against the generated TS
// (Rust tests/fixtures.rs, Swift ECoreTests, Kotlin ECoreTest replay it too).
import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/ecore.json"
import {
  bearerHeader,
  capabilityValues,
  clampMessage,
  defaultRetryable,
  deliveryVerdictValues,
  envelopeErrors,
  errorCodeValues,
  failure,
  grantAdmits,
  isRefusal,
  isValidId,
  isValidToken,
  mayRetry,
  parseBearer,
  parseCapability,
  type Capability,
  type DeliveryVerdict,
  type ErrorCode,
} from "./index"

describe(`ecore`, () => {
  it(`ids`, () => {
    for (const c of fixture.idCases) expect([c.id, isValidId(c.id)]).toEqual([c.id, c.valid])
  })

  it(`tokens and bearer headers`, () => {
    for (const c of fixture.tokenCases) expect([c.token, isValidToken(c.token)]).toEqual([c.token, c.valid])
    for (const c of fixture.bearerCases) expect([c.header, parseBearer(c.header)]).toEqual([c.header, c.token])
    expect(bearerHeader(fixture.bearerHeader.token)).toBe(fixture.bearerHeader.header)
  })

  it(`capabilities and grants`, () => {
    for (const c of fixture.capabilityCases) expect([c.name, parseCapability(c.name) !== null]).toEqual([c.name, c.known])
    expect(capabilityValues.length).toBe(fixture.capabilityCases.filter((c) => c.known).length)
    for (const c of fixture.grantCases) {
      expect(grantAdmits(c.capabilities as Capability[], c.expiresAt, c.now, c.capability as Capability)).toBe(c.admits)
    }
  })

  it(`verdicts and errors`, () => {
    expect(fixture.verdictCases.map((c) => c.verdict)).toEqual([...deliveryVerdictValues])
    for (const c of fixture.verdictCases) expect(mayRetry(c.verdict as DeliveryVerdict)).toBe(c.mayRetry)
    expect(fixture.errorCases.map((c) => c.code)).toEqual([...errorCodeValues])
    for (const c of fixture.errorCases) {
      const code = c.code as ErrorCode
      expect([code, isRefusal(code), defaultRetryable(code)]).toEqual([code, c.refusal, c.retryable])
      expect(failure(code, `x`).retryable).toBe(c.retryable)
    }
    for (const c of fixture.errorMessage.cases) {
      if (`message` in c) expect(clampMessage(c.message!)).toBe(c.clamped!)
      else expect(Array.from(clampMessage(c.repeat!.repeat(c.times!))).length).toBe(c.clampedLength!)
    }
  })

  it(`envelopes`, () => {
    for (const c of fixture.envelopeCases) expect(envelopeErrors(c.envelope)).toEqual(c.errors)
  })
})
