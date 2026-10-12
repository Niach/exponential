// EXP-1240: the fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/work-card.json" with { type: "json" }
import { workCard, workDurationLong, workDurationShort, type WorkCardInput } from "./work-card"

describe(`workCard`, () => {
  test(`durations`, () => {
    expect(workDurationShort(42_000)).toBe(`42s`)
    expect(workDurationShort(48 * 60_000)).toBe(`48m`)
    expect(workDurationShort(72 * 60_000)).toBe(`1h 12m`)
    expect(workDurationShort(120 * 60_000)).toBe(`2h`)
    expect(workDurationLong(509_000)).toBe(`8m 29s`)
    expect(workDurationLong(72 * 60_000)).toBe(`1h 12m`)
  })

  test(`the fixture covers all three kinds`, () => {
    const kinds = new Set(fixture.cases.map((entry) => entry.input.kind))
    expect([...kinds].sort()).toEqual([`subagent`, `tasks`, `workflow`])
  })

  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(workCard(entry.input as WorkCardInput)).toEqual(entry.expected as never)
    })
  }
})
