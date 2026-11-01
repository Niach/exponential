package com.exponential.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.TeamSelection
import com.exponential.app.data.api.AppAccessList
import com.exponential.app.data.api.McpActivityList
import com.exponential.app.data.api.McpClientRow
import com.exponential.app.data.api.McpClientsList
import com.exponential.app.data.api.McpServerListRow
import com.exponential.app.data.api.McpServersApi
import com.exponential.app.data.api.McpSettingsApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.McpSettings
import com.exponential.app.ui.components.FailedAction
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

// #VAPP-185 (fixtures/settings-mcp.json): Settings › MCP on a phone. Every
// section is a server-only read (web `mcp-settings-page.tsx` fetches the
// same tRPC routes); null = still loading (skeleton rows). Phones have no
// server add/edit: the rows + the member's own Connect (the web page's
// `?connect=` deep link, in the browser) / Disconnect.

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class McpSettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    holder: DatabaseHolder,
    private val selection: TeamSelection,
    private val servers: McpServersApi,
    private val api: McpSettingsApi,
) : ViewModel() {

    private val dbFlow = accountDatabaseFlow(auth, holder)

    val isOwner: StateFlow<Boolean> = combine(dbFlow, selection.selectedId) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(emptyList()) else db.teamMemberDao().observeByTeam(team)
        }
        .combine(auth.userId) { members, userId ->
            userId != null && members.firstOrNull { it.userId == userId }?.role == DomainContract.teamRoleOwner
        }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** The selected team's slug (the web connect URL's path). */
    private val teamSlug: StateFlow<String?> = combine(dbFlow, selection.selectedId) { db, team -> db to team }
        .flatMapLatest { (db, team) ->
            if (db == null || team == null) flowOf(null) else db.teamDao().observeById(team).map { it?.slug }
        }
        .stateIn(viewModelScope, SharingStarted.Eagerly, null)

    val endpointUrl: StateFlow<String?> = auth.instanceUrl
        .map { it?.let(McpSettings::endpointUrl) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), auth.instanceUrl.value?.let(McpSettings::endpointUrl))

    private val _servers = MutableStateFlow<List<McpServerListRow>?>(null)
    val serverRows: StateFlow<List<McpServerListRow>?> = _servers

    private val _scope = MutableStateFlow("mine")
    val scope: StateFlow<String> = _scope

    private val _clients = MutableStateFlow<McpClientsList?>(null)
    val clients: StateFlow<McpClientsList?> = _clients

    private val _access = MutableStateFlow<AppAccessList?>(null)
    val access: StateFlow<AppAccessList?> = _access

    private val _activity = MutableStateFlow<McpActivityList?>(null)
    val activity: StateFlow<McpActivityList?> = _activity

    private val _pending = MutableStateFlow<String?>(null)
    /** The server or client id a write is in flight for. */
    val pending: StateFlow<String?> = _pending

    private val _failure = MutableStateFlow<FailedAction?>(null)
    val failure: StateFlow<FailedAction?> = _failure

    fun clearFailure() {
        _failure.value = null
    }

    private fun ids(): Pair<String, String>? {
        val account = auth.activeAccountId.value ?: return null
        val team = selection.selectedId.value ?: return null
        return account to team
    }

    /** The MCP page's reads (Servers, Clients, Access). */
    fun load() {
        val (account, team) = ids() ?: return
        viewModelScope.launch { _servers.value = quiet { servers.rows(account, team) } ?: emptyList() }
        loadClients()
        viewModelScope.launch { _access.value = quiet { api.appAccess(account, team) } }
    }

    fun loadActivity() {
        val (account, team) = ids() ?: return
        viewModelScope.launch { _activity.value = quiet { api.activity(account, team) } ?: McpActivityList() }
    }

    fun setScope(next: String) {
        if (next == _scope.value) return
        _scope.value = next
        _clients.value = null
        loadClients()
    }

    private fun loadClients() {
        val (account, team) = ids() ?: return
        val scope = _scope.value
        viewModelScope.launch { _clients.value = quiet { api.clients(account, team, scope) } ?: McpClientsList() }
    }

    fun revoke(client: McpClientRow) {
        val (account, team) = ids() ?: return
        val everyone = _scope.value == "everyone"
        _pending.value = client.clientId
        viewModelScope.launch {
            try {
                api.revokeClient(account, team, client.clientId, if (everyone) client.userId else null)
                loadClients()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("revoke", "the client", trpcErrorMessage(t, "")) { revoke(client) }
            } finally {
                _pending.value = null
            }
        }
    }

    /** Owners pick a member's level; optimistic, rolled back on a refusal. */
    fun setLevel(appId: String, userId: String, level: String) {
        val account = auth.activeAccountId.value ?: return
        val before = _access.value ?: return
        _access.value = before.copy(
            rows = before.rows.map { if (it.userId == userId) it.copy(levels = it.levels + (appId to level)) else it },
        )
        viewModelScope.launch {
            try {
                api.setAppAccess(account, appId, userId, level)
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _access.value = before
                _failure.value = FailedAction("save", "the access", trpcErrorMessage(t, "")) { setLevel(appId, userId, level) }
            }
        }
    }

    fun disconnect(server: McpServerListRow) {
        val (account, team) = ids() ?: return
        _pending.value = server.id
        viewModelScope.launch {
            try {
                servers.disconnect(account, server.id)
                _servers.value = quiet { servers.rows(account, team) } ?: _servers.value
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                _failure.value = FailedAction("disconnect from", server.name, trpcErrorMessage(t, "")) { disconnect(server) }
            } finally {
                _pending.value = null
            }
        }
    }

    /** The web page's connect deep link: `?connect=<serverId>` runs the
     *  OAuth (or Set key) flow there, where the provider's consent lives. */
    fun connectUrl(server: McpServerListRow): String? {
        val origin = auth.instanceUrl.value?.trimEnd('/') ?: return null
        val slug = teamSlug.value ?: return null
        return "$origin/t/$slug/settings/mcp?connect=${server.id}"
    }

    private suspend fun <T> quiet(block: suspend () -> T): T? = try {
        block()
    } catch (t: Throwable) {
        if (t is CancellationException) throw t
        null
    }
}
