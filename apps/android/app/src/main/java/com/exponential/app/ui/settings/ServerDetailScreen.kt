package com.exponential.app.ui.settings

import com.exponential.app.ui.components.PickerRow
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.AuthApi
import com.exponential.app.data.api.AuthWire
import com.exponential.app.data.api.SignInMethodsDto
import com.exponential.app.data.api.UsersApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.domain.Prompts

@HiltViewModel
class ServerDetailViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val databaseHolder: DatabaseHolder,
    private val syncManager: SyncManager,
    private val usersApi: UsersApi,
    private val pushTokenManager: PushTokenManager,
    private val authApi: AuthApi,
) : ViewModel() {
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts

    /**
     * EXP-311: the account's own synced `users` row — it carries the profile
     * image the account store doesn't. Emits null until the shape lands.
     */
    fun userFor(accountId: String): Flow<UserEntity?> {
        val userId = auth.accounts.value.firstOrNull { it.id == accountId }?.userId
            ?: return flowOf(null)
        return databaseHolder.database(forAccountId = accountId).userDao().observeById(userId)
    }

    // Account teardown must NOT run in viewModelScope: the callers pop the nav
    // entry right after invoking it, which clears this ViewModel and cancels
    // viewModelScope — the awaited network unregister at the head of the flow
    // would be cancelled mid-flight and take the whole sign-out (removeAccount,
    // cache deletion) down with it. Same main dispatcher as viewModelScope, but
    // process-lifetime so teardown always completes; every job here is bounded
    // (the unregister is timeout-capped, the rest is local work).
    private val teardownScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /**
     * The account's stored timezone (null = unset, shown as "UTC", web
     * parity) and whether it has loaded — the row waits for the read, so it
     * never flashes a wrong zone.
     */
    var timezone by mutableStateOf<String?>(null)
        private set
    var timezoneLoaded by mutableStateOf(false)
        private set

    fun loadTimezone(accountId: String) {
        viewModelScope.launch {
            runCatching { usersApi.timezone(accountId) }
                .onSuccess {
                    timezone = it
                    timezoneLoaded = true
                }
        }
    }

    /** An explicit pick: optimistic, reverted when the server refuses it. */
    fun pickTimezone(accountId: String, zone: String) {
        val previous = timezone
        timezone = zone
        viewModelScope.launch {
            runCatching { usersApi.setTimezone(accountId, zone, onlyIfUnset = false) }
                .onFailure { timezone = previous }
        }
    }

    var deletingAccount by mutableStateOf(false)
        private set
    var deleteAccountError by mutableStateOf<String?>(null)

    /**
     * Store policy (Play "Delete account" / App Store 5.1.1(v)): deletion must
     * be initiable in-app. Server-side deletion first; only on success tear
     * down the local account + cache (mirrors the remove() path).
     */
    fun deleteAccount(accountId: String, onDeleted: () -> Unit) {
        if (deletingAccount) return
        deletingAccount = true
        teardownScope.launch {
            try {
                usersApi.deleteAccount(accountId)
            } catch (e: Exception) {
                deleteAccountError = trpcErrorMessage(e, "Account deletion failed")
                deletingAccount = false
                return@launch
            }
            // No push-token unregister here: deleting the user server-side
            // cascades their fcm_tokens rows away.
            syncManager.signOut(accountId)
            auth.removeAccount(accountId)
            databaseHolder.deleteFiles(accountId)
            deletingAccount = false
            onDeleted()
        }
    }

    fun signOut(accountId: String) {
        teardownScope.launch {
            // Capture URL + token BEFORE removeAccount drops the row.
            val account = auth.accounts.value.firstOrNull { it.id == accountId }
            val instanceUrl = account?.instanceUrl
            val token = account?.token
            // Awaited before removeAccount drops the credentials the
            // unregister request authenticates with.
            pushTokenManager.unregisterToken(accountId)
            // Revoke the server session AFTER the unregister (which needs it
            // live) and BEFORE the token drops locally (REV2-15).
            if (instanceUrl != null && token != null) authApi.signOut(instanceUrl, token)
            syncManager.signOut(accountId)
            auth.removeAccount(accountId)
            // Keep the server URL around so the user can hit Reauthenticate
            // without re-typing it — `setInstanceUrl` re-adds the entry with
            // a fresh `token == null` row.
            if (instanceUrl != null) auth.setInstanceUrl(instanceUrl)
        }
    }

    // ---- EXP-1126: sign-in methods ------------------------------------------
    // Every call names the SCREEN's accountId: this screen can show an account
    // that is not the active one.

    var methods by mutableStateOf<SignInMethodsState>(SignInMethodsState.Loading)
        private set

    /** The provider whose link is being started (the row reads "Redirecting…"). */
    var linkingProvider by mutableStateOf<String?>(null)
        private set

    /** One line under the section: a link/unlink outcome or a failure. */
    var methodsNotice by mutableStateOf<MethodsNotice?>(null)

    /** A removal in flight (unlink or passkey) and the refusal it hit, shown in its dialog. */
    var removing by mutableStateOf(false)
        private set
    var removeError by mutableStateOf<String?>(null)

    var changeEmail by mutableStateOf<ChangeEmailState?>(null)
        private set

    private var methodsAccountId: String? = null

    init {
        // The oauth-return deep link (MainActivity) finishes a link attempt;
        // the initial StateFlow value is not an outcome.
        viewModelScope.launch {
            auth.linkResult.drop(1).collect { result ->
                val id = methodsAccountId ?: return@collect
                if (result.accountId != null && result.accountId != id) return@collect
                linkingProvider = null
                methodsNotice = if (result.error != null) {
                    MethodsNotice(result.error, isError = true)
                } else {
                    val name = (methods as? SignInMethodsState.Ready)?.methods?.providers
                        ?.firstOrNull { it.id == result.providerId }?.name ?: result.providerId
                    MethodsNotice("$name is now linked.", isError = false)
                }
                loadMethods(id)
            }
        }
    }

    fun loadMethods(accountId: String) {
        methodsAccountId = accountId
        viewModelScope.launch {
            try {
                methods = SignInMethodsState.Ready(usersApi.signInMethods(accountId))
            } catch (e: Exception) {
                // Keep a loaded list on a failed refresh; only a first load shows the error.
                if (methods !is SignInMethodsState.Ready) {
                    methods = SignInMethodsState.Error(trpcErrorMessage(e, "Couldn't load your sign-in methods."))
                }
            }
        }
    }

    /**
     * Link a provider: mint a ticket for this account's session, remember the
     * attempt, and hand the link-mode start URL to [open] (a Custom Tab). The
     * result comes back through the oauth-return deep link → [AuthRepository.linkResult].
     */
    fun startLink(accountId: String, providerId: String, open: (String) -> Unit) {
        if (linkingProvider != null) return
        val baseUrl = auth.accounts.value.firstOrNull { it.id == accountId }?.instanceUrl ?: return
        linkingProvider = providerId
        methodsNotice = null
        viewModelScope.launch {
            try {
                val ticket = usersApi.mintSignInLinkTicket(accountId, providerId)
                val challenge = auth.beginLinkAttempt(accountId, providerId)
                open(AuthWire.linkStartUrl(baseUrl, ticket.ticket, providerId, challenge))
            } catch (e: Exception) {
                methodsNotice = MethodsNotice(trpcErrorMessage(e, "Couldn't start linking. Try again."), isError = true)
            } finally {
                // The Custom Tab owns the flow now; a user who closes it without
                // finishing must not be left with a stuck "Redirecting…" row.
                linkingProvider = null
            }
        }
    }

    fun unlink(accountId: String, providerId: String, onDone: () -> Unit) =
        remove(onDone, fallback = "Couldn't remove that sign-in method.") {
            usersApi.unlinkSignInMethod(accountId, providerId)
            loadMethods(accountId)
        }

    fun deletePasskey(accountId: String, id: String, onDone: () -> Unit) =
        remove(onDone, fallback = "Couldn't remove the passkey.") {
            usersApi.deletePasskey(accountId, id)
            loadMethods(accountId)
        }

    // A refusal (PRECONDITION_FAILED: the last way in) carries the server's
    // own message, which the dialog shows as-is.
    private fun remove(onDone: () -> Unit, fallback: String, call: suspend () -> Unit) {
        if (removing) return
        removing = true
        removeError = null
        viewModelScope.launch {
            try {
                call()
                onDone()
            } catch (e: Exception) {
                removeError = trpcErrorMessage(e, fallback)
            } finally {
                removing = false
            }
        }
    }

    fun openChangeEmail() {
        changeEmail = ChangeEmailState()
    }

    fun dismissChangeEmail() {
        changeEmail = null
    }

    /** Step 1: mail a code to the new address (also "Resend code"). */
    fun sendEmailChangeCode(accountId: String, newEmail: String) {
        val state = changeEmail ?: return
        if (state.sending || state.verifying) return
        val email = newEmail.trim()
        val current = (methods as? SignInMethodsState.Ready)?.methods?.email
            ?: auth.accounts.value.firstOrNull { it.id == accountId }?.userEmail
        if (current != null && email.equals(current, ignoreCase = true)) {
            changeEmail = state.copy(error = "That is already your email.")
            return
        }
        val account = auth.accounts.value.firstOrNull { it.id == accountId } ?: return
        val token = account.token ?: return
        changeEmail = state.copy(sending = true, error = null)
        viewModelScope.launch {
            val result = authApi.requestEmailChange(account.instanceUrl, token, email)
            val latest = changeEmail ?: return@launch
            changeEmail = result.fold(
                onSuccess = { latest.copy(step = ChangeEmailStep.Code, newEmail = email, sending = false, error = null) },
                onFailure = { latest.copy(sending = false, error = it.message ?: "Couldn't send the code.") },
            )
        }
    }

    /** Step 2: redeem the code, then re-read the session for the new identity. */
    fun confirmEmailChange(accountId: String, code: String) {
        val state = changeEmail ?: return
        if (state.sending || state.verifying) return
        val account = auth.accounts.value.firstOrNull { it.id == accountId } ?: return
        val token = account.token ?: return
        changeEmail = state.copy(verifying = true, error = null)
        viewModelScope.launch {
            val result = authApi.changeEmail(account.instanceUrl, token, state.newEmail, code.trim())
            if (result.isFailure) {
                changeEmail = changeEmail?.copy(
                    verifying = false,
                    error = result.exceptionOrNull()?.message ?: "Couldn't change the email.",
                )
                return@launch
            }
            val info = authApi.fetchSession(accountId)
            auth.updateIdentity(accountId, info?.email ?: state.newEmail, info?.name)
            changeEmail = null
            methodsNotice = MethodsNotice("Your email is now ${info?.email ?: state.newEmail}.", isError = false)
            loadMethods(accountId)
        }
    }

    /** "Use a different email": back to the address step. */
    fun backToEmailAddress() {
        changeEmail = changeEmail?.copy(step = ChangeEmailStep.Address, error = null)
    }

    fun reauthenticate(instanceUrl: String) {
        auth.setInstanceUrl(instanceUrl)
    }

    fun remove(accountId: String) {
        teardownScope.launch {
            // Capture URL + token BEFORE removeAccount drops the row.
            val account = auth.accounts.value.firstOrNull { it.id == accountId }
            val instanceUrl = account?.instanceUrl
            val token = account?.token
            pushTokenManager.unregisterToken(accountId)
            // Revoke the server session AFTER the unregister (which needs it
            // live) and BEFORE the token drops locally (REV2-15).
            if (instanceUrl != null && token != null) authApi.signOut(instanceUrl, token)
            syncManager.signOut(accountId)
            auth.removeAccount(accountId)
            databaseHolder.deleteFiles(accountId)
        }
    }
}

