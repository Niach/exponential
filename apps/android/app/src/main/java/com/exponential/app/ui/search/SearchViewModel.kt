package com.exponential.app.ui.search

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.IssuesApi
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.ProjectEntity
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

// Cross-project search (the Search tab), hybrid local + server:
//   - Fast path: a pure client-side substring match over identifier + title
//     across every project of the active account (local Room data, instant).
//   - Augmentation: the server-side full-text `issues.search` (title +
//     description + comment text) fires on the same debounced query, one call
//     per workspace of the account, and appends whatever the local filter
//     missed. Server errors degrade silently to local-only — typing is never
//     blocked on the network.
// The empty-query state embeds the "Assigned to you" list instead (the old
// My Issues tab's content).

/** Results under one project header, most recently updated project first. */
data class SearchResultGroup(val project: ProjectEntity, val issues: List<IssueEntity>)

data class SearchState(
    // The debounced query the current groups were computed for; blank means
    // "show the assigned-to-you state".
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
    private val projectsFlow = dbFlow.scopedQuery(emptyList<ProjectEntity>()) { it.projectDao().observeAll() }

    /**
     * A server response pinned to the query it answered, so a slow response
     * can never be merged under a fresher query's results.
     */
    private data class ServerSearch(val query: String = "", val hits: List<SearchIssueHit> = emptyList())

    // Server-backed "search everything". There is no single active workspace —
    // this tab spans the whole account — so fan out one `issues.search` per
    // distinct workspace id of the synced projects (typically one or two) and
    // flatten. `transformLatest` cancels the in-flight round trip whenever the
    // debounced query (or account/workspace set) changes; per-call failures
    // collapse to "no extra hits".
    private val serverSearch: Flow<ServerSearch> = combine(
        auth.activeAccountId,
        projectsFlow.map { projects -> projects.map { it.workspaceId }.distinct().sorted() }.distinctUntilChanged(),
        debouncedQuery,
    ) { accountId, workspaceIds, query -> Triple(accountId, workspaceIds, query.trim()) }
        .distinctUntilChanged()
        .transformLatest { (accountId, workspaceIds, query) ->
            // Clear stale hits for the new query immediately (local-only view
            // renders while the round trip runs).
            emit(ServerSearch(query))
            if (accountId == null || query.isEmpty() || workspaceIds.isEmpty()) return@transformLatest
            val hits = coroutineScope {
                workspaceIds.map { workspaceId ->
                    async {
                        try {
                            issuesApi.search(accountId, workspaceId, query)
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
        projectsFlow,
        debouncedQuery,
        serverSearch,
    ) { issues, projects, query, server ->
        val trimmed = query.trim()
        if (trimmed.isEmpty()) {
            SearchState(query = "")
        } else {
            val projectsById = projects.associateBy { it.id }
            // Live projects only (the DAO already filters archived projects);
            // archived issues are excluded here — observeAll includes them.
            val localMatches = issues.asSequence()
                .filter { it.archivedAt == null && it.projectId in projectsById }
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
                            local != null -> local.takeIf { it.archivedAt == null && it.projectId in projectsById }
                            hit.projectId in projectsById -> placeholderIssue(hit)
                            // No local project to group the row under (sync
                            // lag / archived project) — drop it.
                            else -> null
                        }
                    }
                    .toList()
                localMatches + extras
            } else {
                localMatches
            }

            // Group by project, most recently updated match first.
            val groups = LinkedHashMap<String, MutableList<IssueEntity>>()
            for (issue in matches) {
                groups.getOrPut(issue.projectId) { mutableListOf() }.add(issue)
            }
            SearchState(
                query = trimmed,
                groups = groups.map { (projectId, list) ->
                    SearchResultGroup(projectsById.getValue(projectId), list)
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
    projectId = hit.projectId,
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
