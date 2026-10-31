// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/audit.json" with { type: "json" }
import { argsHash, auditExpired, callStatus, canonicalJson } from "./audit"

describe(`canonicalJson`, () => {
  for (const entry of fixture.canonicalCases) {
    test(entry.expected, () => expect(canonicalJson(entry.value)).toBe(entry.expected))
  }
})

describe(`argsHash`, () => {
  for (const entry of fixture.hashCases) {
    test(JSON.stringify(entry.args), () => expect(argsHash(entry.args)).toBe(entry.expected))
  }
})

describe(`callStatus`, () => {
  for (const entry of fixture.statusCases) {
    test(JSON.stringify(entry.outcome), () => expect(callStatus(entry.outcome)).toBe(entry.expected as never))
  }
})

describe(`auditExpired`, () => {
  for (const entry of fixture.retentionCases) {
    test(entry.createdAt, () => expect(auditExpired(entry.createdAt, entry.now)).toBe(entry.expected))
  }
})
