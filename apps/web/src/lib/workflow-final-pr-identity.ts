// EXP-1072: how a workflow's ONE final pull request is NAMED wherever a PR
// is shown — Reviews ×4, the `pr` input pickers, the fix-conflicts prompt.
// Pure and dependency-free so both the server routers and the client import
// it; the natives carry the same strings (`WorkflowFinalPr` on iOS/Android,
// `domain::workflow_final_pr` on the desktop).

/** The final PR's title on GitHub, and the `pr` input's "identifier" for it. */
export function workflowFinalPrIdentifier(name: string): string {
  return `Workflow: ${name}`
}

/** The Reviews queue's entry key for a workflow's final PR. */
export function workflowReviewKey(workflowId: string): string {
  return `workflow:${workflowId}`
}
