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
import com.exponential.app.data.db.ServerProjectGroup
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// Powers the Issues tab root's project-switcher sheet (the old Projects home
// collapsed into a bottom sheet): the cross-server workspace/project tree,
// first-run workspace bootstrap, and the pick action that swaps the root
// list's project in place.

data class HomeState(
    // Server > Workspace > Project tree shown in the switcher sheet. Every
    // signed-in account contributes one block.
    val projectTree: List<ServerProjectGroup> = emptyList(),
    // True while the first workspace bootstrap is in flight and nothing has
    // synced yet — drives the root "Syncing…" state (parity with iOS).
    val isSyncing: Boolean = false,
    // True when the ACTIVE account has at least one project. Distinct from
    // "the tree is non-empty": the tree includes projectless workspaces (a
    // fresh account has exactly one), and a sibling account may hold projects
    // the active one doesn't — so the root screen must gate its spinner on
    // THIS, not on tree-non-emptiness, or a projectless active account spins
    // forever (its current project can never resolve).
    val activeAccountHasProject: Boolean = false,
    // True once the ACTIVE account's projects shape has reached up-to-date at
    // least once (initial snapshot done, even at zero rows). Until then a
    // projectless-looking account is still syncing — the root shows "Syncing…"
    // rather than prematurely flashing "Create your first project".
    val activeAccountProjectsSynced: Boolean = false,
)

@HiltViewModel
class HomeViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val workspacesApi: WorkspacesApi,
    private val holder: DatabaseHolder,
    private val selection: WorkspaceSelection,
    multiAccountProjects: MultiAccountProjectRepository,
) : ViewModel() {

    private val _syncing = MutableStateFlow(false)

    // The active account's projects-shape "up-to-date seen" flag, re-scoped on
    // account switch. Null (no offset row yet) reads as not-yet-synced.
    private val activeProjectsSynced =
        accountDatabaseFlow(auth, holder)
            .scopedQuery<Boolean?>(null) { db -> db.electricOffsetDao().observeIsLive("projects") }
            .map { it == true }

    val state: StateFlow<HomeState> = combine(
        multiAccountProjects.serverGroups,
        _syncing,
        auth.activeAccountId,
        activeProjectsSynced,
    ) { projectTree, syncing, activeId, projectsSynced ->
        val activeHasProject = projectTree
            .firstOrNull { it.accountId == activeId }
            ?.workspaceBlocks
            ?.any { it.projects.isNotEmpty() } == true
        HomeState(
            projectTree = projectTree,
            isSyncing = syncing,
            activeAccountHasProject = activeHasProject,
            activeAccountProjectsSynced = projectsSynced,
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

    /// Project pick from the switcher sheet. Makes the picked project's account
    /// active (a no-op for same-server picks) and records it as last-used —
    /// the Issues root's current-project resolution reacts to both, so the
    /// list swaps in place with no navigation.
    fun selectProject(accountId: String, projectId: String) {
        if (accountId != auth.activeAccountId.value) {
            auth.switchAccount(accountId)
        }
        selection.rememberLastProject(accountId, projectId)
    }
}
