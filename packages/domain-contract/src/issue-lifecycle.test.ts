import { describe, expect, test } from "bun:test"
import {
  ISSUE_LIFECYCLE_CASES,
  PR_LINK_CASES,
  SQUASH_TITLE_CASES,
  issueLifecycleTarget,
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
