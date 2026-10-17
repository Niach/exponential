import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-draft.json"
import {
  ISSUE_DRAFT_AUTOSAVE_MS,
  ISSUE_DRAFT_COPY,
  ISSUE_DRAFT_DISCARDED_GRACE_MS,
  canCreateDraft,
  draftExitPrompt,
  draftLandingNavigation,
  draftOriginFrom,
  inboxDotLit,
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

// EXP-1212: a draft with content never goes silently. EXP-1272: a reopened
// one saves and leaves (`keep`); only a fresh New issue asks.
describe(`draftExitPrompt`, () => {
  for (const row of fixture.exitPrompt) {
    it(row.case, () => {
      expect(draftExitPrompt(row.hasContent, row.reopened)).toBe(row.prompt)
    })
  }

  // EXP-1247: no `×` discard trigger and no discard confirm any more.
  it(`has no discard copy outside the leave dialog`, () => {
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

// EXP-1272: the phone Inbox dot = unread OR a draft in the team.
describe(`inboxDotLit`, () => {
  for (const row of fixture.inboxDot) {
    it(row.case, () => {
      expect(inboxDotLit(row.unread, row.drafts)).toBe(row.lit)
    })
  }
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

// EXP-1284 (review T11): a thread draft lands back on its thread with the new
// issue's tab selected; any other draft lands on the issue, keeping ?from=.
describe(`draftLandingNavigation`, () => {
  it(`lands a thread draft on the thread, the new issue selected`, () => {
    expect(
      draftLandingNavigation({
        teamSlug: `acme`,
        threadId: `th1`,
        boardSlug: `web`,
        issueId: `iss-9`,
        identifier: `APP-9`,
        from: `inbox`,
      })
    ).toEqual({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `th1` },
      search: { tab: `issue`, issue: `iss-9`, from: `inbox` },
    })
  })

  it(`lands any other draft on the issue page`, () => {
    expect(
      draftLandingNavigation({
        teamSlug: `acme`,
        threadId: null,
        boardSlug: `web`,
        issueId: `iss-9`,
        identifier: `APP-9`,
        from: `inbox`,
      })
    ).toEqual({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: `acme`, boardSlug: `web`, issueIdentifier: `APP-9` },
      search: { from: `inbox` },
    })
  })
})
