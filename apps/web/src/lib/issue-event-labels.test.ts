import { describe, expect, it } from "vitest"
import {
  issueEventActorFallback,
  issueEventPhrase,
  statusLabel,
} from "./issue-event-labels"
import { buildStatusOptions, resolveIssueStatus } from "./team-statuses"
import type { StatusResolvable } from "./team-statuses"

// A team that renamed its builtin In Progress to "Doing" and added a custom
// started row, the way desktop timeline.rs `status_display_name` sees it.
const TEAM_ROWS = buildStatusOptions([
  { id: `row-backlog`, name: `Backlog`, color: `#888888`, category: `backlog`, builtinKey: `backlog`, sortOrder: 0 },
  { id: `row-doing`, name: `Doing`, color: `#3b82f6`, category: `started`, builtinKey: `in_progress`, sortOrder: 1 },
  { id: `row-qa`, name: `QA`, color: `#a855f7`, category: `started`, builtinKey: null, sortOrder: 2 },
  { id: `row-done`, name: `Done`, color: `#22c55e`, category: `completed`, builtinKey: `done`, sortOrder: 3 },
])
const resolve = (issue: StatusResolvable) => resolveIssueStatus(issue, TEAM_ROWS)

describe(`statusLabel`, () => {
  it(`prefers the EXP-314 name snapshot`, () => {
    expect(statusLabel({ to: `in_progress`, toName: `Doing` }, `to`)).toBe(`Doing`)
    expect(statusLabel({ from: `backlog`, fromName: `Icebox` }, `from`)).toBe(`Icebox`)
  })

  it(`reads a name-less legacy anchor as the builtin row's display name`, () => {
    expect(statusLabel({ to: `in_review` }, `to`)).toBe(`In Review`)
    expect(statusLabel({ to: `in_review`, toName: null }, `to`)).toBe(`In Review`)
    expect(statusLabel({ to: `some_custom` }, `to`)).toBe(`some custom`)
  })

  it(`keeps the retired todo label`, () => {
    expect(statusLabel({ to: `todo` }, `to`)).toBe(`Todo`)
  })

  it(`resolves a name-less row through the team's status rows (F47)`, () => {
    // The statusId's row wins, whatever the legacy anchor says.
    expect(statusLabel({ to: `in_progress`, toStatusId: `row-qa` }, `to`, resolve)).toBe(`QA`)
    expect(statusLabel({ from: `in_progress`, fromStatusId: `row-doing` }, `from`, resolve)).toBe(`Doing`)
    // A legacy enum-only side reads as the team's row for that anchor.
    expect(statusLabel({ to: `in_progress` }, `to`, resolve)).toBe(`Doing`)
    expect(statusLabel({ to: `in_progress`, toName: `` }, `to`, resolve)).toBe(`Doing`)
    // A deleted row's id falls through to the anchor's team row.
    expect(statusLabel({ to: `in_progress`, toStatusId: `row-gone` }, `to`, resolve)).toBe(`Doing`)
    // An anchor the team has no row for keeps the constructed builtin label.
    expect(statusLabel({ to: `in_review` }, `to`, resolve)).toBe(`In Review`)
    // Wire values no row answers to: retired label, then the munge.
    expect(statusLabel({ to: `todo` }, `to`, resolve)).toBe(`Todo`)
    expect(statusLabel({ to: `some_custom` }, `to`, resolve)).toBe(`some custom`)
    // The name snapshot still wins over everything.
    expect(statusLabel({ to: `in_progress`, toStatusId: `row-qa`, toName: `Snap` }, `to`, resolve)).toBe(`Snap`)
    // An id-only side whose row is gone stays empty (the row renders the resolved option).
    expect(statusLabel({ toStatusId: `row-gone` }, `to`, resolve)).toBe(``)
    expect(
      issueEventPhrase(`status_changed`, { from: `backlog`, to: `in_progress` }, resolve)
    ).toBe(`changed status from Backlog to Doing`)
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
    expect(
      issueEventPhrase(`status_changed`, { from: `backlog`, to: `in_progress` })
    ).toBe(`changed status from Backlog to In Progress`)
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
