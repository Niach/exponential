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

export function reviewRowMergeAction(input: ReviewMergeInput): ReviewMergeAction {
  if (input.finalPr) return input.finalPrState === `open` ? `merge` : `none`
  if (liveWorkflow(input.workflowStatus)) return `none`
  if (input.stack === `bottom`) return `merge_stack`
  if (input.stack === `upper`) return `none`
  return `merge`
}

/** Why a row offers no merge control; null when it offers one or has nothing
 *  to say (a final PR that is not open). */
export function reviewsMergeDisabledReason(input: ReviewMergeInput): string | null {
  if (input.finalPr) return null
  if (liveWorkflow(input.workflowStatus)) return REVIEW_MERGES_THROUGH_WORKFLOW
  if (input.stack === `upper`) return REVIEW_MERGES_WITH_STACK
  return null
}

function liveWorkflow(status: string | null | undefined): boolean {
  return status === `running` || status === `paused`
}

/** Issue id → the status of the workflow whose node covers it (the node's
 *  `issueId` or a `memberIssueIds` entry). A live workflow wins over a
 *  finished one covering the same issue. */
export function workflowStatusByIssue(
  workflows: readonly { id: string; status: string }[],
  nodes: readonly { workflowId: string; issueId: string; memberIssueIds?: readonly string[] | null }[]
): Map<string, string> {
  const statusById = new Map(workflows.map((row) => [row.id, row.status]))
  const byIssue = new Map<string, string>()
  for (const node of nodes) {
    const status = statusById.get(node.workflowId)
    if (!status) continue
    for (const issueId of [node.issueId, ...(node.memberIssueIds ?? [])]) {
      if (!byIssue.has(issueId) || (liveWorkflow(status) && !liveWorkflow(byIssue.get(issueId)))) {
        byIssue.set(issueId, status)
      }
    }
  }
  return byIssue
}

/** A PR's `workflowStatus` input over its linked issues: a live workflow
 *  first, else any covering one, else null. */
export function reviewWorkflowStatus(
  issueIds: readonly string[],
  byIssue: ReadonlyMap<string, string>
): string | null {
  const statuses = issueIds
    .map((id) => byIssue.get(id))
    .filter((status): status is string => status !== undefined)
  return statuses.find(liveWorkflow) ?? statuses[0] ?? null
}
