import { describe, expect, test } from "bun:test"
import {
  ISSUE_LIFECYCLE_CASES,
  ISSUE_PURGE_DUE_CASES,
  ISSUE_TRASH_ACTION_CASES,
  ISSUE_TRASH_CAPTION_CASES,
  PR_LINK_CASES,
  SQUASH_TITLE_CASES,
  isIssuePurgeDue,
  issueLifecycleTarget,
  issueTrashActions,
  issueTrashCaption,
  issueTrashRowText,
  prLinks,
  squashTitle,
} from "./issue-lifecycle"

describe(`issue-lifecycle.json`, () => {
  for (const entry of ISSUE_LIFECYCLE_CASES) {
    test(entry.name, () => {
      const { event, ...state } = entry.input
      expect(issueLifecycleTarget(event, state)).toBe(entry.expected)
    })
  }
})

describe(`issue-lifecycle.json links`, () => {
  for (const entry of PR_LINK_CASES) {
    test(entry.name, () => {
      expect(prLinks(entry.input)).toEqual(entry.expected)
    })
  }
})

describe(`issue-lifecycle.json squashTitle`, () => {
  for (const entry of SQUASH_TITLE_CASES) {
    test(entry.name, () => {
      const { identifier, title, number } = entry.input
      expect(squashTitle(identifier, title, number)).toBe(entry.expected)
    })
  }
})

describe(`issue-lifecycle.json trash (#EXP-1797)`, () => {
  for (const entry of ISSUE_TRASH_CAPTION_CASES) {
    test(`caption: ${entry.name}`, () => {
      expect(issueTrashCaption(entry.input.purgeAt, new Date(entry.input.now))).toBe(entry.expected)
    })
  }
  for (const entry of ISSUE_TRASH_ACTION_CASES) {
    test(`actions: ${entry.name}`, () => {
      expect(issueTrashActions(entry.input.role)).toEqual(entry.expected as (`restore` | `purge`)[])
    })
  }
  for (const entry of ISSUE_PURGE_DUE_CASES) {
    test(`purge due: ${entry.name}`, () => {
      expect(isIssuePurgeDue(entry.input.deletedAt, new Date(entry.input.now))).toBe(entry.expected)
    })
  }
  test(`row text`, () => {
    expect(issueTrashRowText(`EXP-12`, `Fix the strip`)).toBe(`EXP-12 Fix the strip`)
  })
})
