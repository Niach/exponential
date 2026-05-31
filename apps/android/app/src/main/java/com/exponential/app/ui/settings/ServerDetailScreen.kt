package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.clickable
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Login
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.electric.SyncManager
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

@HiltViewModel
class ServerDetailViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val databaseHolder: DatabaseHolder,
    private val syncManager: SyncManager,
) : ViewModel() {
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts

    fun signOut(accountId: String) {
        viewModelScope.launch {
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
        viewModelScope.launch {
            syncManager.signOut(accountId)
            auth.removeAccount(accountId)
            databaseHolder.deleteFiles(accountId)
        }
    }
}

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

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(account?.displayHost ?: "Server") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = androidx.compose.ui.graphics.Color.Transparent,
                ),
            )
        },
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
    ) { padding ->
        LazyColumn(
            modifier = Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(vertical = 8.dp),
        ) {
            item {
                ListItem(
                    headlineContent = { Text(account?.displayHost.orEmpty()) },
                    supportingContent = {
                        Column {
                            if (!account?.userEmail.isNullOrBlank()) {
                                Text(account!!.userEmail!!)
                            }
                            Text(
                                if (account?.token == null) "Signed out" else "Signed in",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    },
                    leadingContent = { Icon(Icons.Filled.Dns, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent),
                )
                HorizontalDivider()
            }

            if (account?.token != null) {
                item {
                    ListItem(
                        headlineContent = { Text("Sign out") },
                        leadingContent = {
                            Icon(Icons.AutoMirrored.Filled.Logout, contentDescription = null)
                        },
                        colors = ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent),
                        modifier = Modifier.clickable {
                            viewModel.signOut(accountId)
                            onBack()
                        },
                    )
                    HorizontalDivider()
                }
            } else {
                item {
                    val url = account?.instanceUrl
                    ListItem(
                        headlineContent = { Text("Reauthenticate") },
                        leadingContent = {
                            Icon(Icons.AutoMirrored.Filled.Login, contentDescription = null)
                        },
                        colors = ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent),
                        modifier = Modifier.clickable(enabled = url != null) {
                            if (url != null) {
                                viewModel.reauthenticate(url)
                                onBack()
                            }
                        },
                    )
                    HorizontalDivider()
                }
            }

            item {
                ListItem(
                    headlineContent = {
                        Text("Remove server", color = MaterialTheme.colorScheme.error)
                    },
                    leadingContent = {
                        Icon(
                            Icons.Filled.Delete,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.error,
                        )
                    },
                    colors = ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent),
                    modifier = Modifier.clickable { showRemoveConfirm = true },
                )
            }
        }
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
