import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/chat-list.json"
import { chatLeadPrState, type ChatsListPr } from "./chats-list"

// Release train 2026-10-23 (NEW-41): the row lead's PR glyph replays the
// fixture's `leadCases`, like desktop, iOS and Android.
describe(`chatLeadPrState (chat-list.json leadCases)`, () => {
  for (const entry of fixture.leadCases) {
    it(entry.name, () => {
      const prs: ChatsListPr[] = entry.prStates.map((state, index) => ({ id: `pr${index}`, state, basePullRequestId: null }))
      expect(chatLeadPrState(prs)).toBe(entry.expected)
    })
  }
})
