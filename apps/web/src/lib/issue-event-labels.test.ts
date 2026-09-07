import { describe, expect, it } from "vitest"
import {
  issueEventActorFallback,
  issueEventPhrase,
  statusLabel,
} from "./issue-event-labels"

describe(`statusLabel`, () => {
  it(`prefers the EXP-314 name snapshot`, () => {
    expect(statusLabel({ to: `in_progress`, toName: `Doing` }, `to`)).toBe(`Doing`)
    expect(statusLabel({ from: `backlog`, fromName: `Icebox` }, `from`)).toBe(`Icebox`)
  })

  it(`munges the legacy enum anchor when no name was recorded`, () => {
    expect(statusLabel({ to: `in_review` }, `to`)).toBe(`in review`)
    expect(statusLabel({ to: `in_review`, toName: null }, `to`)).toBe(`in review`)
  })

  it(`keeps the retired todo label`, () => {
    expect(statusLabel({ to: `todo` }, `to`)).toBe(`Todo`)
  })

  it(`is empty for a payload without either field`, () => {
    // Post-EXP-314 rows whose status row was deleted carry only ids.
    expect(statusLabel({ toStatusId: `abc` }, `to`)).toBe(``)
  })
})

describe(`issueEventPhrase`, () => {
  it(`never renders the "?" placeholder for a modern status change`, () => {
    expect(
      issueEventPhrase(`status_changed`, { toStatusId: `x`, toName: `Ready` })
    ).toBe(`changed status to Ready`)
    expect(issueEventPhrase(`status_changed`, { toStatusId: `x` })).toBe(
      `changed status`
    )
    expect(issueEventPhrase(`status_changed`, null)).toBe(`changed status`)
  })

  it(`phrases the EXP-530 and EXP-736 event types`, () => {
    expect(issueEventPhrase(`created`, {})).toBe(`created`)
    expect(issueEventPhrase(`priority_changed`, { to: `urgent` })).toBe(
      `set priority to Urgent`
    )
    expect(issueEventPhrase(`board_moved`, {})).toBe(`moved to another board`)
    expect(
      issueEventPhrase(`relation_added`, { relatedIdentifier: `EXP-12` })
    ).toBe(`linked EXP-12`)
    expect(issueEventPhrase(`relation_removed`, {})).toBe(`unlinked an issue`)
  })

  it(`falls back to the munged type for unknown kinds`, () => {
    expect(issueEventPhrase(`something_new`, {})).toBe(`something new`)
  })
})

describe(`issueEventActorFallback`, () => {
  it(`names the widget for widget-filed rows`, () => {
    expect(issueEventActorFallback({ source: `widget` })).toBe(`Widget`)
    expect(issueEventActorFallback({ source: `user` })).toBe(`Someone`)
    expect(issueEventActorFallback(null)).toBe(`Someone`)
  })
})
