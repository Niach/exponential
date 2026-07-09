package com.exponential.app.ui.share

import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.WorkspaceEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn

data class WorkspaceProjects(val workspace: WorkspaceEntity, val projects: List<ProjectEntity>)

data class ShareTargetState(
    val groups: List<WorkspaceProjects> = emptyList(),
    val recentProjectId: String? = null,
    val isLoading: Boolean = true,
)

/**
 * Data source for the single-screen share composer (`share-compose`): the
 * active account's workspaces → projects, with the most recently opened project
 * surfaced as the default. Consumed by [com.exponential.app.ui.issue.CreateIssueScreen]
 * in share mode, which renders the project selector inline at the bottom.
 */
@HiltViewModel
class ShareTargetPickerViewModel @Inject constructor(
    auth: AuthRepository,
    holder: DatabaseHolder,
    selection: WorkspaceSelection,
) : ViewModel() {

    // Reactive account scoping (no constructor-time DB snapshot).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    val state: StateFlow<ShareTargetState> = combine(
        dbFlow.scopedQuery(emptyList()) { it.workspaceDao().observeAll() },
        dbFlow.scopedQuery(emptyList()) { it.projectDao().observeAll() },
        auth.activeAccountId,
    ) { workspaces, projects, accountId ->
        val byWorkspace = projects.groupBy { it.workspaceId }
        val groups = workspaces.mapNotNull { ws ->
            val ps = byWorkspace[ws.id].orEmpty()
            if (ps.isEmpty()) null else WorkspaceProjects(ws, ps)
        }
        ShareTargetState(
            groups = groups,
            recentProjectId = accountId?.let { selection.lastProject(it) },
            isLoading = false,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ShareTargetState())
}
