package com.exponential.app.ui.markdown

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import com.exponential.app.ui.components.rememberServerSearchHits

/**
 * EXP-892 — the `#` autocomplete's rows: the team's locally synced issues
 * ranked by the shared [com.exponential.app.domain.IssueSearch] engine, with
 * the server's full-text hits (title + description + COMMENT bodies) spliced
 * in behind them once typing has settled and the round trip has landed.
 *
 * A handler with no [IssueRefHandler.searchServer] (a host that cannot be
 * asynchronous) simply stays local-only — which is what the menu always was.
 *
 * [query] is the `#` query at the caret, or null while the menu is shut.
 */
@Composable
internal fun rememberIssueRefCandidates(
    handler: IssueRefHandler?,
    query: String?,
    limit: Int = ISSUE_REF_MENU_LIMIT,
): List<IssueRefTarget> {
    val hits = rememberServerSearchHits(query, handler?.searchServer)
    return remember(handler, query, hits, limit) {
        if (handler == null || query == null) {
            emptyList()
        } else {
            handler.searchWith(query, hits, limit)
        }
    }
}
