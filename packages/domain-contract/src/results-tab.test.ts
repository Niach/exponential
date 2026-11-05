// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/results-tab.json" with { type: "json" }
import { isPrPictureCard, resultsTab } from "./results-tab"

describe(`resultsTab`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(resultsTab(entry.input as never)).toEqual(entry.expected as never)
    })
  }
})

describe(`isPrPictureCard`, () => {
  for (const entry of fixture.prPictureCases) {
    test(entry.name, () => {
      expect(isPrPictureCard(entry.card)).toBe(entry.expected)
    })
  }
})
