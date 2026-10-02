import { describe, expect, it } from "vitest"
import { ISSUE_DRAFT_AUTOSAVE_MS, ISSUE_DRAFT_COPY } from "./issue-draft-page"

// EXP-1170: the New issue page's copy and autosave timing, locked ×4
// (desktop domain::issue_draft, iOS IssueDraftPageTests, Android
// IssueDraftPageTest) against the ONE contract fixture.
describe(`issue draft page (contract fixture)`, () => {
  it(`carries the contract copy`, () => {
    expect(ISSUE_DRAFT_COPY).toEqual({
      header: `New issue`,
      titlePlaceholder: `Issue title`,
      descriptionPlaceholder: `Add description...`,
      create: `Create`,
      discard: `Discard draft`,
      untitled: `Untitled draft`,
    })
  })

  it(`carries the autosave debounce`, () => {
    expect(ISSUE_DRAFT_AUTOSAVE_MS).toBe(800)
  })
})
