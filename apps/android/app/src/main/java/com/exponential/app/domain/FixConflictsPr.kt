package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity

// EXP-1233: the Fix merge conflicts builtin as the composer draws it once a
// pull request is picked — its own headline (the contract verb + the PR's
// issue chips) and a conflict CARD in place of the generic "Pull request"
// field. The pure half, so the rules are tests: web `resolveFixConflictsPr`
// + `branchLine` (`use-launch-composer.ts`, `@exp/ui` fix-conflicts-card),
// iOS `FixConflictsCard`, desktop `chat_screen::fix_conflicts_card`.

/**
 * `exp/APP-14 → master`, or the branch alone while the base is unknown; ""
 * without a branch (×4).
 */
fun branchLine(branch: String?, baseBranch: String?): String {
    if (branch.isNullOrEmpty()) return ""
    return if (baseBranch.isNullOrEmpty()) branch else "$branch → $baseBranch"
}

/** The picked pull request, resolved off the synced issue rows. */
data class FixConflictsPr(
    /** The representative issue id — the `pr` input's value. */
    val issueId: String,
    val prNumber: Int?,
    val branch: String?,
    val baseBranch: String?,
    /** Every synced issue the pull request links (a batch PR links several), by identifier. */
    val issues: List<IssueEntity>,
)

/**
 * The PR picked into the Fix merge conflicts builtin: null while nothing is
 * picked or the representative has not synced. The linked set is every
 * open-PR row sharing the representative's `prUrl`, sorted by identifier;
 * the representative alone without a `prUrl`.
 */
fun resolveFixConflictsPr(prIssueId: String?, issues: List<IssueEntity>): FixConflictsPr? {
    if (prIssueId.isNullOrEmpty()) return null
    val representative = issues.firstOrNull { it.id == prIssueId } ?: return null
    val url = representative.prUrl
    val linked = if (url.isNullOrEmpty()) {
        listOf(representative)
    } else {
        issues.filter { it.prUrl == url && it.prState == "open" }.sortedBy { it.identifier }
    }
    return FixConflictsPr(
        issueId = representative.id,
        prNumber = representative.prNumber,
        branch = representative.branch,
        baseBranch = representative.prBaseBranch,
        issues = linked.ifEmpty { listOf(representative) },
    )
}
