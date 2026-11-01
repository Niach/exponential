// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/plan-page.json" with { type: "json" }
import { mosaicTiles, planBlocks, planCounts, planCountsLine, planOutline, planPageFor, planPagePath } from "./plan-page"

describe(`planBlocks`, () => {
  for (const entry of fixture.blockCases) {
    test(entry.name, () => expect(planBlocks(entry.markdown)).toEqual(entry.expected as never))
  }
})

describe(`planOutline`, () => {
  for (const entry of fixture.outlineCases) {
    test(entry.name, () => expect(planOutline(entry.markdown)).toEqual(entry.expected))
  }
})

describe(`planCounts`, () => {
  for (const entry of fixture.countCases) {
    test(entry.name, () => expect(planCounts(entry.card)).toEqual(entry.expected))
  }
  for (const entry of fixture.countsLineCases) {
    test(`line ${JSON.stringify(entry.counts)}`, () => expect(planCountsLine(entry.counts)).toBe(entry.expected))
  }
})

describe(`mosaicTiles`, () => {
  for (const entry of fixture.mosaicCases) {
    test(`${entry.count} pictures`, () => expect(mosaicTiles(entry.count)).toEqual(entry.expected as never))
  }
})

describe(`planPageFor`, () => {
  for (const entry of fixture.targetCases) {
    test(entry.name, () => expect(planPageFor(entry.sessionId, entry.messages)).toBe(entry.expected))
  }
  test(`the path`, () => expect(planPagePath(`acme`, `s1`, `m1`)).toBe(`/t/acme/sessions/s1/plan/m1`))
})
