package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity

/**
 * What a run's Merge control actually merges (EXP-734, SLOP-3).
 *
 * Every run that opened a PR carries it on its OWN row
 * (`coding_sessions.pr_url/pr_number/pr_state`). An issue run merges through
 * its issue (`issues.mergePr` — merging there completes the issue); an
 * issue-less run (batch, chat, action) merges the PR on its row through
 * `codingSessions.mergePr`. Every surface that offers a merge keys its
 * in-flight / failure state on [key].
 */
sealed interface MergeTarget {
    /** Stable identity for the merging + mergeErrors maps a surface keeps. */
    val key: String

    /** Merge through an issue (`issues.mergePr`). */
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
 * merge (no PR, or one already merged/closed): an ISSUE run → its issue while
 * that issue's PR is open; an issue-less run → its own row's open PR.
 */
fun resolveMergeTarget(
    session: CodingSessionEntity,
    issue: IssueEntity?,
): MergeTarget? {
    if (session.issueId != null) {
        return issue
            ?.takeIf { it.prState == DomainContract.prStateOpen }
            ?.let { MergeTarget.Issue(it.id) }
    }
    if (session.hasOpenPr) return MergeTarget.Session(session.id)
    return null
}
