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
//   - Fast path: a pure client-side substring match over identifier + title
//     across every board of the active account (local Room data, instant).
//   - Augmentation: the server-side full-text `issues.search` (title +
//     description + comment text) fires on the same debounced query, one call
//     per team of the account, and appends whatever the local filter
//     missed. Server errors degrade silently to local-only — typing is never
//     blocked on the network.
// The empty-query state shows a search hint (assigned issues live on the
// "My Work" tab since EXP-58).

/** Results under one board header, most recently updated board first. */
data class SearchResultGroup(val board: BoardEntity, val issues: List<IssueEntity>)

data class SearchState(
    // The debounced query the current groups were computed for; blank means
    // "show the idle search hint".
    val query: String = "",
    val groups: List<SearchResultGroup> = emptyList(),
)

private const val MAX_RESULTS = 50

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
                            issuesApi.search(accountId, teamId, query)
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
            // Live boards only (the DAO already filters archived boards);
            // archived issues are excluded here — observeAll includes them.
            val localMatches = issues.asSequence()
                .filter { it.archivedAt == null && it.boardId in boardsById }
                .filter {
                    it.title.contains(trimmed, ignoreCase = true) ||
                        it.identifier.contains(trimmed, ignoreCase = true)
                }
                .sortedByDescending { it.updatedAt }
                .take(MAX_RESULTS)
                .toList()

            // Merge: local matches first, then server-found issues the local
            // substring filter missed (description/comment hits), deduped by
            // id in server relevance order. A hit that exists in local Room
            // renders as its live local row; an unsynced hit renders from the
            // returned fields (a placeholder entity — the row only shows
            // identifier/title/status/priority).
            val seen = localMatches.mapTo(HashSet()) { it.id }
            val matches = if (server.query == trimmed && server.hits.isNotEmpty()) {
                val issuesById = issues.associateBy { it.id }
                val extras = server.hits.asSequence()
                    .filter { seen.add(it.id) }
                    .mapNotNull { hit ->
                        val local = issuesById[hit.id]
                        when {
                            local != null -> local.takeIf { it.archivedAt == null && it.boardId in boardsById }
                            hit.boardId in boardsById -> placeholderIssue(hit)
                            // No local board to group the row under (sync
                            // lag / archived board) — drop it.
                            else -> null
                        }
                    }
                    .toList()
                localMatches + extras
            } else {
                localMatches
            }

            // Group by board, most recently updated match first.
            val groups = LinkedHashMap<String, MutableList<IssueEntity>>()
            for (issue in matches) {
                groups.getOrPut(issue.boardId) { mutableListOf() }.add(issue)
            }
            SearchState(
                query = trimmed,
                groups = groups.map { (boardId, list) ->
                    SearchResultGroup(boardsById.getValue(boardId), list)
                },
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
    creatorId = "",
    sortOrder = 0.0,
    createdAt = "",
    updatedAt = "",
)
