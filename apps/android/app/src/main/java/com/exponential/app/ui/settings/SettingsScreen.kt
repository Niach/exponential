package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.clickable
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AdminPanelSettings
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Group
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.outlined.Circle
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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.ServerAccount
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.ui.nav.LocalDrawerOpener
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.StateFlow

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val auth: AuthRepository,
    private val databaseHolder: DatabaseHolder,
) : ViewModel() {
    val email: StateFlow<String?> = auth.userEmail
    val isAdmin: StateFlow<Boolean> = auth.isAdmin
    val instanceUrl: StateFlow<String?> = auth.instanceUrl
    val accounts: StateFlow<List<ServerAccount>> = auth.accounts
    val activeAccountId: StateFlow<String?> = auth.activeAccountId

    fun switchAccount(id: String) = auth.switchAccount(id)
    fun startAddServer() = auth.startAddServer()
    fun removeAccount(id: String) {
        auth.removeAccount(id)
        databaseHolder.deleteFiles(id)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    onOpenIntegrations: () -> Unit,
    onOpenWorkspaceSettings: () -> Unit,
    onOpenAdminUsers: () -> Unit,
    onOpenAdminWorkspaces: () -> Unit,
    onSignOut: () -> Unit,
    viewModel: SettingsViewModel = hiltViewModel(),
) {
    val email by viewModel.email.collectAsState()
    val isAdmin by viewModel.isAdmin.collectAsState()
    val instanceUrl by viewModel.instanceUrl.collectAsState()
    val accounts by viewModel.accounts.collectAsState()
    val activeAccountId by viewModel.activeAccountId.collectAsState()
    val openDrawer = LocalDrawerOpener.current
    val context = androidx.compose.ui.platform.LocalContext.current
    var accountToRemove by remember { mutableStateOf<ServerAccount?>(null) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = { openDrawer() }) {
                        Icon(Icons.Filled.Menu, contentDescription = "Menu")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                ),
            )
        },
        containerColor = MaterialTheme.colorScheme.background,
    ) { padding ->
        LazyColumn(
            modifier = Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(vertical = 8.dp),
        ) {
            item {
                ListItem(
                    headlineContent = { Text("Signed in as") },
                    supportingContent = {
                        Column {
                            email?.let { Text(it) }
                            instanceUrl?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
                        }
                    },
                    leadingContent = { Icon(Icons.Filled.Person, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                )
                HorizontalDivider()
            }
            item {
                ListItem(
                    headlineContent = { Text("Servers", style = MaterialTheme.typography.titleSmall) },
                    leadingContent = { Icon(Icons.Filled.Dns, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                )
            }
            items(accounts) { account ->
                val isActive = account.id == activeAccountId
                ListItem(
                    headlineContent = { Text(account.displayHost) },
                    supportingContent = {
                        when {
                            account.userEmail != null -> Text(account.userEmail)
                            account.token == null -> Text("Signed out")
                            else -> Text("Signed in")
                        }
                    },
                    leadingContent = {
                        if (isActive) {
                            Icon(
                                Icons.Filled.CheckCircle,
                                contentDescription = "Active",
                                tint = MaterialTheme.colorScheme.primary,
                            )
                        } else {
                            Icon(Icons.Outlined.Circle, contentDescription = null)
                        }
                    },
                    trailingContent = {
                        IconButton(onClick = { accountToRemove = account }) {
                            Icon(Icons.Filled.Delete, contentDescription = "Remove")
                        }
                    },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    modifier = Modifier
                        .padding(start = 8.dp)
                        .clickable(enabled = !isActive) { viewModel.switchAccount(account.id) },
                )
            }
            item {
                ListItem(
                    headlineContent = { Text("Add server") },
                    leadingContent = { Icon(Icons.Filled.Add, contentDescription = null) },
                    trailingContent = {
                        IconButton(onClick = { viewModel.startAddServer() }) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                        }
                    },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                )
                HorizontalDivider()
            }
            item {
                ListItem(
                    headlineContent = { Text("Workspace") },
                    supportingContent = { Text("Members, invites, labels") },
                    leadingContent = { Icon(Icons.Filled.Group, contentDescription = null) },
                    modifier = Modifier.padding(0.dp),
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    trailingContent = {
                        IconButton(onClick = onOpenWorkspaceSettings) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                        }
                    },
                )
                HorizontalDivider()
            }
            item {
                ListItem(
                    headlineContent = { Text("Integrations") },
                    supportingContent = { Text("Google Calendar, push notifications") },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    trailingContent = {
                        IconButton(onClick = onOpenIntegrations) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                        }
                    },
                )
                HorizontalDivider()
            }
            if (isAdmin) {
                item {
                    ListItem(
                        headlineContent = { Text("Admin · Users") },
                        leadingContent = { Icon(Icons.Filled.AdminPanelSettings, contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                        trailingContent = {
                            IconButton(onClick = onOpenAdminUsers) {
                                Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                            }
                        },
                    )
                    HorizontalDivider()
                }
                item {
                    ListItem(
                        headlineContent = { Text("Admin · Workspaces") },
                        leadingContent = { Icon(Icons.Filled.AdminPanelSettings, contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                        trailingContent = {
                            IconButton(onClick = onOpenAdminWorkspaces) {
                                Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                            }
                        },
                    )
                    HorizontalDivider()
                }
            }
            item {
                ListItem(
                    headlineContent = { Text("Send feedback") },
                    supportingContent = { Text("Open the feedback workspace") },
                    leadingContent = { Icon(Icons.Filled.Email, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    trailingContent = {
                        IconButton(
                            onClick = {
                                val base = instanceUrl ?: return@IconButton
                                val url = "$base/w/feedback/projects/feedback"
                                runCatching {
                                    val intent = android.content.Intent(
                                        android.content.Intent.ACTION_VIEW,
                                        android.net.Uri.parse(url),
                                    )
                                    intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                                    context.startActivity(intent)
                                }
                            },
                            enabled = instanceUrl != null,
                        ) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open")
                        }
                    },
                )
                HorizontalDivider()
            }
            item {
                ListItem(
                    headlineContent = { Text("Sign out") },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.Logout, contentDescription = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
                    modifier = Modifier.padding(0.dp),
                    trailingContent = {
                        IconButton(onClick = onSignOut) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Sign out")
                        }
                    },
                )
            }
        }
    }

    val pending = accountToRemove
    if (pending != null) {
        AlertDialog(
            onDismissRequest = { accountToRemove = null },
            title = { Text("Remove ${pending.displayHost}?") },
            text = {
                Text("This will sign you out and delete cached data for this server. The server can be re-added at any time.")
            },
            confirmButton = {
                TextButton(onClick = {
                    viewModel.removeAccount(pending.id)
                    accountToRemove = null
                }) {
                    Text("Remove", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { accountToRemove = null }) { Text("Cancel") }
            },
        )
    }
}
