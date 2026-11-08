// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/ask-dock.json" with { type: "json" }
import { anchorSessionCards, askDockTitle, planDecisionLabel, sessionCards, chatPendingAsks, chatSessionNames } from "./ask-dock"

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

describe(`chatPendingAsks`, () => {
  for (const entry of fixture.chatCases) {
    test(entry.name, () => {
      const names = entry.sessionNames as Record<string, string>
      expect(chatPendingAsks(entry.rows as never, (id) => names[id] ?? null)).toEqual(entry.expected as never)
    })
  }
})

describe(`chatSessionNames`, () => {
  for (const entry of fixture.sessionNameCases) {
    test(entry.name, () => {
      const name = chatSessionNames(entry.tabs as never)
      for (const [id, expected] of Object.entries(entry.lookups)) expect(name(id)).toBe(expected)
    })
  }
})

describe(`sessionCards`, () => {
  for (const entry of fixture.sessionCardsCases) {
    test(entry.name, () => {
      expect(sessionCards(entry.messages as never)).toEqual(entry.expected as never)
    })
  }
})

describe(`anchorSessionCards`, () => {
  for (const entry of fixture.anchorCases) {
    test(entry.name, () => {
      const { anchors, unanchored } = anchorSessionCards(entry.feed, entry.cards as never)
      expect({ anchors: Object.fromEntries([...anchors].map(([id, cards]) => [String(id), cards])), unanchored }).toEqual(
        entry.expected
      )
    })
  }
})
