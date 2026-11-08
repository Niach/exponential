import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/message-session.json" with { type: "json" }
import { keyMessagesBySession } from "./message-session"

describe(`messages keyed by their own session (#EXP-1730)`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      const keyed = keyMessagesBySession(entry.messages as { id: string; sessionId: string | null }[])
      expect(keyed.map(({ id, sessionId }) => ({ id, sessionId }))).toEqual(entry.expected)
    })
  }
})
