// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/waiting-cards.json" with { type: "json" }
import { cardState, dismissedCard, waitingChips } from "./waiting-cards"

describe(`cardState`, () => {
  for (const entry of fixture.stateCases) {
    test(entry.name, () => {
      expect(cardState(entry.card, entry.answeredAt)).toBe(entry.expected as never)
    })
  }
})

describe(`dismissedCard`, () => {
  for (const entry of fixture.dismissCases) {
    test(entry.name, () => {
      expect(dismissedCard(entry.card)).toEqual(entry.expected)
    })
  }
})

describe(`waitingChips`, () => {
  for (const entry of fixture.chipCases) {
    test(entry.name, () => {
      expect(waitingChips(entry.cards as never, entry.phone)).toEqual(entry.expected as never)
    })
  }
})
