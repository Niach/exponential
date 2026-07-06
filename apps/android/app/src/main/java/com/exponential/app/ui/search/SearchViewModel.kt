package com.exponential.app.ui.search

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.stateIn

// Cross-project search (the Search tab): a pure client-side substring match
// over identifier + title across every project of the active account — local
// Room data only, no server round trip. The empty-query state embeds the
// "Assigned to you" list instead (the old My Issues tab's content).

/** Results under one project header, most recently updated project first. */
data class SearchResultGroup(val project: ProjectEntity, val issues: List<IssueEntity>)

data class SearchState(
    // The debounced query the current groups were computed for; blank means
    // "show the assigned-to-you state".
    val query: String = "",
    val groups: List<SearchResultGroup> = emptyList(),
)

private const val MAX_RESULTS = 50

@OptIn(FlowPreview::class)
@HiltViewModel
class SearchViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
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

    val state: StateFlow<SearchState> = combine(
        dbFlow.scopedQuery(emptyList()) { it.issueDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.projectDao().observeAll() },
        debouncedQuery,
    ) { issues, projects, query ->
        val trimmed = query.trim()
        if (trimmed.isEmpty()) {
            SearchState(query = "")
        } else {
            val projectsById = projects.associateBy { it.id }
            // Live projects only (the DAO already filters archived projects);
            // archived issues are excluded here — observeAll includes them.
            val matches = issues.asSequence()
                .filter { it.archivedAt == null && it.projectId in projectsById }
                .filter {
                    it.title.contains(trimmed, ignoreCase = true) ||
                        it.identifier.contains(trimmed, ignoreCase = true)
                }
                .sortedByDescending { it.updatedAt }
                .take(MAX_RESULTS)
                .toList()
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
