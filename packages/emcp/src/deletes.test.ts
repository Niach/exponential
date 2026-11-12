// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/deletes.json" with { type: "json" }
import {
  burstAdmit,
  burstLimits,
  destructiveTarget,
  newBurstState,
  prodBaseVerdict,
  singleIdError,
} from "./deletes"

describe(`singleIdError`, () => {
  for (const entry of fixture.singleIdCases) {
    test(JSON.stringify(entry.value), () => expect(singleIdError(entry.value)).toBe(entry.expected as never))
  }
})

describe(`burstAdmit`, () => {
  for (const entry of fixture.burstCases) {
    test(entry.name, () => {
      const state = newBurstState()
      const limits = { max: entry.max, windowSeconds: entry.windowSeconds }
      expect(entry.at.map((s) => burstAdmit(state, s * 1000, limits))).toEqual(entry.expected as never)
    })
  }
})

describe(`burstLimits`, () => {
  for (const entry of fixture.envCases) {
    test(JSON.stringify(entry.env), () => expect(burstLimits(entry.env)).toEqual(entry.expected))
  }
})

describe(`destructiveTarget`, () => {
  for (const entry of fixture.targetCases) {
    test(`${entry.tool} ${JSON.stringify(entry.args)}`, () =>
      expect(destructiveTarget(entry.tool, entry.args)).toBe(entry.expected))
  }
})

describe(`prodBaseVerdict`, () => {
  for (const entry of fixture.prodHostCases) {
    test(`${entry.base} ${entry.flag}`, () => expect(prodBaseVerdict(entry.base, entry.flag)).toBe(entry.expected as never))
  }
})
