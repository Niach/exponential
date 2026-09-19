package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity

/**
 * EXP-897: starting a run on an issue that something else still BLOCKS.
 *
 * The launcher's third start mode: a blocked issue can either be started as
 * usual ("Start anyway") or as a STACKED PR — the branch is cut from the
 * blocker's PR branch and the pull request is based on it, so the diff shows
 * only this issue's own work and the run builds the blocker first if nobody
 * has.
 *
 * The rule and the copy are mirrored ×4 by name (web `lib/stack-start.ts`,
 * iOS `StackStart.swift`, desktop `chat_launch.rs`) and the same three tests
 * run on every client.
 */
object StackStart {
    /** The dialog's title. */
    const val BLOCKED_START_TITLE = "This issue is blocked"

    /** The secondary button: start the run without stacking. */
    const val START_ANYWAY_LABEL = "Start anyway"

    /** The primary button: cut from the blocker and base the PR on it. */
    const val STACKED_PR_LABEL = "Stacked PR"

    /** The body, around the blocker chips: prefix + chips + suffix. */
    const val BODY_PREFIX = "This issue is blocked by "
    const val BODY_SUFFIX = ". Start anyway, or start a stacked PR?"

    // EXP-980: the dialog also asks for a BATCH (blockers outside the picked
    // set, [IssueGraph.openBlockersOfSet]), draws the transitive chain as the
    // mini-graph, and never hides the stacked button: it is DISABLED with one
    // of the reason notes below. All byte-identical ×4.

    /** The title when two or more issues were picked. */
    const val BLOCKED_BATCH_TITLE = "Some of these issues are blocked"

    /** The batch body, above the graph. */
    const val BLOCKED_BATCH_BODY = "Open issues outside this batch block it. Start anyway?"

    /**
     * Why "Stacked PR" is disabled: the runner device lacks the `stacked-start`
     * capability (an older app or daemon).
     */
    const val STACK_NEEDS_UPDATE_NOTE = "Update Exponential on this device to start stacked PRs."

    /** Why "Stacked PR" is disabled for a batch. */
    const val STACK_SINGLE_ISSUE_NOTE = "A stacked PR starts one issue at a time."

    /** Why "Stacked PR" is disabled on a blocking cycle. */
    const val STACK_CYCLE_NOTE =
        "These issues block each other in a cycle. Remove one relation to stack them."

    /** Why the stacked start is off — [stackDisabledReason]'s vocabulary. */
    enum class StackDisabledReason { Cycle, Batch, Cap }

    /**
     * Why the stacked start is off, or null when it is on. One reason at a
     * time, the most fundamental first: a cycle can never stack, a batch never
     * does, a missing capability is fixed by an update.
     */
    fun stackDisabledReason(
        pickedCount: Int,
        canStack: Boolean,
        hasCycle: Boolean,
    ): StackDisabledReason? = when {
        hasCycle -> StackDisabledReason.Cycle
        pickedCount > 1 -> StackDisabledReason.Batch
        !canStack -> StackDisabledReason.Cap
        else -> null
    }

    fun stackDisabledNote(reason: StackDisabledReason): String = when (reason) {
        StackDisabledReason.Cycle -> STACK_CYCLE_NOTE
        StackDisabledReason.Batch -> STACK_SINGLE_ISSUE_NOTE
        StackDisabledReason.Cap -> STACK_NEEDS_UPDATE_NOTE
    }

    /**
     * The issues that still block [issueId].
     *
     * A stored relation is always the CANONICAL direction (`issue_id` blocks
     * `related_issue_id`), so MY blockers are the `blocks` rows whose
     * `related_issue_id` is me. A blocker whose issue row has not synced
     * (another team, a trashed board) is dropped rather than rendered as a
     * dangling id, and a blocker that is done, cancelled or a duplicate does
     * not block anything any more. Ordered by identifier so the chips read the
     * same on every client.
     */
    fun openBlockers(
        issueId: String,
        relations: List<IssueRelationEntity>,
        issues: List<IssueEntity>,
    ): List<IssueEntity> {
        val issuesById = issues.associateBy { it.id }
        return relations
            .asSequence()
            .filter { it.type == DomainContract.issueRelationTypeBlocks && it.relatedIssueId == issueId }
            .mapNotNull { issuesById[it.issueId] }
            .filter { !isClosed(it) }
            .distinctBy { it.id }
            .sortedBy { it.identifier }
            .toList()
    }

    /** The anchor statuses that retire a blocker (`CATEGORY_ANCHOR` parity). */
    private fun isClosed(issue: IssueEntity): Boolean =
        when (IssueStatus.fromWire(issue.status)) {
            IssueStatus.Done, IssueStatus.Cancelled, IssueStatus.Duplicate -> true
            else -> false
        }
}
