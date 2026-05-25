package com.exponential.app.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.MultiAccountWorkspaceRepository
import com.exponential.app.data.db.ProjectDao
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.ServerWorkspaceGroup
import com.exponential.app.data.db.WorkspaceDao
import com.exponential.app.data.db.WorkspaceEntity
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class HomeState(
    val email: String? = null,
    val workspaces: List<WorkspaceEntity> = emptyList(),
    val selectedWorkspace: WorkspaceEntity? = null,
    val projects: List<ProjectEntity> = emptyList(),
    val error: String? = null,
    // Unified cross-server picker source. `workspaces` above stays scoped to
    // the active server so existing in-screen displays (top-bar workspace
    // name, project list) keep working unchanged.
    val serverGroups: List<ServerWorkspaceGroup> = emptyList(),
    val activeAccountId: String? = null,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val authApi: AuthApi,
    private val workspacesApi: WorkspacesApi,
    private val workspaceDao: WorkspaceDao,
    private val projectDao: ProjectDao,
    private val selection: WorkspaceSelection,
    private val multiAccountWorkspaces: MultiAccountWorkspaceRepository,
) : ViewModel() {

    private val workspacesFlow = workspaceDao.observeAll()

    private val projectsFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else projectDao.observeByWorkspace(id)
    }

    // Pre-combine the new cross-server inputs into a single Flow so the outer
    // combine stays within kotlinx.coroutines' 5-arg typed overload.
    private val multiAccountFlow = combine(
        multiAccountWorkspaces.serverGroups,
        auth.activeAccountId,
    ) { groups, activeAccountId -> groups to activeAccountId }

    val state: StateFlow<HomeState> = combine(
        workspacesFlow,
        selection.selectedId,
        projectsFlow,
        auth.userEmail,
        multiAccountFlow,
    ) { workspaces, selectedId, projects, email, multi ->
        val (groups, activeAccountId) = multi
        val selected = workspaces.firstOrNull { it.id == selectedId }
            ?: workspaces.firstOrNull()
        HomeState(
            email = email,
            workspaces = workspaces,
            selectedWorkspace = selected,
            projects = projects,
            serverGroups = groups,
            activeAccountId = activeAccountId,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HomeState())

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error

    fun bootstrap() {
        viewModelScope.launch {
            try {
                val workspace = workspacesApi.ensureDefault()
                workspaceDao.upsert(workspace)
                if (selection.selectedId.value == null) selection.select(workspace.id)
                _error.value = null
            } catch (error: Throwable) {
                _error.value = error.message ?: "Failed to load workspace"
            }
        }
    }

    /// Cross-server-aware pick from the unified workspace picker. If the
    /// chosen workspace lives on the active server this is just a selection
    /// update; otherwise we pre-set the selection and switch accounts —
    /// SyncManager swaps the DB and MainActivity's `key(activeAccountId)`
    /// rebuilds the home UI; the new VM's combine honors the pre-set
    /// selectedId once the new account's workspaces sync in.
    fun selectWorkspace(accountId: String, workspaceId: String) {
        selection.select(workspaceId)
        if (accountId != auth.activeAccountId.value) {
            auth.switchAccount(accountId)
        }
    }
}
