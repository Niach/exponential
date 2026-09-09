import { describe, expect, it } from "vitest"
import {
  toolGroupSummary,
  TOOL_GROUP_SUMMARY_SEPARATOR,
  type ToolCallSummary,
} from "@exp/domain-contract"
import fixture from "@exp/domain-contract/fixtures/tool-group-summary.json"

// EXP-785: the collapsed tool-group caption, locked ×4 (Android
// ToolGroupSummaryTest, iOS ToolGroupSummaryTests, desktop
// steer::tool_group_summary) against the ONE contract fixture — same cases,
// same test names.
interface FixtureCase {
  name: string
  calls: ToolCallSummary[]
  expected: string
}

const cases = fixture as FixtureCase[]

describe(`toolGroupSummary`, () => {
  it(`every fixture case renders byte-exact`, () => {
    expect(cases.length).toBeGreaterThanOrEqual(12)
    for (const { name, calls, expected } of cases) {
      expect(toolGroupSummary(calls), name).toBe(expected)
    }
  })

  it(`the fixture covers every segment`, () => {
    const expectations = cases.map((c) => c.expected)
    const covers = (needle: string) =>
      expectations.some((text) => text.includes(needle))
    expect(covers(`No tool calls`)).toBe(true)
    expect(covers(`Used 1 tool`)).toBe(true)
    expect(covers(`Used 3 tools`)).toBe(true)
    for (const first of [`Ran `, `Edited `, `Read `, `Searched `, `Fetched `]) {
      expect(expectations.some((text) => text.startsWith(first)), first).toBe(true)
    }
    for (const segment of [
      `1 command`,
      `2 commands`,
      `1 file`,
      `2 files`,
      `1 time`,
      `2 times`,
      `1 page`,
      `2 pages`,
      `1 other tool`,
      `2 other tools`,
      `1 failed`,
      `2 failed`,
    ]) {
      expect(covers(segment), segment).toBe(true)
    }
    expect(TOOL_GROUP_SUMMARY_SEPARATOR).toBe(` · `)
    expect(covers(TOOL_GROUP_SUMMARY_SEPARATOR)).toBe(true)
  })
})
