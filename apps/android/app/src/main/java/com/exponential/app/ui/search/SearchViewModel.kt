package com.exponential.app.ui.search

import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.IssueSearch
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import com.exponential.app.data.TeamSelection
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transformLatest

// The global issue search (#EXP-1382, search-list.json TEAM SCOPE + TYPING),
// hybrid local + server, scoped to the ACTIVE team (`TeamSelection`, the
// shell's one active team):
//   - Fast path: the shared `IssueSearch` engine (EXP-892 — identifier before
//     title before description, one ranking ×4) over that team's issues
//     (local Room data). The rows are PREPARED once per issues emission
//     (status resolved, timestamps parsed) and every keystroke ranks them on
//     Dispatchers.Default, so the field paints every keystroke at once.
//   - Augmentation: ONE server-side full-text `issues.search` (title +
//     description + comment text) scoped to that team, on the debounced
//     query; it appends whatever the local rank missed. Server errors degrade
//     silently to local-only — typing is never blocked on the network.
// Switching the team re-runs the query. EXP-922: one flat relevance-ordered
// list (undone issues first), the same limit and copy as the other clients.

/**
 * EXP-922: ONE flat ranked row — the web sheet's and the desktop palette's
 * shape. The board rides ALONG the issue (it draws the row's sub-line) instead
 * of banding the list into per-board sections, so the four surfaces list the
 * same rows in the same single relevance order. [board] is null only while a
 * server hit's board has not synced.
 */
data class SearchResult(
    val issue: IssueEntity,
    val board: BoardEntity?,
    /** P53: the issue's RESOLVED team status row (statusId, then the anchor). */
    val status: ResolvedIssueStatus = IssueStatusResolver.backlogDefault,
)

data class SearchState(
    // The query the current results were computed for; blank means "show the
    // idle search hint".
    val query: String = "",
    val results: List<SearchResult> = emptyList(),
)

/** EXP-922: the ONE limit every search surface on every client uses. */
private const val MAX_RESULTS = IssueSearch.DEFAULT_LIMIT

/**
 * #EXP-1382: one issue prepared ONCE per issues emission — its result (board +
 * resolved status) and its timestamps parsed — so a keystroke only ranks.
 */
private class PreparedRow(val result: SearchResult) : IssueSearch.Row {
    private val issue get() = result.issue
    override val id: String get() = issue.id
    override val identifier: String get() = issue.identifier
    override val title: String get() = issue.title
    override val description: String? get() = issue.description
    override val createdAt: String? get() = issue.createdAt
    override val updatedAt: String? get() = issue.updatedAt
    override val status: String? get() = issue.status
    override val createdAtMs: Long = IssueSearch.epochMs(issue.createdAt)
    override val updatedAtMs: Long = IssueSearch.epochMs(issue.updatedAt)
}

/** The active team's prepared pool. */
private class SearchPool(
    val rows: List<PreparedRow> = emptyList(),
    val boardsById: Map<String, BoardEntity> = emptyMap(),
    val statuses: List<ResolvedIssueStatus> = emptyList(),
) {
    val byId: Map<String, PreparedRow> = rows.associateBy { it.id }
}

@OptIn(FlowPreview::class, ExperimentalCoroutinesApi::class)
@HiltViewModel
class SearchViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: TeamSelection,
    private val issuesApi: IssuesApi,
) : ViewModel() {

    // Reactive account + team scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)
    private val dbAndTeam = combine(dbFlow, selection.selectedId) { db, teamId -> db to teamId }

    // Raw input updates on every keystroke (the field itself stays responsive
    // via local Compose state); the local rank follows a short debounce, the
    // server round trip a longer one.
    private val _query = MutableStateFlow("")
    fun setQuery(query: String) {
        _query.value = query
    }

    private val rankQuery = _query.map { it.trim() }.debounce(80).distinctUntilChanged()
    private val serverQuery = _query.map { it.trim() }.debounce(250).distinctUntilChanged()

    /** The active team's issues, boards and statuses, prepared once per emission. */
    private val pool: Flow<SearchPool> = dbAndTeam.flatMapLatest { (db, teamId) ->
        if (db == null || teamId == null) {
            flowOf(SearchPool())
        } else {
            combine(
                db.issueDao().observeByTeam(teamId),
                db.boardDao().observeByTeam(teamId),
                db.issueStatusDao().observeByTeam(teamId),
            ) { issues, boards, statusRows ->
                val boardsById = boards.associateBy { it.id }
                val statuses = IssueStatusResolver.teamStatuses(statusRows)
                SearchPool(
                    rows = issues.mapNotNull { issue ->
                        val board = boardsById[issue.boardId] ?: return@mapNotNull null
                        PreparedRow(SearchResult(issue, board, IssueStatusResolver.resolve(issue, statuses)))
                    },
                    boardsById = boardsById,
                    statuses = statuses,
                )
            }
        }
    }.flowOn(Dispatchers.Default)

    /**
     * A server response pinned to the query it answered, so a slow response
     * can never be merged under a fresher query's results.
     */
    private data class ServerSearch(val query: String = "", val hits: List<SearchIssueHit> = emptyList())

    // ONE `issues.search` scoped to the active team. `transformLatest` cancels
    // the in-flight round trip whenever the query, account or team changes;
    // a failure collapses to "no extra hits".
    private val serverSearch: Flow<ServerSearch> = combine(
        auth.activeAccountId,
        selection.selectedId,
        serverQuery,
    ) { accountId, teamId, query -> Triple(accountId, teamId, query) }
        .distinctUntilChanged()
        .transformLatest { (accountId, teamId, query) ->
            // Clear stale hits for the new query immediately (local-only view
            // renders while the round trip runs).
            emit(ServerSearch(query))
            if (accountId == null || teamId == null || query.isEmpty()) return@transformLatest
            val hits = try {
                issuesApi.search(accountId, teamId, query, limit = MAX_RESULTS)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // Offline / server error / stale membership: the local fast
                // path already rendered — never surface.
                emptyList()
            }
            emit(ServerSearch(query, hits))
        }

    // The rank: a flow on Dispatchers.Default over the prepared pool.
    val state: StateFlow<SearchState> = combine(pool, rankQuery, serverSearch) { pool, query, server ->
        Triple(pool, query, server)
    }
        .mapLatest { (pool, query, server) -> rank(pool, query, server) }
        .flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SearchState())

    private fun rank(pool: SearchPool, query: String, server: ServerSearch): SearchState {
        if (query.isEmpty()) return SearchState(query = "")
        val localMatches = IssueSearch.rank(pool.rows, query, limit = MAX_RESULTS)

        // Merge (the shared splice): local matches first, then the
        // server-found issues the local ranking missed (description / comment
        // hits), deduped by id in server relevance order. A hit that exists in
        // local Room renders as its live local row; an unsynced hit renders
        // from the returned fields (a placeholder entity — the row only shows
        // identifier/title/status/priority).
        val matches: List<SearchResult> = if (server.query == query && server.hits.isNotEmpty()) {
            IssueSearch.mergeServerHits(localMatches, server.hits, limit = MAX_RESULTS) { hit ->
                pool.byId[hit.id] ?: pool.boardsById[hit.boardId]?.let { board ->
                    val issue = placeholderIssue(hit)
                    PreparedRow(SearchResult(issue, board, IssueStatusResolver.resolve(issue, pool.statuses)))
                }
                // No local board to show the row under (sync lag / trashed
                // board) — dropped.
            }.map { it.result }
        } else {
            localMatches.map { it.result }
        }
        // EXP-922: ONE flat list in the ranked order — no board bands.
        return SearchState(query = query, results = matches)
    }
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
