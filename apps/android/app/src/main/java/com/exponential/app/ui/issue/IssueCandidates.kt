package com.exponential.app.ui.issue

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueSearch
import com.exponential.app.ui.components.rememberServerSearchHits

/**
 * The ranked "pick one of the team's other issues" pool — the second stage of
 * both issue pickers (EXP-736): "Duplicate of…" and every relation pick.
 *
 * EXP-1021 turned this from a list COMPOSABLE into the ranking alone: the rows
 * are the shared picker's now (`IssuePicker`), so all that is left here is the
 * order, which is the one thing those two surfaces must not each invent.
 */
@Composable
fun rememberIssueCandidates(
    candidates: List<IssueEntity>,
    query: String,
    /**
     * EXP-892: the server-side full-text search, when the host ViewModel has
     * the API. Its hits only ever REORDER what is already here — this pool is
     * a subset of the team's issues (the current issue and its existing
     * relations are out), so a hit outside it is dropped rather than widening
     * the picker. Null = local-only.
     */
    searchServer: (suspend (String) -> List<SearchIssueHit>)? = null,
): List<IssueEntity> {
    // The shared engine, ranked identically to the Search tab and the `#`
    // menu; an empty query lists the newest-created first.
    val hits = rememberServerSearchHits(query.takeIf { it.isNotBlank() }, searchServer)
    return remember(candidates, query, hits) {
        val local = IssueSearch.rank(candidates, query, limit = CANDIDATE_LIMIT)
        if (hits.isEmpty()) {
            local
        } else {
            val byId = candidates.associateBy { it.id }
            IssueSearch.mergeServerHits(local, hits, limit = CANDIDATE_LIMIT) { hit -> byId[hit.id] }
        }
    }
}

/** The picker never lists more than this, ranked (the web picker's cap). */
private const val CANDIDATE_LIMIT = 50
