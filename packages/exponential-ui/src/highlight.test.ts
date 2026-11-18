import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/text-highlight.json" with { type: "json" }
import { highlightCount, highlightLocalCurrent, highlightRanges, highlightSpec } from "./highlight"

describe(`#EDE-95 fixtures/text-highlight.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      const ranges = highlightRanges(c.text, c.query)
      expect(ranges.map((r) => [r.start, r.end])).toEqual(c.ranges)
      expect(ranges.map((r) => c.text.slice(r.start, r.end))).toEqual(c.matches)
      expect(highlightCount(c.text, c.query)).toBe(c.matches.length)
    })
  }
  for (const c of fixture.currentCases) {
    test(`current: ${c.name}`, () => {
      expect(highlightLocalCurrent(highlightSpec(c.highlight)!)).toBe(c.local)
    })
  }
  test(`the prop: a query that folds to nothing = no highlight`, () => {
    expect(highlightSpec({ query: `` })).toBeNull()
    expect(highlightSpec({ query: `́` })).toBeNull()
    expect(highlightSpec(`foo`)).toBeNull()
    expect(highlightSpec({ query: `a`, current: 2.7, before: `` })).toEqual({ query: `a`, current: 2 })
  })
})
