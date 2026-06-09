package com.exponential.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.WorkspaceSelection
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class AppState(
    val instanceUrl: String? = null,
    val token: String? = null,
    val activeAccountId: String? = null,
    val accounts: List<ServerAccount> = emptyList(),
)

@HiltViewModel
class AppViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val syncManager: SyncManager,
    private val pushTokenManager: PushTokenManager,
    private val databaseHolder: DatabaseHolder,
    private val workspaceSelection: WorkspaceSelection,
) : ViewModel() {

    val state: StateFlow<AppState> = combine(
        auth.instanceUrl,
        auth.token,
        auth.activeAccountId,
        auth.accounts,
    ) { url, token, activeId, accounts ->
        AppState(
            instanceUrl = url,
            token = token,
            activeAccountId = activeId,
            accounts = accounts,
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

    fun setInstanceUrl(url: String) {
        viewModelScope.launch { auth.setInstanceUrl(url) }
    }

    // Resolves the project a fresh start should land in: the last project the
    // user opened on the active account, if it still exists locally and isn't
    // archived (deleted/archived projects fall back to home).
    suspend fun lastOpenedProjectId(): String? {
        val accountId = auth.activeAccountId.value ?: return null
        val projectId = workspaceSelection.lastProject(accountId) ?: return null
        return runCatching {
            databaseHolder.database(forAccountId = accountId).projectDao().getActiveById(projectId)?.id
        }.getOrNull()
    }

    fun clearInstance() {
        viewModelScope.launch {
            pushTokenManager.unregisterAndForget()
            syncManager.signOut()
            auth.clearInstanceUrl()
        }
    }

    fun signOut() {
        viewModelScope.launch {
            pushTokenManager.unregisterAndForget()
            syncManager.signOut()
            auth.clearToken()
        }
    }

    fun switchAccount(id: String) {
        viewModelScope.launch { auth.switchAccount(id) }
    }

    fun removeAccount(id: String) {
        viewModelScope.launch {
            auth.removeAccount(id)
            databaseHolder.deleteFiles(id)
        }
    }

}
