// EXP-1094: the ONE merge control a Reviews row carries, ×4 (desktop
// `domain::reviews_merge`, iOS `ReviewsMerge.swift`, Android
// `ReviewsMerge.kt`), locked by
// `@exp/domain-contract/fixtures/reviews-merge.json`.
//
// Rules, first match wins:
//   1. a workflow's FINAL PR row merges only while that PR is open;
//   2. a workflow NODE PR (its workflow running or paused) merges through the
//      workflow, never from the row;
//   3. a stack's bottom row merges the whole stack; an upper member merges
//      with it;
//   4. anything else is a plain Merge.
import { MERGE_STACK_LABEL } from "@/lib/pr-stack"

export { MERGE_STACK_LABEL }

/** Where a row sits in its PR stack: `bottom` = the root row that merges the
 *  whole stack, `upper` = any member above it. */
export type ReviewStackPosition = `none` | `bottom` | `upper`

export interface ReviewMergeInput {
  stack: ReviewStackPosition
  /** The status of the workflow whose node covers this PR's issue, else null. */
  workflowStatus: string | null
  /** The row IS a workflow's final PR. */
  finalPr: boolean
  /** The final PR's state (`open` | `closed` | `merged`); null off final rows. */
  finalPrState: string | null
}

export type ReviewMergeAction = `merge` | `merge_stack` | `none`

export const MERGE_LABEL = `Merge`
/** An upper stack member's quiet caption. */
export const REVIEW_MERGES_WITH_STACK = `merges with its stack`
/** A workflow node PR's quiet caption. */
export const REVIEW_MERGES_THROUGH_WORKFLOW = `merges through the workflow`

function workflowOwnsMerge(status: string | null): boolean {
  return status === `running` || status === `paused`
}

export function reviewRowMergeAction(input: ReviewMergeInput): ReviewMergeAction {
  if (input.finalPr) return input.finalPrState === `open` ? `merge` : `none`
  if (workflowOwnsMerge(input.workflowStatus)) return `none`
  if (input.stack === `bottom`) return `merge_stack`
  if (input.stack === `upper`) return `none`
  return `merge`
}

/** Why a row offers no merge control; null when it offers one or has nothing
 *  to say (a final PR that is not open). */
export function reviewsMergeDisabledReason(input: ReviewMergeInput): string | null {
  if (input.finalPr) return null
  if (workflowOwnsMerge(input.workflowStatus)) return REVIEW_MERGES_THROUGH_WORKFLOW
  if (input.stack === `upper`) return REVIEW_MERGES_WITH_STACK
  return null
}
