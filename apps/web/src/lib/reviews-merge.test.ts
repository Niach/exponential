import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/reviews-merge.json"
import {
  MERGE_LABEL,
  MERGE_STACK_LABEL,
  REVIEW_MERGES_THROUGH_WORKFLOW,
  REVIEW_MERGES_WITH_STACK,
  reviewRowMergeAction,
  reviewsMergeDisabledReason,
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
