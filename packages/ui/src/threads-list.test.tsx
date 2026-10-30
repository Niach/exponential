import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/threads-list.json"
import { threadLeadPrState, type ThreadsListPr } from "./threads-list"

// Release train 2026-10-23 (NEW-41): the row lead's PR glyph replays the
// fixture's `leadCases`, like desktop, iOS and Android.
describe(`threadLeadPrState (threads-list.json leadCases)`, () => {
  for (const entry of fixture.leadCases) {
    it(entry.name, () => {
      const prs: ThreadsListPr[] = entry.prStates.map((state, index) => ({ id: `pr${index}`, state, basePullRequestId: null }))
      expect(threadLeadPrState(prs)).toBe(entry.expected)
    })
  }
})
