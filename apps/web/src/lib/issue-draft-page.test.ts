import { describe, expect, it } from "vitest"
import {
  ISSUE_DRAFT_AUTOSAVE_MS,
  ISSUE_DRAFT_COPY,
  ISSUE_DRAFT_DISCARDED_GRACE_MS,
  canCreateDraft,
  draftExitPrompt,
  draftOriginFrom,
  newDraftNavigation,
  parseIssueDraftSearch,
} from "./issue-draft-page"
import { defaultStatusOptions } from "./team-statuses"

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
      untitled: `Untitled draft`,
      leave: {
        title: `Save this issue as a draft?`,
        discard: `Discard`,
        keep: `Save draft`,
        create: `Create issue`,
      },
      // EXP-1231: the toast when another client discarded the draft.
      discardedElsewhere: `Draft discarded elsewhere`,
    })
  })

  it(`carries the autosave debounce`, () => {
    expect(ISSUE_DRAFT_AUTOSAVE_MS).toBe(800)
  })

  it(`carries the discarded-elsewhere grace (EXP-1231)`, () => {
    expect(ISSUE_DRAFT_DISCARDED_GRACE_MS).toBe(3000)
  })
})

describe(`newDraftNavigation`, () => {
  it(`mints a draft id per call and carries board, status and origin`, () => {
    const status = { ...defaultStatusOptions()[0], id: `s-1` }
    const first = newDraftNavigation({
      teamSlug: `acme`,
      boardId: `b-1`,
      status,
      from: `board:web`,
    })
    const second = newDraftNavigation({ teamSlug: `acme` })
    expect(first.to).toBe(`/t/$teamSlug/drafts/$draftId`)
    expect(first.params.teamSlug).toBe(`acme`)
    expect(first.params.draftId).toMatch(/^[0-9a-f-]{36}$/)
    expect(second.params.draftId).not.toBe(first.params.draftId)
    expect(first.search).toEqual({
      board: `b-1`,
      status: `s-1`,
      from: `board:web`,
    })
    expect(second.search).toEqual({})
  })

  it(`never carries a constructed fallback status`, () => {
    const fallback = defaultStatusOptions()[0]
    expect(
      newDraftNavigation({ teamSlug: `acme`, status: fallback }).search
    ).toEqual({})
  })

  it(`parses only non-empty string params`, () => {
    expect(
      parseIssueDraftSearch({ board: `b`, status: ``, from: 3, x: `y` })
    ).toEqual({ board: `b`, status: undefined, from: undefined })
  })
})

// EXP-1212: a draft with content never goes silently.
describe(`draftExitPrompt`, () => {
  // EXP-1247: no `×` discard trigger and no discard confirm any more.
  it(`asks only when the draft has content`, () => {
    expect(draftExitPrompt(`leave`, true)).toBe(`leave`)
    expect(draftExitPrompt(`leave`, false)).toBe(`none`)
    expect(`discard` in ISSUE_DRAFT_COPY).toBe(false)
    expect(`discardConfirm` in ISSUE_DRAFT_COPY).toBe(false)
  })

  it(`enables Create only with a title and nothing in flight`, () => {
    const idle = { creating: false, uploading: false }
    expect(canCreateDraft({ title: `Fix it`, ...idle })).toBe(true)
    expect(canCreateDraft({ title: `   `, ...idle })).toBe(false)
    expect(
      canCreateDraft({ title: `Fix it`, creating: true, uploading: false })
    ).toBe(false)
    expect(
      canCreateDraft({ title: `Fix it`, creating: false, uploading: true })
    ).toBe(false)
  })
})

describe(`draftOriginFrom`, () => {
  it(`hands on the list in force where New issue was tapped`, () => {
    expect(draftOriginFrom(`/t/acme/boards/web`, undefined)).toBe(`board:web`)
    expect(draftOriginFrom(`/t/acme/inbox`, undefined)).toBe(`inbox`)
    expect(
      draftOriginFrom(`/t/acme/boards/web/issues/WEB-1`, `board:api`)
    ).toBe(`board:api`)
    expect(draftOriginFrom(`/t/acme/devices`, undefined)).toBeUndefined()
  })
})
