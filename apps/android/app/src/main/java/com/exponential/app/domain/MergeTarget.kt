package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity

/**
 * What a run's Merge control actually merges (EXP-734).
 *
 * Until now every merge went through an ISSUE — the run's own issue, or, for
 * a batch run, the representative issue of its combined PR (EXP-535). An
 * action or chat run has neither, and since EXP-626 it can still open a PR of
 * its own (`exponential_pr_open({repositoryId, head})`); the server records
 * that PR on the SESSION row and merges it through `codingSessions.mergePr`.
 * So a merge control now resolves to one of two targets, and every surface
 * that offers one keys its in-flight / failure state on [key].
 */
sealed interface MergeTarget {
    /** Stable identity for the merging + mergeErrors maps a surface keeps. */
    val key: String

    /** Merge through an issue (`issues.mergePr`) — single or batch PR. */
    data class Issue(val issueId: String) : MergeTarget {
        override val key: String get() = issueId
    }

    /** Merge the run's own PR (`codingSessions.mergePr`) — no issue is linked. */
    data class Session(val sessionId: String) : MergeTarget {
        override val key: String get() = "session:$sessionId"
    }
}

/** EXP-734: the run carries a pull request of its OWN that is still open. */
val CodingSessionEntity.hasOpenPr: Boolean
    get() = prState == DomainContract.prStateOpen && !prUrl.isNullOrEmpty()

/**
 * The merge control's target for one run, or null when there is nothing to
 * merge (no PR, or one already merged/closed).
 *
 * Order matters: an ISSUE-linked run always merges through its issue (the
 * server refuses `codingSessions.mergePr` on one — merging there completes the
 * issue), a batch run through the representative issue [batchPrIssue] the
 * caller resolved from the branch (EXP-535), and only an issueless run with no
 * batch resolution falls back to its own recorded PR.
 */
fun resolveMergeTarget(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    batchPrIssue: IssueEntity? = null,
): MergeTarget? {
    if (session.issueId != null) {
        return issue
            ?.takeIf { it.prState == DomainContract.prStateOpen }
            ?.let { MergeTarget.Issue(it.id) }
    }
    batchPrIssue?.takeIf { it.prState == DomainContract.prStateOpen }?.let {
        return MergeTarget.Issue(it.id)
    }
    if (session.hasOpenPr) return MergeTarget.Session(session.id)
    return null
}
