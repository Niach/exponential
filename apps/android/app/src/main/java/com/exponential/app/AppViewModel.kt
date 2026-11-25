package com.exponential.app

import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.ShareTargets
import com.exponential.app.ui.components.Blocking
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.ShellState
import com.exponential.app.data.crossTeamOrder
import com.exponential.app.navigation.AssistantRef
import com.exponential.app.ui.inbox.inboxUnread
import com.exponential.app.ui.settings.AssistantSettingsRouteViewModel
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.UpdateGate
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.electric.SyncHealth
import com.exponential.app.data.electric.SyncHealthTracker
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.data.steer.SteerConnectionStore
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOn

data class AppState(
    val instanceUrl: String? = null,
    val token: String? = null,
    val activeAccountId: String? = null,
    val accounts: List<ServerAccount> = emptyList(),
    // Non-null once the ACTIVE account's server has answered HTTP 426 (this
    // build is below that server's configured minimum, EXP-104) — drives the
    // blocking "Update required" gate. Keyed per instance (REV2-18): a
    // background account's 426 never blocks the app, it only stops that
    // account's sync and raises [gatedOtherServers].
    val updateRequired: UpdateGate.UpgradeInfo? = null,
)

@HiltViewModel
class AppViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val syncManager: SyncManager,
    private val pushTokenManager: PushTokenManager,
    private val enetNodeManager: EnetNodeManager,
    private val databaseHolder: DatabaseHolder,
    private val shell: ShellState,
    private val updateGate: UpdateGate,
    private val authApi: AuthApi,
    private val steerConnectionStore: SteerConnectionStore,
    private val syncHealthTracker: SyncHealthTracker,
    private val chatStores: com.exponential.app.data.chatstore.ChatStoreManager,
) : ViewModel() {

    init {
        viewModelScope.launch {
            // activeAccountId is a StateFlow (already conflated/distinct); drop(1)
            // skips the initial value so this runs on an actual switch only.
            auth.activeAccountId
                .drop(1)
                .collect {
                    // The app-held steer sockets (EXP-621) carry the OLD
                    // account's ticket and their feeds are its data — an
                    // account switch has to end them, not hand them over.
                    steerConnectionStore.closeAll()
                }
        }
        // REV2-18: a server that 426s rejects every request from this build, so
        // keep its 16 shape loops from polling forever — for the active account
        // (whose screen is the blocking gate) and, crucially, for background
        // accounts, which used to keep re-triggering the process-global latch.
        viewModelScope.launch {
            combine(updateGate.gated, auth.accounts) { gated, accounts ->
                accounts.filter { it.token != null && it.isGated(gated) }.map { it.id }
            }.collectLatest { ids ->
                if (ids.isEmpty()) return@collectLatest
                // Re-assert on a backing-off schedule rather than stopping once:
                // SyncManager's reconcile consumes the SAME auth.accounts
                // emission on its own dispatcher, so on an account-set change a
                // stop issued here can land BEFORE the relaunch it is meant to
                // undo — and nothing re-emits afterwards to fix it up (the latch
                // is first-wins, so later 426s yield an equal map the StateFlow
                // dedupes), leaving that account's 16 loops polling a server
                // that answers nothing but 426. Cancelling an already-cancelled
                // pipeline is a map lookup against an equal stats map, so the
                // steady-state cost is a no-op tick a minute, and collectLatest
                // drops the whole schedule the moment a newer gated/accounts
                // emission supersedes it.
                var backoffMs = GATED_STOP_REASSERT_MIN_MS
                while (true) {
                    ids.forEach { syncManager.signOut(it) }
                    delay(backoffMs)
                    backoffMs = (backoffMs * 2).coerceAtMost(GATED_STOP_REASSERT_MAX_MS)
                }
            }
        }
    }

    val state: StateFlow<AppState> = combine(
        auth.instanceUrl,
        auth.token,
        auth.activeAccountId,
        auth.accounts,
        updateGate.gated,
    ) { url, token, activeId, accounts, gated ->
        AppState(
            instanceUrl = url,
            token = token,
            activeAccountId = activeId,
            accounts = accounts,
            updateRequired = url?.let { UpdateGate.originKey(it) }?.let { gated[it] },
        )
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.Eagerly, AppState())

    // Signed-in servers OTHER than the active one that answered 426 (REV2-18).
    // Their pipelines are stopped below; this is the banner that says so, since
    // silently frozen background sync reads as a broken app.
    val gatedOtherServers: StateFlow<List<String>> = combine(
        updateGate.gated,
        auth.accounts,
        auth.activeAccountId,
    ) { gated, accounts, activeId ->
        accounts
            .filter { it.id != activeId && it.token != null && it.isGated(gated) }
            .map { it.displayName }
            .distinct()
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    // EXP-533: is the ACTIVE account's sync reaching its server? Drives the
    // floating offline banner. WhileSubscribed, because the model re-evaluates
    // on a 2s timer while a failure streak is open and nothing off-screen
    // needs that.
    val syncHealth: StateFlow<SyncHealth> =
        syncHealthTracker.activeHealth(auth.activeAccountId)
            .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SyncHealth.Ok)

    // Unread notifications of every team on the active account — the inbox
    // circle's count, exactly the inbox page's rows. Re-scopes reactively on
    // an account switch.
    @OptIn(ExperimentalCoroutinesApi::class)
    val unreadCount: StateFlow<Int> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        auth.accounts,
    ) { db, activeId, accounts ->
        db to accounts.firstOrNull { it.id == activeId }?.userId
    }.flatMapLatest { (db, userId) ->
        if (db == null || userId == null) flowOf(0)
        else combine(
            db.notificationDao().observeByUser(userId),
            db.issueDao().observeAll(),
        ) { notifications, issues -> inboxUnread(notifications, issues) }
    }.distinctUntilChanged()
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.Eagerly, 0)

    // #EXP-2109 (`shell.newMenu`): the board New issue files onto off a board:
    // the last-used one, else the first board of the first team.
    @OptIn(ExperimentalCoroutinesApi::class)
    val newIssueBoardId: StateFlow<String?> = combine(
        accountDatabaseFlow(auth, databaseHolder),
        auth.activeAccountId,
        shell.lastBoardVersion,
    ) { db, accountId, _ -> db to accountId }
        .flatMapLatest { (db, accountId) ->
            if (db == null || accountId == null) flowOf(null)
            else combine(db.boardDao().observeAll(), db.teamDao().observeAll()) { boards, teams ->
                val last = shell.lastBoard(accountId)
                boards.firstOrNull { it.id == last }?.id ?: crossTeamOrder(teams).firstNotNullOfOrNull { team ->
                    boards.filter { it.teamId == team.id }.minByOrNull { it.sortOrder }?.id
                }
            }
        }
        .distinctUntilChanged()
        .flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.Eagerly, null)

    // #EXP-2109: the Assistant tab: the viewer's Assistant + their chat with it.
    @OptIn(ExperimentalCoroutinesApi::class)
    private val assistantFlow = combine(accountDatabaseFlow(auth, databaseHolder), auth.userId) { db, me -> db to me }
        .flatMapLatest { (db, me) ->
            if (db == null || me == null) flowOf(null)
            else combine(
                db.agentDefinitionDao().observeAll(),
                db.teamDao().observeAll(),
                db.sessionDao().observeAgentsNoDigest(),
            ) { agents, teams, sessions ->
                val id = AssistantSettingsRouteViewModel.assistantOf(agents, teams.map { it.id }, me)
                    ?: return@combine null
                AssistantRef(id, SessionTree.agentChat(id, null, sessions.map(SessionTree::session), me)?.id)
            }
        }
        .distinctUntilChanged()
        .flowOn(Dispatchers.Default)

    val assistant: StateFlow<AssistantRef?> = assistantFlow.stateIn(viewModelScope, SharingStarted.Eagerly, null)

    /** The Assistant as the local rows name it NOW (a cold start's deep link). */
    suspend fun assistantNow(): AssistantRef? = assistant.value ?: assistantFlow.first()

    fun setInstanceUrl(url: String) {
        viewModelScope.launch { auth.setInstanceUrl(url) }
    }

    fun clearInstance() {
        viewModelScope.launch {
            // Capture credentials BEFORE clearInstanceUrl drops the row.
            val active = activeAccount()
            val accountId = auth.activeAccountId.value
            // Awaited before the credentials drop — the unregister request
            // needs the bearer token that clearInstanceUrl removes.
            accountId?.let {
                pushTokenManager.unregisterToken(it)
                enetNodeManager.signOut(it, revoke = true)
            }
            // Revoke the server session AFTER the unregister (which needs it
            // live) and BEFORE the token drops locally (REV2-15).
            revokeSession(active)
            syncManager.signOut()
            steerConnectionStore.closeAll()
            // #EDE-114 (chat-store.json SIGN-OUT): a deliberate sign-out deletes
            // the account's chat store (and its old steer tails).
            accountId?.let { chatStores.signOut(it) }
            auth.clearInstanceUrl()
        }
    }

    fun signOut() {
        viewModelScope.launch {
            // EXP-1275 (D8.5): signing out runs behind the blocking overlay.
            Blocking.whileBlocking(FeedbackStates.BLOCKING_SIGN_OUT) {
                val active = activeAccount()
                val accountId = auth.activeAccountId.value
                accountId?.let {
                    pushTokenManager.unregisterToken(it)
                    enetNodeManager.signOut(it, revoke = true)
                }
                revokeSession(active)
                syncManager.signOut()
                steerConnectionStore.closeAll()
                // #EDE-114 (chat-store.json SIGN-OUT): a deliberate sign-out deletes
                // the account's chat store (and its old steer tails).
                accountId?.let { chatStores.signOut(it) }
                auth.clearToken()
            }
        }
    }

    /**
     * The offline banner's Retry: poll every shape of every account NOW.
     * Forced, so the kick debounce can never make a user-initiated retry
     * silently do nothing.
     */
    fun retrySync() {
        syncManager.kick("offline-banner", force = true)
    }

    fun switchAccount(id: String) {
        viewModelScope.launch { auth.switchAccount(id) }
    }

    /**
     * EXP-1635: where a sharing shortcut's id leads for THIS viewer, null when
     * `parseDirectShareId` rejects it or the agent or chat is no longer theirs
     * to see (share-targets.json: step 1 takes over).
     */
    suspend fun resolveDirectShare(id: String): com.exponential.app.data.ChatShareRoute? {
        val target = ShareTargets.parseDirectShareId(id) ?: return null
        val account = auth.activeAccountId.value ?: return null
        val db = databaseHolder.database(forAccountId = account)
        return when (target.kind) {
            ShareTargets.Kind.Agent -> {
                val row = db.agentDefinitionDao().observeById(target.id).first() ?: return null
                val visible = SessionTree.listAgents(
                    listOf(SessionTree.agentDefinition(row)),
                    emptyList(),
                    row.teamId,
                    auth.userId.value,
                )
                if (visible.isEmpty()) null else com.exponential.app.data.ChatShareRoute(row.teamId, pick = row.id)
            }
            ShareTargets.Kind.Chat -> {
                val row = db.sessionDao().observeById(target.id).first() ?: return null
                com.exponential.app.data.ChatShareRoute(row.teamId, sessionId = row.id)
            }
        }
    }

    fun removeAccount(id: String) {
        viewModelScope.launch { removeAccountAwaiting(id) }
    }

    // The body of [removeAccount], as a suspend function: callers that must
    // sequence work AFTER the removal has fully settled (signOutOfGatedServer)
    // have to await it instead of racing the coroutine removeAccount() fires
    // and forgets.
    private suspend fun removeAccountAwaiting(id: String) {
        val account = auth.accounts.value.firstOrNull { it.id == id }
        pushTokenManager.unregisterToken(id)
        enetNodeManager.signOut(id, revoke = true)
        revokeSession(account)
        steerConnectionStore.closeAll()
        chatStores.signOut(id)
        auth.removeAccount(id)
        databaseHolder.deleteFiles(id)
    }

    // The blocking 426 gate's escape hatch (REV2-18): remove the offending
    // account outright. Clearing just its token would leave that server's
    // account signed out but still present, so the gate would re-latch off its
    // own login round-trips instead of falling through to another signed-in
    // server (AccountStore.remove re-activates the most recent one) or the
    // instance picker.
    fun signOutOfGatedServer() {
        val account = activeAccount() ?: return
        viewModelScope.launch {
            // AWAIT the removal before clearing the latch. The removal's own
            // push unregister is a tRPC call to the very server that is 426ing,
            // and that response re-latches the origin through
            // HttpClientProvider's validator — a clear() issued before it lands
            // is silently undone, and the stale latch then blocks re-adding the
            // server for the rest of the app run (exactly what clear() exists
            // to prevent).
            removeAccountAwaiting(account.id)
            // Keep the latch while another account still lives on that server —
            // the same 426 gates it too.
            val origin = UpdateGate.originKey(account.instanceUrl)
            val othersOnServer = auth.accounts.value.any {
                it.id != account.id && UpdateGate.originKey(it.instanceUrl) == origin
            }
            if (!othersOnServer) updateGate.clear(account.instanceUrl)
        }
    }

    private fun activeAccount(): ServerAccount? =
        auth.activeAccountId.value?.let { id -> auth.accounts.value.firstOrNull { it.id == id } }

    // Best-effort server-side session revocation — sign-out must actually end
    // the Better Auth session, not just drop the local token copy (REV2-15).
    private suspend fun revokeSession(account: ServerAccount?) {
        val token = account?.token ?: return
        authApi.signOut(account.instanceUrl, token)
    }

}

// Re-assert cadence for the gated-account pipeline stop (REV2-18). Starts fast
// enough that the reconcile-relaunch race closes within a blink of the account
// change that opened it, then backs off to a once-a-minute insurance tick for
// as long as a signed-in server stays gated.
private const val GATED_STOP_REASSERT_MIN_MS = 200L
private const val GATED_STOP_REASSERT_MAX_MS = 60_000L

// Does this account's server sit behind a 426 update gate? Matching is by
// instance origin, the same key HttpClientProvider latches responses under.
private fun ServerAccount.isGated(gated: Map<String, UpdateGate.UpgradeInfo>): Boolean {
    val key = UpdateGate.originKey(instanceUrl) ?: return false
    return gated.containsKey(key)
}
