import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/thread-row-action.json" with { type: "json" }
import { sessionWaitingCard, threadRowAction, type ThreadRowActionInput } from "./thread-row-action"

describe(`thread-row-action.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      expect(threadRowAction(c.input as ThreadRowActionInput)).toEqual(c.expected as ReturnType<typeof threadRowAction>)
    })
  }
  for (const c of fixture.waitingCases) {
    test(`waiting: ${c.name}`, () => {
      expect(sessionWaitingCard(c.sessionId, c.messages)).toEqual(c.expected)
    })
  }
})
