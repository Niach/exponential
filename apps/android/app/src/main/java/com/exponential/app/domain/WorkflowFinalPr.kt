package com.exponential.app.domain

/**
 * EXP-1072: how a workflow's ONE final pull request is NAMED wherever a PR is
 * shown — Reviews, the `pr` input picker, the fix-conflicts prompt. It is the
 * workflow's OWN linked PR, never an unlinked one. Same strings as web
 * `lib/workflow-final-pr-identity.ts`, iOS `WorkflowFinalPr` and desktop
 * `domain::workflow_final_pr`.
 */
object WorkflowFinalPr {
    /** The final PR's title on GitHub, and the `pr` input's "identifier" for it. */
    fun identifier(name: String): String = "Workflow: $name"

    /** The `pr` picker's label: `#829 · Workflow: EXP-996 +5`, no number → the identifier. */
    fun pickLabel(number: Int?, name: String): String =
        if (number == null) identifier(name) else "#$number · ${identifier(name)}"

    /** The Reviews queue's entry key for a workflow's final PR. */
    fun reviewKey(workflowId: String): String = "workflow:$workflowId"
}
