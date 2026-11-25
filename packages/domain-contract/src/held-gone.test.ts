import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/held-gone.json" with { type: "json" }
import { heldByCaller, heldCandidates, heldPass, heldRemoval, type HeldPassInput } from "./held-gone"

describe(`held-gone.json`, () => {
  for (const c of fixture.serverCases) {
    test(`server: ${c.name}`, () => expect(heldByCaller(c.row, c.caller)).toBe(c.kept))
  }
  for (const c of fixture.candidateCases) {
    test(`candidates: ${c.name}`, () => expect(heldCandidates(c.account, c.records, c.store)).toEqual(c.expected))
  }
  for (const c of fixture.passCases) {
    test(`pass: ${c.name}`, () => expect(heldPass(c.input as unknown as HeldPassInput)).toEqual(c.expected as never))
  }
  for (const c of fixture.removeCases) {
    test(`remove: ${c.name}`, () =>
      expect(heldRemoval(c.account, c.id, c.records, c.store, c.otherStores)).toEqual(c.expected))
  }
})
