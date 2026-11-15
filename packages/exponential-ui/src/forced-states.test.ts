import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/forced-states.json" with { type: "json" }
import { forcedStatesOf, normalizeForcedStates, type ForcedStates } from "./forced-states"

describe(`#EXP-1781 fixtures/forced-states.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      let forced: Record<string, string[]> = {}
      let dropped: string[] = []
      for (const step of c.steps as { forced?: ForcedStates }[]) if (step.forced) ({ forced, dropped } = normalizeForcedStates(step.forced))
      expect(dropped).toEqual(c.expect.dropped)
      for (const [id, states] of Object.entries(c.expect.states)) expect([id, forcedStatesOf(forced, id)]).toEqual([id, states])
    })
  }
  test(`a dotted prefix only: \`tab\` never covers \`tabs\``, () => {
    expect(forcedStatesOf({ tab: [`hover`] }, `tabs`)).toEqual([])
    expect(forcedStatesOf({ tab: [`hover`] }, `tab.item.0`)).toEqual([`hover`])
  })
})
