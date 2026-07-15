package com.exponential.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.api.UpdateGate
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.domain.DomainContract
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class AppState(
    val instanceUrl: String? = null,
    val token: String? = null,
    val activeAccountId: String? = null,
    val accounts: List<ServerAccount> = emptyList(),
    // Non-null once the server has answered HTTP 426 (this build is below the
    // configured minimum, EXP-104) — drives the blocking "Update required" gate.
    val updateRequired: UpdateGate.UpgradeInfo? = null,
)

@HiltViewModel
class AppViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val syncManager: SyncManager,
    private val pushTokenManager: PushTokenManager,
    private val databaseHolder: DatabaseHolder,
    private val workspaceSelection: WorkspaceSelection,
    private val updateGate: UpdateGate,
) : ViewModel() {

    init {
        // Workspace selection is a global StateFlow; when the active account
        // changes (switch, or login re-keying a pending id to a per-user id) the
        // old selected workspace id belongs to a different DB. Clear it so the
        // new account resolves its own default (drop(1) skips the initial value).
        viewModelScope.launch {
            // activeAccountId is a StateFlow (already conflated/distinct); drop(1)
            // skips the initial value so we only clear on an actual switch.
            auth.activeAccountId
                .drop(1)
                .collect { workspaceSelection.clearSelection() }
        }
        // Stale-selection guard (EXP-43 hardening): a deleted workspace leaves
        // the global selection pointing at a row that no longer exists in Room
        // (Electric removes it), which future consumers of selectedId would
        // trip over. Clear it once the id is confirmed gone. The delay absorbs
        // legitimate transients — e.g. the cross-server Settings tap selects
        // the target workspace BEFORE its account switch lands, so the id is
        // briefly absent from the still-active DB; collectLatest cancels the
        // pending clear as soon as the id resolves (or db/selection change).
        @OptIn(ExperimentalCoroutinesApi::class)
        viewModelScope.launch {
            combine(
                accountDatabaseFlow(auth, databaseHolder),
                workspaceSelection.selectedId,
            ) { db, id -> db to id }
                .flatMapLatest { (db, id) ->
                    if (db == null || id == null) flowOf(false)
                    else db.workspaceDao().observeById(id).map { it == null }
                }
                .collectLatest { stale ->
                    if (!stale) return@collectLatest
                    delay(2_000)
                    workspaceSelection.clearSelection()
                }
        }
    }

    val state: StateFlow<AppState> = combine(
        auth.instanceUrl,
        auth.token,
        auth.activeAccountId,
        auth.accounts,
        updateGate.state,
    ) { url, token, activeId, accounts, updateRequired ->
        AppState(
            instanceUrl = url,
            token = token,
            activeAccountId = activeId,
            accounts = accounts,
            updateRequired = updateRequired,
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, AppState())

    // Unread notifications for the active account — drives the bottom bar's
    // inbox dot. Re-scopes reactively on account switch like the feature VMs.
    @OptIn(ExperimentalCoroutinesApi::class)
    val unreadCount: StateFlow<Int> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        auth.accounts,
    ) { db, activeId, accounts ->
        db to accounts.firstOrNull { it.id == activeId }?.userId
    }.flatMapLatest { (db, userId) ->
        if (db == null || userId == null) flowOf(0)
        else db.notificationDao().observeUnreadCount(userId)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, 0)

    // True while at least one coding session is running on the active account —
    // drives the bottom bar's green Agents dot.
    @OptIn(ExperimentalCoroutinesApi::class)
    val agentsRunning: StateFlow<Boolean> = accountDatabaseFlow(auth, databaseHolder)
        .flatMapLatest { db ->
            if (db == null) flowOf(false)
            else db.codingSessionDao()
                .observeByStatus(DomainContract.codingSessionStatusRunning)
                .map { it.isNotEmpty() }
        }
        .stateIn(viewModelScope, SharingStarted.Eagerly, false)

    // The Issues tab root's current project: last-used on the active account
    // (validated against the live Room table, so deleted/archived projects fall
    // through), else the first project of the first workspace, else none. The
    // lastProjectVersion counter re-runs the resolve after every last-used
    // write — that's what swaps the root list in place after a switcher pick.
    @OptIn(ExperimentalCoroutinesApi::class)
    private val currentProject: StateFlow<ProjectEntity?> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        workspaceSelection.lastProjectVersion,
    ) { db, accountId, _ -> db to accountId }
        .flatMapLatest { (db, accountId) ->
            if (db == null || accountId == null) flowOf(null)
            else combine(
                db.projectDao().observeAll(),
                db.workspaceDao().observeAll(),
            ) { projects, workspaces ->
                val lastUsed = workspaceSelection.lastProject(accountId)
                projects.firstOrNull { it.id == lastUsed }
                    ?: workspaces.firstNotNullOfOrNull { ws ->
                        projects.firstOrNull { it.workspaceId == ws.id }
                    }
                    ?: projects.firstOrNull()
            }
        }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    val currentProjectId: StateFlow<String?> = currentProject
        .map { it?.id }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    fun setInstanceUrl(url: String) {
        viewModelScope.launch { auth.setInstanceUrl(url) }
    }

    fun clearInstance() {
        viewModelScope.launch {
            // Awaited before the credentials drop — the unregister request
            // needs the bearer token that clearInstanceUrl removes.
            auth.activeAccountId.value?.let { pushTokenManager.unregisterToken(it) }
            syncManager.signOut()
            auth.clearInstanceUrl()
        }
    }

    fun signOut() {
        viewModelScope.launch {
            auth.activeAccountId.value?.let { pushTokenManager.unregisterToken(it) }
            syncManager.signOut()
            auth.clearToken()
        }
    }

    fun switchAccount(id: String) {
        viewModelScope.launch { auth.switchAccount(id) }
    }

    fun removeAccount(id: String) {
        viewModelScope.launch {
            pushTokenManager.unregisterToken(id)
            auth.removeAccount(id)
            databaseHolder.deleteFiles(id)
        }
    }

}
