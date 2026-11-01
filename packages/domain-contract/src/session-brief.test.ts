import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/session-brief.json" with { type: "json" }
import { composeBrief, composeIssueBrief, withIssueBrief } from "./session-brief"

describe(`session brief fixture (EXP-1291)`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(composeBrief(entry.input as never)).toBe(entry.expected)
    })
  }
  for (const entry of fixture.issueBriefCases) {
    test(`issue brief: ${entry.name}`, () => {
      expect(composeIssueBrief(entry.input as never)).toBe(entry.expected)
    })
  }
  for (const entry of fixture.deliveryCases) {
    test(`delivery: ${entry.name}`, () => {
      expect(withIssueBrief(entry.body, entry.issueBrief)).toBe(entry.expected)
    })
  }
  test(`an oversized brief is cut at a line and ends in an ellipsis line`, () => {
    const text = Array.from({ length: 9000 }, (_, i) => `line ${i} ${`x`.repeat(5)}`).join(`\n`)
    const brief = composeBrief({ text })
    expect(new TextEncoder().encode(brief).length).toBeLessThanOrEqual(65536)
    expect(brief.endsWith(`\n…`)).toBe(true)
  })
})
