import { describe, expect, test } from "bun:test"
import {
  ISSUE_LIFECYCLE_CASES,
  issueLifecycleTarget,
  PART_PR_CASES,
  PARTS_LANDED_CASES,
  partsLanded,
  pullRequestPart,
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
      expect(pullRequestPart(entry.title)).toEqual(entry.expected)
    })
  }
  for (const entry of PARTS_LANDED_CASES) {
    test(entry.name, () => {
      expect(partsLanded(entry.of, entry.parts)).toBe(entry.expected)
    })
  }
})
