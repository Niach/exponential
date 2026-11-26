import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/text-runs.json" with { type: "json" }
import { DROP_CAP, dropCapLines, dropCapSize, dropCapSplit, runsText, textRuns } from "./text-runs"

describe(`round 4 fixtures/text-runs.json`, () => {
  test(`the drop cap's numbers`, () => expect(fixture.dropCap).toEqual({ ...DROP_CAP }))
  for (const c of fixture.normalCases) {
    test(`runs: ${c.name}`, () => {
      const out = textRuns(c.runs)
      expect(out).toEqual(c.out as never)
      expect(out ? runsText(out) : null).toBe(c.text)
    })
  }
  for (const c of fixture.linesCases) test(`dropCap: ${c.name}`, () => expect(dropCapLines(c.props as Record<string, unknown>)).toBe(c.lines))
  for (const c of fixture.splitCases) test(`split: ${c.name}`, () => expect(dropCapSplit(textRuns(c.runs)!)).toEqual(c.split as never))
  for (const c of fixture.sizeCases) test(`size ${c.fontSize}/${c.lineHeight} × ${c.lines}`, () => expect(dropCapSize(c.fontSize, c.lineHeight, c.lines)).toBe(c.size))
})
