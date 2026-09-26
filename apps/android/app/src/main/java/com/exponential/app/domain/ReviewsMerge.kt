package com.exponential.app.domain

/**
 * EXP-1094: the ONE merge control a Reviews row carries, ×4 (web
 * `lib/reviews-merge.ts`, desktop `domain::reviews_merge`, iOS
 * `ReviewsMerge.swift`), locked by `reviews-merge.json`.
 *
 * First match wins:
 *   1. a workflow's FINAL PR row merges only while that PR is open;
 *   2. a workflow NODE PR (its workflow running or paused) merges through the
 *      workflow, never from the row;
 *   3. a stack's bottom row merges the whole stack; an upper member merges
 *      with it;
 *   4. anything else is a plain Merge.
 */
object ReviewsMerge {
    const val MERGE_LABEL = "Merge"
    const val MERGE_STACK_LABEL = PrStack.MERGE_STACK_LABEL

    /** An upper stack member's quiet caption. */
    const val MERGES_WITH_STACK = "merges with its stack"

    /** A workflow node PR's quiet caption. */
    const val MERGES_THROUGH_WORKFLOW = "merges through the workflow"

    fun reviewRowMergeAction(input: ReviewMergeInput): ReviewMergeAction = when {
        input.finalPr ->
            if (input.finalPrState == DomainContract.prStateOpen) ReviewMergeAction.MERGE else ReviewMergeAction.NONE
        liveWorkflow(input.workflowStatus) -> ReviewMergeAction.NONE
        input.stack == ReviewStackPosition.BOTTOM -> ReviewMergeAction.MERGE_STACK
        input.stack == ReviewStackPosition.UPPER -> ReviewMergeAction.NONE
        else -> ReviewMergeAction.MERGE
    }

    /** Why a row offers no merge control; null when it offers one or has
     *  nothing to say (a final PR that is not open). */
    fun reviewsMergeDisabledReason(input: ReviewMergeInput): String? = when {
        input.finalPr -> null
        liveWorkflow(input.workflowStatus) -> MERGES_THROUGH_WORKFLOW
        input.stack == ReviewStackPosition.UPPER -> MERGES_WITH_STACK
        else -> null
    }

    /**
     * Issue id → the status of the workflow whose node covers it (the node's
     * `issueId` or a member issue). A live workflow wins over a finished one
     * covering the same issue.
     */
    fun workflowStatusByIssue(
        workflows: List<Pair<String, String>>,
        nodes: List<Pair<String, List<String>>>,
    ): Map<String, String> {
        val statusById = workflows.toMap()
        val byIssue = HashMap<String, String>()
        for ((workflowId, issueIds) in nodes) {
            val status = statusById[workflowId] ?: continue
            for (issueId in issueIds) {
                val prior = byIssue[issueId]
                if (prior == null || (liveWorkflow(status) && !liveWorkflow(prior))) byIssue[issueId] = status
            }
        }
        return byIssue
    }

    /** A PR's `workflowStatus` over its linked issues: a live workflow first,
     *  else any covering one, else null. */
    fun reviewWorkflowStatus(issueIds: List<String>, byIssue: Map<String, String>): String? {
        val statuses = issueIds.mapNotNull(byIssue::get)
        return statuses.firstOrNull(::liveWorkflow) ?: statuses.firstOrNull()
    }

    private fun liveWorkflow(status: String?): Boolean =
        status == DomainContract.wfStatusRunning || status == DomainContract.wfStatusPaused
}

/** Where a row sits in its PR stack: [BOTTOM] = the root row that merges the
 *  whole stack, [UPPER] = any member above it. */
enum class ReviewStackPosition(val wire: String) {
    NONE("none"),
    BOTTOM("bottom"),
    UPPER("upper"),
}

enum class ReviewMergeAction(val wire: String) {
    MERGE("merge"),
    MERGE_STACK("merge_stack"),
    NONE("none"),
}

data class ReviewMergeInput(
    val stack: ReviewStackPosition = ReviewStackPosition.NONE,
    /** The status of the workflow whose node covers this PR's issue, else null. */
    val workflowStatus: String? = null,
    /** The row IS a workflow's final PR. */
    val finalPr: Boolean = false,
    /** The final PR's state (`open` | `closed` | `merged`); null off final rows. */
    val finalPrState: String? = null,
)
