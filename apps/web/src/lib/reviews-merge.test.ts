import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/reviews-merge.json"
import {
  MERGE_LABEL,
  MERGE_STACK_LABEL,
  REVIEW_MERGES_THROUGH_WORKFLOW,
  REVIEW_MERGES_WITH_STACK,
  reviewRowMergeAction,
  reviewsMergeDisabledReason,
  reviewWorkflowStatus,
  workflowStatusByIssue,
  type ReviewMergeInput,
} from "./reviews-merge"

// EXP-1094: the Reviews row's merge control, replayed off the ONE contract
// fixture ×4.
describe(`reviews merge (contract fixture)`, () => {
  it(`locks the words`, () => {
    expect(MERGE_LABEL).toBe(fixture.labels.merge)
    expect(MERGE_STACK_LABEL).toBe(fixture.labels.mergeStack)
    expect(REVIEW_MERGES_WITH_STACK).toBe(fixture.reasons.mergesWithStack)
    expect(REVIEW_MERGES_THROUGH_WORKFLOW).toBe(fixture.reasons.mergesThroughWorkflow)
  })

  for (const entry of fixture.cases) {
    it(entry.name, () => {
      const input = entry.input as ReviewMergeInput
      expect(reviewRowMergeAction(input)).toBe(entry.action)
      expect(reviewsMergeDisabledReason(input)).toBe(entry.disabledReason)
    })
  }
})

describe(`reviewWorkflowStatus`, () => {
  const byIssue = workflowStatusByIssue(
    [
      { id: `w1`, status: `done` },
      { id: `w2`, status: `running` },
    ],
    [
      { workflowId: `w1`, issueId: `a`, memberIssueIds: [] },
      { workflowId: `w2`, issueId: `b`, memberIssueIds: [`c`, `a`] },
      { workflowId: `gone`, issueId: `d`, memberIssueIds: [] },
    ]
  )

  it(`reads a node's issue and its members, a live workflow first`, () => {
    expect(reviewWorkflowStatus([`b`], byIssue)).toBe(`running`)
    expect(reviewWorkflowStatus([`c`], byIssue)).toBe(`running`)
    expect(reviewWorkflowStatus([`a`], byIssue)).toBe(`running`)
  })

  it(`is null off every synced workflow`, () => {
    expect(reviewWorkflowStatus([`d`, `x`], byIssue)).toBeNull()
  })
})
