package com.exponential.app.ui.settings

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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Login
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.PersonOff
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.lifecycle.compose.collectAsStateWithLifecycle
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
import com.exponential.app.data.api.UsersApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.push.PushTokenManager
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassSection
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

@HiltViewModel
class ServerDetailViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val databaseHolder: DatabaseHolder,
    private val syncManager: SyncManager,
    private val usersApi: UsersApi,
    private val pushTokenManager: PushTokenManager,
) : ViewModel() {
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts

    // Account teardown must NOT run in viewModelScope: the callers pop the nav
    // entry right after invoking it, which clears this ViewModel and cancels
    // viewModelScope — the awaited network unregister at the head of the flow
    // would be cancelled mid-flight and take the whole sign-out (removeAccount,
    // cache deletion) down with it. Same main dispatcher as viewModelScope, but
    // process-lifetime so teardown always completes; every job here is bounded
    // (the unregister is timeout-capped, the rest is local work).
    private val teardownScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

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
                deleteAccountError = e.message ?: "Account deletion failed"
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
            // Awaited before removeAccount drops the credentials the
            // unregister request authenticates with.
            pushTokenManager.unregisterToken(accountId)
            syncManager.signOut(accountId)
            auth.removeAccount(accountId)
            // Keep the server URL around so the user can hit Reauthenticate
            // without re-typing it — `setInstanceUrl` re-adds the entry with
            // a fresh `token == null` row.
            val instanceUrl = auth.accounts.value.firstOrNull { it.id == accountId }?.instanceUrl
                ?: accounts.value.firstOrNull { it.id == accountId }?.instanceUrl
            if (instanceUrl != null) auth.setInstanceUrl(instanceUrl)
        }
    }

    fun reauthenticate(instanceUrl: String) {
        auth.setInstanceUrl(instanceUrl)
    }

    fun remove(accountId: String) {
        teardownScope.launch {
            pushTokenManager.unregisterToken(accountId)
            syncManager.signOut(accountId)
            auth.removeAccount(accountId)
            databaseHolder.deleteFiles(accountId)
        }
    }
}

// iOS-parity server detail: glass-grouped sections over the shared
// AppBackground, mirroring SettingsScreen's glassSection row pattern.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ServerDetailScreen(
    accountId: String,
    onBack: () -> Unit,
    viewModel: ServerDetailViewModel = hiltViewModel(),
) {
    val accounts by viewModel.accounts.collectAsStateWithLifecycle()
    val account = accounts.firstOrNull { it.id == accountId }
    var showRemoveConfirm by remember { mutableStateOf(false) }
    var showDeleteAccountConfirm by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(account?.displayHost ?: "Server") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
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
            // Server identity card.
            Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    Icon(
                        Icons.Filled.Dns,
                        contentDescription = null,
                        modifier = Modifier.size(22.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Spacer(Modifier.width(12.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            account?.displayHost.orEmpty(),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        if (!account?.userEmail.isNullOrBlank()) {
                            Text(
                                account!!.userEmail!!,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
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

            // Actions card.
            Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                if (account?.token != null) {
                    ActionRow(
                        icon = Icons.AutoMirrored.Filled.Logout,
                        title = "Sign out",
                        onClick = {
                            viewModel.signOut(accountId)
                            onBack()
                        },
                    )
                    CardDivider()
                    // Store policy (Play "Delete account" / App Store
                    // 5.1.1(v)): account deletion must be initiable in-app.
                    ActionRow(
                        icon = Icons.Filled.PersonOff,
                        title = if (viewModel.deletingAccount) "Deleting account…" else "Delete account",
                        tint = MaterialTheme.colorScheme.error,
                        enabled = !viewModel.deletingAccount,
                        onClick = { showDeleteAccountConfirm = true },
                    )
                } else {
                    val url = account?.instanceUrl
                    ActionRow(
                        icon = Icons.AutoMirrored.Filled.Login,
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
                CardDivider()
                ActionRow(
                    icon = Icons.Filled.Delete,
                    title = "Remove server",
                    tint = MaterialTheme.colorScheme.error,
                    onClick = { showRemoveConfirm = true },
                )
            }
        }
    }

    if (showDeleteAccountConfirm) {
        AlertDialog(
            onDismissRequest = { showDeleteAccountConfirm = false },
            title = { Text("Delete your account?") },
            text = {
                Text(
                    "This permanently deletes your account on ${account?.displayHost ?: "this server"}, " +
                        "including your personal teams, issues, and comments. This cannot be undone.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    showDeleteAccountConfirm = false
                    viewModel.deleteAccount(accountId) { onBack() }
                }) {
                    Text("Delete account", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { showDeleteAccountConfirm = false }) { Text("Cancel") }
            },
        )
    }

    viewModel.deleteAccountError?.let { error ->
        AlertDialog(
            onDismissRequest = { viewModel.deleteAccountError = null },
            title = { Text("Couldn't delete account") },
            text = { Text(error) },
            confirmButton = {
                TextButton(onClick = { viewModel.deleteAccountError = null }) { Text("OK") }
            },
        )
    }

    if (showRemoveConfirm) {
        AlertDialog(
            onDismissRequest = { showRemoveConfirm = false },
            title = { Text("Remove ${account?.displayHost ?: "server"}?") },
            text = {
                Text("This will sign you out and delete cached data for this server. The server can be re-added at any time.")
            },
            confirmButton = {
                TextButton(onClick = {
                    showRemoveConfirm = false
                    viewModel.remove(accountId)
                    onBack()
                }) {
                    Text("Remove", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { showRemoveConfirm = false }) { Text("Cancel") }
            },
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

// Hairline divider between grouped-card rows (iOS Divider white@6%).
@Composable
private fun CardDivider() {
    HorizontalDivider(thickness = 0.5.dp, color = Color.White.copy(alpha = 0.06f))
}
