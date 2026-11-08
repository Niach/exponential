import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/transcript-grouping.json" with { type: "json" }
import { TRANSCRIPT_LAYOUT, crossSessionInput, transcriptLayout } from "./transcript-grouping"

describe(`transcript grouping (EXP-1736)`, () => {
  test(`layout comes from the fixture`, () => {
    expect(TRANSCRIPT_LAYOUT).toEqual(fixture.layout)
  })
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(transcriptLayout(entry.input as never)).toEqual(entry.expected as never)
    })
  }
  for (const entry of fixture.inputCases) {
    test(`input: ${entry.name}`, () => {
      expect(crossSessionInput(entry.text)).toEqual(entry.expected as never)
    })
  }
})
