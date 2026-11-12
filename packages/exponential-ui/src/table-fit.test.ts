import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/table-fit.json" with { type: "json" }
import { TABLE_WORD_CAP, fitTableColumns } from "./table-fit"

describe(`#FEED-224 fixtures/table-fit.json`, () => {
  test(`the word cap`, () => expect(TABLE_WORD_CAP).toBe(160))
  for (const c of fixture.cases) {
    test(c.name, () => {
      expect(fitTableColumns(c.columns, c.available)).toEqual(c.widths)
    })
  }
})
