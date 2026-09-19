package com.exponential.app.ui.search

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.IssueSearch
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest

// Cross-board search (the Search tab), hybrid local + server:
//   - Fast path: the shared `IssueSearch` engine (EXP-892 — identifier before
//     title before description, one ranking ×4) over every board of the
//     active account (local Room data, instant).
//   - Augmentation: the server-side full-text `issues.search` (title +
//     description + comment text) fires on the same debounced query, one call
//     per team of the account, and appends whatever the local filter
//     missed. Server errors degrade silently to local-only — typing is never
//     blocked on the network.
// EXP-922: one flat relevance-ordered list (undone issues first), the same
// limit and the same copy as the other three clients.
// The empty-query state shows a search hint (assigned issues live on the
// "My Work" tab since EXP-58).

/**
 * EXP-922: ONE flat ranked row — the web sheet's and the desktop palette's
 * shape. The board rides ALONG the issue (it draws the row's sub-line) instead
 * of banding the list into per-board sections, so the four surfaces list the
 * same rows in the same single relevance order. [board] is null only while a
 * server hit's board has not synced.
 */
data class SearchResult(val issue: IssueEntity, val board: BoardEntity?)

data class SearchState(
    // The debounced query the current results were computed for; blank means
    // "show the idle search hint".
    val query: String = "",
    val results: List<SearchResult> = emptyList(),
)

/** EXP-922: the ONE limit every search surface on every client uses. */
private const val MAX_RESULTS = IssueSearch.DEFAULT_LIMIT

@OptIn(FlowPreview::class, ExperimentalCoroutinesApi::class)
@HiltViewModel
class SearchViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    private val issuesApi: IssuesApi,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    // Raw input updates on every keystroke (the field itself stays responsive
    // via local Compose state); the match recomputes ~250ms after typing stops.
    private val _query = MutableStateFlow("")
    fun setQuery(query: String) {
        _query.value = query
    }

    private val debouncedQuery = _query.debounce(250).distinctUntilChanged()

    private val issuesFlow = dbFlow.scopedQuery(emptyList<IssueEntity>()) { it.issueDao().observeAll() }
    private val boardsFlow = dbFlow.scopedQuery(emptyList<BoardEntity>()) { it.boardDao().observeAll() }

    /**
     * A server response pinned to the query it answered, so a slow response
     * can never be merged under a fresher query's results.
     */
    private data class ServerSearch(val query: String = "", val hits: List<SearchIssueHit> = emptyList())

    // Server-backed "search everything". There is no single active team —
    // this tab spans the whole account — so fan out one `issues.search` per
    // distinct team id of the synced boards (typically one or two) and
    // flatten. `transformLatest` cancels the in-flight round trip whenever the
    // debounced query (or account/team set) changes; per-call failures
    // collapse to "no extra hits".
    private val serverSearch: Flow<ServerSearch> = combine(
        auth.activeAccountId,
        boardsFlow.map { boards -> boards.map { it.teamId }.distinct().sorted() }.distinctUntilChanged(),
        debouncedQuery,
    ) { accountId, teamIds, query -> Triple(accountId, teamIds, query.trim()) }
        .distinctUntilChanged()
        .transformLatest { (accountId, teamIds, query) ->
            // Clear stale hits for the new query immediately (local-only view
            // renders while the round trip runs).
            emit(ServerSearch(query))
            if (accountId == null || query.isEmpty() || teamIds.isEmpty()) return@transformLatest
            val hits = coroutineScope {
                teamIds.map { teamId ->
                    async {
                        try {
                            issuesApi.search(accountId, teamId, query, limit = MAX_RESULTS)
                        } catch (e: CancellationException) {
                            throw e
                        } catch (_: Exception) {
                            // Offline / server error / stale membership: the
                            // local fast path already rendered — never surface.
                            emptyList()
                        }
                    }
                }.awaitAll()
            }.flatten()
            emit(ServerSearch(query, hits))
        }

    val state: StateFlow<SearchState> = combine(
        issuesFlow,
        boardsFlow,
        debouncedQuery,
        serverSearch,
    ) { issues, boards, query, server ->
        val trimmed = query.trim()
        if (trimmed.isEmpty()) {
            SearchState(query = "")
        } else {
            val boardsById = boards.associateBy { it.id }
            // Live boards only (the DAO already filters trashed boards).
            val localMatches = IssueSearch.rank(
                issues.filter { it.boardId in boardsById },
                trimmed,
                limit = MAX_RESULTS,
            )

            // Merge (the shared splice): local matches first, then the
            // server-found issues the local ranking missed (description /
            // comment hits), deduped by id in server relevance order. A hit
            // that exists in local Room renders as its live local row; an
            // unsynced hit renders from the returned fields (a placeholder
            // entity — the row only shows identifier/title/status/priority).
            val matches = if (server.query == trimmed && server.hits.isNotEmpty()) {
                val issuesById = issues.associateBy { it.id }
                IssueSearch.mergeServerHits(
                    localMatches,
                    server.hits,
                    limit = MAX_RESULTS,
                ) { hit ->
                    val local = issuesById[hit.id]
                    when {
                        local != null -> local.takeIf { it.boardId in boardsById }
                        hit.boardId in boardsById -> placeholderIssue(hit)
                        // No local board to group the row under (sync lag /
                        // trashed board) — drop it.
                        else -> null
                    }
                }
            } else {
                localMatches
            }

            // EXP-922: ONE flat list in the ranked order — no board bands.
            // Banding fought the ranking (a board's header jumped to wherever
            // its best hit landed and pulled its weaker hits up with it), so
            // the same query read differently here than on every other client.
            SearchState(
                query = trimmed,
                results = matches.map { SearchResult(it, boardsById[it.boardId]) },
            )
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SearchState())
}

/**
 * Render-only stand-in for a server hit that isn't in local Room yet.
 * [com.exponential.app.ui.issue.IssueRow] reads identifier/title/status/
 * priority/dueDate only, so the synthesized bookkeeping fields never show.
 */
private fun placeholderIssue(hit: SearchIssueHit): IssueEntity = IssueEntity(
    id = hit.id,
    boardId = hit.boardId,
    number = hit.identifier.substringAfterLast('-').toIntOrNull() ?: 0,
    identifier = hit.identifier,
    title = hit.title,
    status = hit.status,
    priority = hit.priority,
    creatorId = null,
    sortOrder = 0.0,
    createdAt = "",
    updatedAt = "",
)
