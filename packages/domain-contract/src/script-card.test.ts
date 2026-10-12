// EXP-1279: the fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/script-card.json" with { type: "json" }
import { scriptCard, scriptDuration, type ScriptCardInput } from "./script-card"

describe(`scriptCard`, () => {
  test(`durations`, () => {
    expect(scriptDuration(3040)).toBe(`3.0 s`)
    expect(scriptDuration(59_900)).toBe(`59.9 s`)
    expect(scriptDuration(75_000)).toBe(`1m`)
  })

  for (const entry of fixture.durations) {
    test(`${entry.ms} ms reads ${entry.expected}`, () => {
      expect(scriptDuration(entry.ms)).toBe(entry.expected)
    })
  }

  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(scriptCard(entry.input as ScriptCardInput)).toEqual(entry.expected as never)
    })
  }
})
