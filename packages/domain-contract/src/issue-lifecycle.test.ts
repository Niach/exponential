import { describe, expect, test } from "bun:test"
import { ISSUE_LIFECYCLE_CASES, issueLifecycleTarget } from "./issue-lifecycle"

describe(`issue-lifecycle.json`, () => {
  for (const entry of ISSUE_LIFECYCLE_CASES) {
    test(entry.name, () => {
      const { event, ...state } = entry.input
      expect(issueLifecycleTarget(event, state)).toBe(entry.expected)
    })
  }
})
