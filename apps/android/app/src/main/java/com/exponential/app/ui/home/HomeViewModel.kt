package com.exponential.app.ui.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.CreateProjectInput
import com.exponential.app.data.api.CreateWorkspaceInput
import com.exponential.app.data.api.ProjectsApi
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
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
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
    // True while the first workspace bootstrap is in flight and nothing has
    // synced yet — drives the Home "Syncing…" state (parity with iOS).
    val isSyncing: Boolean = false,
)

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val workspacesApi: WorkspacesApi,
    private val projectsApi: ProjectsApi,
    private val holder: DatabaseHolder,
    private val selection: WorkspaceSelection,
    private val multiAccountWorkspaces: MultiAccountWorkspaceRepository,
    private val multiAccountProjects: MultiAccountProjectRepository,
) : ViewModel() {

    // Reactive account scoping: all queries re-scope on account switch (no
    // constructor-time DB snapshot, no key(activeAccountId) rebuild needed).
    private val dbFlow = accountDatabaseFlow(auth, holder)

    private val workspacesFlow = dbFlow.scopedQuery(emptyList()) { it.workspaceDao().observeAll() }

    private val projectsFlow = combine(dbFlow, selection.selectedId) { db, id -> db to id }
        .flatMapLatest { (db, id) ->
            if (db == null || id == null) flowOf(emptyList())
            else db.projectDao().observeByWorkspace(id)
        }

    private val _syncing = MutableStateFlow(false)

    // Pre-combine the new cross-server inputs into a single Flow so the outer
    // combine stays within kotlinx.coroutines' 5-arg typed overload.
    private data class MultiAccount(
        val groups: List<ServerWorkspaceGroup>,
        val activeAccountId: String?,
        val projectTree: List<ServerProjectGroup>,
        val syncing: Boolean,
    )

    private val multiAccountFlow = combine(
        multiAccountWorkspaces.serverGroups,
        auth.activeAccountId,
        multiAccountProjects.serverGroups,
        _syncing,
    ) { groups, activeAccountId, projectTree, syncing ->
        MultiAccount(groups, activeAccountId, projectTree, syncing)
    }

    val state: StateFlow<HomeState> = combine(
        workspacesFlow,
        selection.selectedId,
        projectsFlow,
        auth.userEmail,
        multiAccountFlow,
    ) { workspaces, selectedId, projects, email, multi ->
        val selected = workspaces.firstOrNull { it.id == selectedId }
            ?: workspaces.firstOrNull()
        HomeState(
            email = email,
            workspaces = workspaces,
            selectedWorkspace = selected,
            projects = projects,
            serverGroups = multi.groups,
            activeAccountId = multi.activeAccountId,
            projectTree = multi.projectTree,
            isSyncing = multi.syncing,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HomeState())

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error

    fun bootstrap() {
        viewModelScope.launch {
            _syncing.value = true
            try {
                val accountId = auth.activeAccountId.value ?: return@launch
                val workspace = workspacesApi.ensureDefault(accountId)
                holder.database(forAccountId = accountId).workspaceDao().upsert(workspace)
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
            } finally {
                // Always clear the spinner — the success path, the
                // null-accountId early `return@launch`, and the catch block
                // all funnel through here. Without this, an account that
                // bootstraps fine but has zero projects would be stuck on a
                // perpetual "Syncing…" state.
                _syncing.value = false
            }
        }
    }

    /// Project tap from the cross-server Home tree. Makes the tapped project's
    /// account active (a no-op for same-server taps); the caller navigates
    /// immediately afterwards — feature ViewModels scope to the active account
    /// reactively, so no rebuild/pending-handoff dance is needed.
    fun onProjectTap(accountId: String) {
        if (accountId != auth.activeAccountId.value) {
            auth.switchAccount(accountId)
        }
    }

    // Create a workspace on the given account's server. Returns an error message
    // on failure, or null on success — Electric sync then surfaces the new row in
    // the Home tree (no manual upsert, so this stays correct across accounts).
    suspend fun createWorkspace(accountId: String, name: String): String? =
        try {
            workspacesApi.create(accountId, CreateWorkspaceInput(name.trim()))
            null
        } catch (error: Throwable) {
            error.message ?: "Failed to create workspace"
        }

    // Create a project in the given workspace on the given account's server.
    suspend fun createProject(
        accountId: String,
        workspaceId: String,
        name: String,
        prefix: String,
        color: String,
    ): String? =
        try {
            projectsApi.create(
                accountId,
                CreateProjectInput(
                    workspaceId = workspaceId,
                    name = name.trim(),
                    prefix = prefix.trim().uppercase(),
                    color = color,
                ),
            )
            null
        } catch (error: Throwable) {
            error.message ?: "Failed to create project"
        }
}
