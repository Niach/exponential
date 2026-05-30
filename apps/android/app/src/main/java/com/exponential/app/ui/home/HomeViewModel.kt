package com.exponential.app.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.WorkspacesApi
import com.exponential.app.data.auth.AuthRepository
import io.ktor.http.HttpStatusCode
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.MultiAccountProjectRepository
import com.exponential.app.data.db.MultiAccountWorkspaceRepository
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.ServerProjectGroup
import com.exponential.app.data.db.ServerWorkspaceGroup
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
    // name, drawer dropdown) keep working unchanged.
    val serverGroups: List<ServerWorkspaceGroup> = emptyList(),
    val activeAccountId: String? = null,
    // Server > Workspace > Project tree shown on Home. Every signed-in
    // account contributes one block.
    val projectTree: List<ServerProjectGroup> = emptyList(),
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val workspacesApi: WorkspacesApi,
    private val holder: DatabaseHolder,
    private val selection: WorkspaceSelection,
    private val multiAccountWorkspaces: MultiAccountWorkspaceRepository,
    private val multiAccountProjects: MultiAccountProjectRepository,
) : ViewModel() {

    private val accountId = auth.activeAccountId.value ?: ""
    private val db = holder.database(forAccountId = accountId)

    private val workspacesFlow = db.workspaceDao().observeAll()

    private val projectsFlow = selection.selectedId.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else db.projectDao().observeByWorkspace(id)
    }

    // Pre-combine the new cross-server inputs into a single Flow so the outer
    // combine stays within kotlinx.coroutines' 5-arg typed overload.
    private val multiAccountFlow = combine(
        multiAccountWorkspaces.serverGroups,
        auth.activeAccountId,
        multiAccountProjects.serverGroups,
    ) { groups, activeAccountId, projectTree -> Triple(groups, activeAccountId, projectTree) }

    val state: StateFlow<HomeState> = combine(
        workspacesFlow,
        selection.selectedId,
        projectsFlow,
        auth.userEmail,
        multiAccountFlow,
    ) { workspaces, selectedId, projects, email, multi ->
        val (groups, activeAccountId, projectTree) = multi
        val selected = workspaces.firstOrNull { it.id == selectedId }
            ?: workspaces.firstOrNull()
        HomeState(
            email = email,
            workspaces = workspaces,
            selectedWorkspace = selected,
            projects = projects,
            serverGroups = groups,
            activeAccountId = activeAccountId,
            projectTree = projectTree,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HomeState())

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error

    fun bootstrap() {
        viewModelScope.launch {
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                val workspace = workspacesApi.ensureDefault(accountId)
                db.workspaceDao().upsert(workspace)
                if (selection.selectedId.value == null) selection.select(workspace.id)
                _error.value = null
            } catch (error: Throwable) {
                // A rejected session (expired/revoked token) can't be recovered by
                // retrying — clear it so the app routes the active account back to
                // login instead of looping on a 401'd home screen.
                if (error is TrpcException && error.status == HttpStatusCode.Unauthorized) {
                    auth.clearToken()
                }
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

    /// Project tap from the cross-server Home tree. Returns whether the
    /// caller can navigate immediately (same-server) or should wait for the
    /// activeAccountId-keyed rebuild + `pendingProjectId` consumption to do
    /// it for them (cross-server).
    fun onProjectTap(accountId: String, projectId: String): Boolean {
        return if (accountId == auth.activeAccountId.value) {
            true
        } else {
            selection.setPendingProject(projectId)
            auth.switchAccount(accountId)
            false
        }
    }
}
