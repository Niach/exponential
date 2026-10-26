import { describe, expect, test } from "bun:test"
import {
  ISSUE_LIFECYCLE_CASES,
  isPartPullRequest,
  issueLifecycleTarget,
  PART_PR_CASES,
} from "./issue-lifecycle"

describe(`issue-lifecycle.json`, () => {
  for (const entry of ISSUE_LIFECYCLE_CASES) {
    test(entry.name, () => {
      const { event, ...state } = entry.input
      expect(issueLifecycleTarget(event, state)).toBe(entry.expected)
    })
  }
})

describe(`issue-lifecycle.json partPr`, () => {
  for (const entry of PART_PR_CASES) {
    test(entry.name, () => {
      expect(isPartPullRequest(entry.title)).toBe(entry.expected)
    })
  }
})
