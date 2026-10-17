// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/ask-dock.json" with { type: "json" }
import { askDock, askDockTitle, planDecisionLabel, threadPendingAsks, threadSessionNames, type AskDockPending } from "./ask-dock"

describe(`askDock`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      const input = entry.input as { pending: AskDockPending[]; focusedId: string | null }
      expect(askDock(input)).toEqual(entry.expected as never)
    })
  }
})

describe(`askDockTitle`, () => {
  for (const entry of fixture.titleCases) {
    test(JSON.stringify(entry.text), () => {
      expect(askDockTitle(entry.text)).toBe(entry.expected)
    })
  }
})

describe(`planDecisionLabel`, () => {
  for (const entry of fixture.decisionLabelCases) {
    test(`${entry.decision} fresh=${entry.fresh}`, () => {
      expect(planDecisionLabel(entry.decision as `approved` | `rejected`, entry.fresh)).toBe(entry.expected)
    })
  }
})

describe(`threadPendingAsks`, () => {
  for (const entry of fixture.threadCases) {
    test(entry.name, () => {
      const names = entry.sessionNames as Record<string, string>
      expect(threadPendingAsks(entry.rows as never, (id) => names[id] ?? null)).toEqual(entry.expected as never)
    })
  }
})

describe(`threadSessionNames`, () => {
  for (const entry of fixture.sessionNameCases) {
    test(entry.name, () => {
      const name = threadSessionNames(entry.tabs as never)
      for (const [id, expected] of Object.entries(entry.lookups)) expect(name(id)).toBe(expected)
    })
  }
})
