package com.exponential.app.ui.session

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.ISSUE_SYNCING_TITLE
import com.exponential.app.domain.batchRunName
import com.exponential.app.domain.pastRunTitle

// EXP-688: a coding session's IDENTITY: the identifier and title helpers
// every session row (SessionRow, WorkScreen) reads.

/** EXP-874: the issue shortcode for an issue run; EXP-876: a batch's
 *  `EXP-874 +2`, off the issues it covers ([batchIssues]; empty on a surface
 *  that joins none). An action/chat run — and an issue run whose issue hasn't
 *  synced — prints no identifier. */
internal fun sessionRowIdentifier(
    issue: IssueEntity?,
    session: CodingSessionEntity? = null,
    batchIssues: List<IssueEntity> = emptyList(),
): String? {
    if (issue != null) return issue.identifier
    if (session == null || session.issueId != null || session.actionName != null) return null
    return batchRunName(session, batchIssues).identifier
}

/**
 * What the run is about: the issue's title, an action run's `action_name`
 * snapshot (EXP-253), a chat run's own auto-title (EXP-905), else the batch's
 * own name (EXP-876: its first covered issue's title, else "Batch run") — and,
 * for an issue-scoped session whose issue genuinely hasn't landed yet,
 * [ISSUE_SYNCING_TITLE].
 *
 * EXP-968: the live rows and the Recent ones are named by the SAME rule —
 * this used to carry a second copy of it that said "Issue not synced yet"
 * where every other client (and the Agent page one line below) said
 * "Issue syncing…".
 */
internal fun sessionRowTitle(
    session: CodingSessionEntity,
    issue: IssueEntity?,
    batchIssues: List<IssueEntity> = emptyList(),
): String = pastRunTitle(session, issue, batchIssues)
