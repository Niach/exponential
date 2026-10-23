import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/message-conversation.json" with { type: "json" }
import { keyMessagesBySession } from "./message-conversation"

describe(`messages keyed through their conversation (NEW-8)`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      const conversations = (`conversations` in entry ? entry.conversations : fixture.conversations) as {
        id: string
        sessionId: string | null
      }[]
      const keyed = keyMessagesBySession(entry.messages, conversations)
      expect(keyed.map(({ id, sessionId }) => ({ id, sessionId }))).toEqual(entry.expected)
    })
  }
})