/** EXP-1126: the sign-in methods load. */
sealed interface SignInMethodsState {
    data object Loading : SignInMethodsState
    data class Ready(val methods: SignInMethodsDto) : SignInMethodsState
    data class Error(val message: String) : SignInMethodsState
}

data class MethodsNotice(val message: String, val isError: Boolean)

enum class ChangeEmailStep { Address, Code }

/** The change-email sheet's state machine (web `ChangeEmailDialog` twin). */
data class ChangeEmailState(
    val step: ChangeEmailStep = ChangeEmailStep.Address,
    val newEmail: String = "",
    val sending: Boolean = false,
    val verifying: Boolean = false,
    val error: String? = null,
)

// iOS-parity server detail: glass-grouped sections over the shared
// AppBackground, mirroring SettingsScreen's grouped-card row pattern.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ServerDetailScreen(
    accountId: String,
    onBack: () -> Unit,
    viewModel: ServerDetailViewModel = hiltViewModel(),
) {
    val accounts by viewModel.accounts.collectAsStateWithLifecycle()
    val account = accounts.firstOrNull { it.id == accountId }
    // EXP-311: the synced users row carries the profile image for the avatar.
    val user by remember(accountId) { viewModel.userFor(accountId) }
        .collectAsStateWithLifecycle(initialValue = null)
    var showRemoveConfirm by remember { mutableStateOf(false) }
    var showDeleteAccountConfirm by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(account?.displayName ?: "Server") },
                navigationIcon = {
                    TopBarBackButton(onClick = onBack)
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            // Identity card — signed in it shows the ACCOUNT identity (avatar
            // + "Firstname Lastname" + email, EXP-311, iOS/web parity); signed
            // out the server block stays so the row remains identifiable.
            Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    if (account != null && account.token != null && !account.userEmail.isNullOrBlank()) {
                        // Prefer the login-time name, then the synced users row
                        // (accounts persisted before the name was captured);
                        // name-less accounts (Apple sign-in) fall back to the
                        // email for the title/initials (EXP-331 iOS parity).
                        val name = account.userName?.takeIf { it.isNotBlank() }
                            ?: user?.name?.takeIf { it.isNotBlank() }
                            ?: account.userEmail.orEmpty()
                        UserAvatar(user = user, nameOrEmail = name, size = 40.dp)
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                name,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            val email = account.userEmail
                            if (!email.isNullOrBlank() && email != name) {
                                Text(
                                    email,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                    } else {
                        Icon(
                            ExpIcons.settingsServers,
                            contentDescription = null,
                            modifier = Modifier.size(22.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                account?.displayName.orEmpty(),
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Text(
                                if (account?.token == null) "Signed out" else "Signed in",
                                style = MaterialTheme.typography.labelSmall,
                                color = if (account?.token == null) {
                                    MaterialTheme.colorScheme.tertiary
                                } else {
                                    MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                                },
                            )
                        }
                    }
                }
            }

            // Pinned ×4: Timezone right under the identity card, a picker row.
            if (account?.token != null) {
                LaunchedEffect(accountId, account.token) { viewModel.loadTimezone(accountId) }
                if (viewModel.timezoneLoaded) {
                    val current = viewModel.timezone ?: "UTC"
                    val zones = remember(current) { timezoneOptions(current) }
                    Column(Modifier.fillMaxWidth().glassGroup()) {
                        PickerRow(
                            label = "Timezone",
                            value = current,
                            options = zones,
                            selected = current,
                            optionLabel = { it },
                            onSelect = { viewModel.pickTimezone(accountId, it) },
                        )
                    }
                }
            }

            // EXP-1126: how this account signs in — only with a live session.
            if (account?.token != null) {
                LaunchedEffect(accountId, account.token) { viewModel.loadMethods(accountId) }
                SignInMethodsSection(accountId = accountId, viewModel = viewModel)
            }

            // Actions card.
            Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                if (account?.token != null) {
                    ActionRow(
                        icon = ExpIcons.navSignOut,
                        title = "Sign out",
                        onClick = {
                            viewModel.signOut(accountId)
                            onBack()
                        },
                    )
                    GroupDivider()
                    // Store policy (Play "Delete account" / App Store
                    // 5.1.1(v)): account deletion must be initiable in-app.
                    ActionRow(
                        icon = ExpIcons.uiDeleteAccount,
                        title = if (viewModel.deletingAccount) "Deleting account…" else "Delete account",
                        tint = MaterialTheme.colorScheme.error,
                        enabled = !viewModel.deletingAccount,
                        onClick = { showDeleteAccountConfirm = true },
                    )
                } else {
                    val url = account?.instanceUrl
                    ActionRow(
                        icon = ExpIcons.uiSignIn,
                        title = "Reauthenticate",
                        enabled = url != null,
                        onClick = {
                            if (url != null) {
                                viewModel.reauthenticate(url)
                                onBack()
                            }
                        },
                    )
                }
                // The bundled cloud can never be removed — "Remove server" is a
                // custom-server affordance only (iOS parity, EXP-331).
                if (account?.isCloud != true) {
                    GroupDivider()
                    ActionRow(
                        icon = ExpIcons.uiDelete,
                        title = "Remove server",
                        tint = MaterialTheme.colorScheme.error,
                        onClick = { showRemoveConfirm = true },
                    )
                }
            }
        }
    }

    if (showDeleteAccountConfirm) {
        PromptAlert(
            prompt = Prompts.DeleteAccount.prompt(account?.displayName ?: "this server"),
            onDismiss = { showDeleteAccountConfirm = false },
            handlers = mapOf(
                "delete" to {
                    showDeleteAccountConfirm = false
                    viewModel.deleteAccount(accountId) { onBack() }
                },
            ),
        )
    }

    // EXP-1031: a failed delete is an error toast, consumed at once.
    val toaster = LocalToaster.current
    LaunchedEffect(viewModel.deleteAccountError) {
        viewModel.deleteAccountError?.let { error ->
            toaster.error("Couldn't delete account", description = error)
            viewModel.deleteAccountError = null
        }
    }

    if (showRemoveConfirm) {
        PromptAlert(
            prompt = Prompts.RemoveServer.prompt(account?.displayName ?: "this server"),
            onDismiss = { showRemoveConfirm = false },
            handlers = mapOf(
                "remove" to {
                    showRemoveConfirm = false
                    viewModel.remove(accountId)
                    onBack()
                },
            ),
        )
    }
}

// One tappable action row inside a glass section: leading icon + title (iOS
// settingsRow, same pattern as SettingsScreen).
@Composable
private fun ActionRow(
    icon: ImageVector,
    title: String,
    enabled: Boolean = true,
    tint: Color? = null,
    onClick: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(22.dp),
            tint = tint ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Text(
            title,
            style = MaterialTheme.typography.bodyMedium,
            color = tint ?: MaterialTheme.colorScheme.onSurface,
        )
    }
}


/** Every IANA zone the runtime knows, sorted; a stored zone it lacks still lists. */
internal fun timezoneOptions(current: String): List<String> {
    val zones = java.time.ZoneId.getAvailableZoneIds()
        .filter { '/' in it || it == "UTC" }
        .sorted()
    return if (current in zones) zones else listOf(current) + zones
}
