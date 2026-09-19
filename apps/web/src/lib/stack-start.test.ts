import { describe, expect, it } from "vitest"
import {
  BLOCKED_START_BODY_PREFIX,
  BLOCKED_START_BODY_SUFFIX,
  BLOCKED_START_TITLE,
  blockedStartBody,
  openBlockers,
  STACK_CYCLE_NOTE,
  STACK_NEEDS_UPDATE_NOTE,
  STACK_SINGLE_ISSUE_NOTE,
  stackDisabledNote,
  stackDisabledReason,
  START_ANYWAY_LABEL,
  STACKED_PR_LABEL,
} from "./stack-start"

// EXP-897 — the blocked-start rule. Every `it` name here is mirrored by iOS
// StackStartTests, Android StackStartTest and the desktop `blockers_of` tests;
// a change on one side without the others is a cross-client drift.

const issue = (id: string, status = `backlog`) => ({
  id,
  identifier: id.toUpperCase(),
  status,
})

/** `blocker` blocks `blocked` — the canonical direction (EXP-736). */
const blocks = (blocker: string, blocked: string) => ({
  type: `blocks`,
  issueId: blocker,
  relatedIssueId: blocked,
})

describe(`openBlockers`, () => {
  it(`counts only blocked-by relations`, () => {
    const issues = [issue(`me`), issue(`lower`), issue(`upper`)]
    const relations = [
      blocks(`lower`, `me`),
      // The other side: `me` blocks `upper`, which is not in `me`'s way.
      blocks(`me`, `upper`),
      // A related row is never a blocker.
      { type: `related`, issueId: `upper`, relatedIssueId: `me` },
    ]
    expect(openBlockers(`me`, relations, issues).map((row) => row.id)).toEqual([
      `lower`,
    ])
  })

  it(`drops a blocker that is done, cancelled or a duplicate`, () => {
    const issues = [
      issue(`me`),
      issue(`done`, `done`),
      issue(`cancelled`, `cancelled`),
      issue(`dupe`, `duplicate`),
      issue(`open`, `in_progress`),
    ]
    const relations = [
      blocks(`done`, `me`),
      blocks(`cancelled`, `me`),
      blocks(`dupe`, `me`),
      blocks(`open`, `me`),
    ]
    expect(openBlockers(`me`, relations, issues).map((row) => row.id)).toEqual([
      `open`,
    ])
  })

  it(`drops a blocker whose issue row is not synced`, () => {
    const issues = [issue(`me`), issue(`b`), issue(`a`)]
    const relations = [
      blocks(`gone`, `me`),
      blocks(`b`, `me`),
      blocks(`a`, `me`),
      // A duplicate row for the same blocker counts once.
      blocks(`a`, `me`),
    ]
    // …and the rest are ordered by identifier.
    expect(openBlockers(`me`, relations, issues).map((row) => row.id)).toEqual([
      `a`,
      `b`,
    ])
  })

  it(`keeps the dialog copy byte-identical`, () => {
    expect(BLOCKED_START_TITLE).toBe(`This issue is blocked`)
    expect(START_ANYWAY_LABEL).toBe(`Start anyway`)
    expect(STACKED_PR_LABEL).toBe(`Stacked PR`)
    expect(blockedStartBody([`ABC-12`, `ABC-13`])).toBe(
      `${BLOCKED_START_BODY_PREFIX}#ABC-12, #ABC-13${BLOCKED_START_BODY_SUFFIX}`
    )
  })
})

describe(`stackDisabledReason`, () => {
  it(`names one reason, the most fundamental first`, () => {
    expect(stackDisabledReason({ pickedCount: 1, canStack: true, hasCycle: false })).toBeNull()
    expect(stackDisabledReason({ pickedCount: 1, canStack: false, hasCycle: false })).toBe(`cap`)
    expect(stackDisabledReason({ pickedCount: 2, canStack: false, hasCycle: false })).toBe(`batch`)
    expect(stackDisabledReason({ pickedCount: 2, canStack: true, hasCycle: true })).toBe(`cycle`)
  })

  it(`has a note for every reason`, () => {
    expect(stackDisabledNote(`cap`)).toBe(STACK_NEEDS_UPDATE_NOTE)
    expect(stackDisabledNote(`batch`)).toBe(STACK_SINGLE_ISSUE_NOTE)
    expect(stackDisabledNote(`cycle`)).toBe(STACK_CYCLE_NOTE)
  })
})
