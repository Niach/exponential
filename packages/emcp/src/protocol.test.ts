// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/protocol.json" with { type: "json" }
import {
  canDeliver,
  argsCanonical,
  clientContextOf,
  elicitationAnswer,
  elicitForm,
  elicitUrlRequest,
  inputRequiredResult,
  redeemVerdict,
  type InputRequest,
} from "./protocol"

describe(`clientContextOf`, () => {
  for (const entry of fixture.clientCases) {
    test(entry.name, () => expect(clientContextOf(entry.message) as unknown).toEqual(entry.expected))
  }
})

describe(`canDeliver`, () => {
  for (const entry of fixture.deliverCases) {
    test(entry.name, () => {
      const client = clientContextOf({ params: { _meta: { [fixture.metaKeys.protocolVersion]: `2026-07-28`, [fixture.metaKeys.clientCapabilities]: entry.capabilities } } })
      expect(canDeliver(entry.request as InputRequest, client)).toBe(entry.expected)
    })
  }
})

describe(`elicitationAnswer`, () => {
  for (const entry of fixture.answerCases) {
    test(entry.name, () => expect(elicitationAnswer(fixture.answerResponses, entry.key) as unknown).toEqual(entry.expected))
  }
})

describe(`argsCanonical`, () => {
  for (const entry of fixture.digestCases) {
    test(entry.name, () => expect(argsCanonical(entry.args)).toBe(entry.canonical))
  }
})

describe(`redeemVerdict`, () => {
  for (const entry of fixture.stateCases) {
    test(entry.name, () => expect(redeemVerdict(entry.claims, entry.expected, entry.now, entry.used)).toBe(entry.verdict as never))
  }
})

describe(`the spec's goldens`, () => {
  const golden = fixture.spec.inputRequiredResult.result
  test(`elicitForm builds the spec's form request`, () => {
    const { params } = golden.inputRequests.github_login
    expect(elicitForm(params.message, params.requestedSchema) as unknown).toEqual(golden.inputRequests.github_login)
  })
  test(`elicitUrlRequest builds the spec's url request`, () => {
    const { params } = fixture.spec.urlElicitation
    expect(elicitUrlRequest(params.message, params.url) as unknown).toEqual(fixture.spec.urlElicitation)
  })
  test(`inputRequiredResult builds the spec's result`, () => {
    const built = inputRequiredResult({ inputRequests: golden.inputRequests as Record<string, InputRequest>, requestState: golden.requestState })
    expect(built as unknown).toEqual(golden)
  })
  test(`inputRequiredResult needs inputRequests or requestState`, () => {
    expect(() => inputRequiredResult({})).toThrow()
    expect(() => inputRequiredResult({ inputRequests: {} })).toThrow()
    expect(inputRequiredResult({ requestState: `s` })).toEqual({ resultType: `input_required`, requestState: `s` })
  })
  test(`the discover golden carries every key ours does`, () => {
    expect(Object.keys(fixture.spec.discoverResponse.result).sort()).toEqual([...fixture.spec.discoverResultKeys].sort())
  })
})
