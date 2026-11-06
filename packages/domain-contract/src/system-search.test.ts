import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/system-search.json" with { type: "json" }
import { parseSystemSearchId, systemSearchItems } from "./system-search"

describe(`systemSearchItems`, () => {
  for (const entry of fixture.itemCases) {
    test(entry.name, () => expect(systemSearchItems(entry.input)).toEqual(entry.expected))
  }
})

describe(`parseSystemSearchId`, () => {
  for (const entry of fixture.parseCases) {
    test(entry.name, () => expect(parseSystemSearchId(entry.input)).toEqual(entry.expected as never))
  }
})
