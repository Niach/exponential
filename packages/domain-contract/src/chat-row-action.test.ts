import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/chat-row-action.json" with { type: "json" }
import { sessionWaitingCard, chatRowAction, type ChatRowActionInput } from "./chat-row-action"

describe(`chat-row-action.json`, () => {
  for (const c of fixture.cases) {
    test(c.name, () => {
      expect(chatRowAction(c.input as ChatRowActionInput)).toEqual(c.expected as ReturnType<typeof chatRowAction>)
    })
  }
  for (const c of fixture.waitingCases) {
    test(`waiting: ${c.name}`, () => {
      expect(sessionWaitingCard(c.sessionId, c.messages)).toEqual(c.expected)
    })
  }
})
