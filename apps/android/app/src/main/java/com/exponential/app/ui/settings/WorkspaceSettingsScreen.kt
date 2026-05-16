package com.exponential.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.DeleteOutline
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.WorkspaceInviteEntity

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkspaceSettingsScreen(
    onBack: () -> Unit,
    viewModel: WorkspaceSettingsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsState()
    var selectedTab by remember { mutableStateOf(0) }
    val snackbarHostState = remember { SnackbarHostState() }

    LaunchedEffect(state.transient) {
        state.transient?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeTransient()
        }
    }
    LaunchedEffect(state.createdInviteToken) {
        val token = state.createdInviteToken ?: return@LaunchedEffect
        val instance = state.instanceUrl ?: ""
        snackbarHostState.showSnackbar("Invite created — copy from list ($instance/invite/$token)")
        viewModel.consumeCreatedInvite()
    }

    Scaffold(
        topBar = {
            Column {
                TopAppBar(
                    title = { Text(state.workspace?.name ?: "Workspace") },
                    navigationIcon = {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    },
                    colors = TopAppBarDefaults.topAppBarColors(
                        containerColor = MaterialTheme.colorScheme.background,
                    ),
                )
                TabRow(selectedTabIndex = selectedTab) {
                    listOf("Members", "Invites", "Labels").forEachIndexed { i, label ->
                        Tab(
                            selected = selectedTab == i,
                            onClick = { selectedTab = i },
                            text = { Text(label) },
                        )
                    }
                }
            }
        },
        floatingActionButton = {
            when (selectedTab) {
                1 -> ExtendedFloatingActionButton(
                    onClick = { viewModel.createInvite() },
                    icon = { Icon(Icons.Filled.Add, null) },
                    text = { Text("New invite") },
                )
            }
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
        containerColor = MaterialTheme.colorScheme.background,
    ) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            when (selectedTab) {
                0 -> MembersTab(
                    rows = state.members,
                    currentUserId = state.currentUserId,
                    onChangeRole = viewModel::updateRole,
                    onRemove = viewModel::removeMember,
                )
                1 -> InvitesTab(
                    invites = state.invites,
                    instanceUrl = state.instanceUrl,
                    onRevoke = viewModel::revokeInvite,
                )
                2 -> LabelsTab(
                    labels = state.labels,
                    onRename = viewModel::renameLabel,
                    onDelete = viewModel::deleteLabel,
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MembersTab(
    rows: List<MemberRow>,
    currentUserId: String?,
    onChangeRole: (String, String) -> Unit,
    onRemove: (String) -> Unit,
) {
    if (rows.isEmpty()) {
        EmptyBox("No members yet.")
        return
    }
    LazyColumn(contentPadding = PaddingValues(vertical = 8.dp)) {
        items(rows, key = { it.member.id }) { row ->
            var menuOpen by remember { mutableStateOf(false) }
            ListItem(
                headlineContent = { Text(row.user?.name ?: row.user?.email ?: row.member.userId) },
                supportingContent = { Text("${row.member.role}${if (row.member.userId == currentUserId) " · you" else ""}") },
                trailingContent = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        IconButton(onClick = { menuOpen = true }) {
                            Icon(Icons.Filled.ExpandMore, contentDescription = "Change role")
                        }
                        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                            listOf("owner", "member").forEach { role ->
                                DropdownMenuItem(
                                    text = { Text(role) },
                                    onClick = {
                                        if (role != row.member.role) onChangeRole(row.member.id, role)
                                        menuOpen = false
                                    },
                                )
                            }
                        }
                        IconButton(onClick = { onRemove(row.member.id) }) {
                            Icon(Icons.Filled.DeleteOutline, contentDescription = "Remove")
                        }
                    }
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            HorizontalDivider()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun InvitesTab(
    invites: List<WorkspaceInviteEntity>,
    instanceUrl: String?,
    onRevoke: (String) -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    if (invites.isEmpty()) {
        EmptyBox("No pending invites. Tap + to create one.")
        return
    }
    LazyColumn(contentPadding = PaddingValues(vertical = 8.dp)) {
        items(invites, key = { it.id }) { invite ->
            val url = "${instanceUrl ?: ""}/invite/${invite.token}"
            ListItem(
                headlineContent = {
                    Text(
                        url,
                        style = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                        maxLines = 1,
                    )
                },
                supportingContent = { Text("${invite.role} · expires ${invite.expiresAt.substringBefore('T')}") },
                trailingContent = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        TextButton(onClick = { clipboard.setText(AnnotatedString(url)) }) {
                            Text("Copy")
                        }
                        IconButton(onClick = { onRevoke(invite.id) }) {
                            Icon(Icons.Filled.DeleteOutline, contentDescription = "Revoke")
                        }
                    }
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            HorizontalDivider()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LabelsTab(
    labels: List<LabelEntity>,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
) {
    var editing by remember { mutableStateOf<LabelEntity?>(null) }
    if (labels.isEmpty()) {
        EmptyBox("No labels yet — create one from any issue.")
        return
    }
    LazyColumn(contentPadding = PaddingValues(vertical = 8.dp)) {
        items(labels, key = { it.id }) { label ->
            ListItem(
                headlineContent = { Text(label.name) },
                leadingContent = {
                    Box(modifier = Modifier.size(14.dp).background(parseLabelColor(label.color), CircleShape))
                },
                trailingContent = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        TextButton(onClick = { editing = label }) { Text("Rename") }
                        IconButton(onClick = { onDelete(label.id) }) {
                            Icon(Icons.Filled.DeleteOutline, contentDescription = "Delete")
                        }
                    }
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            HorizontalDivider()
        }
    }
    editing?.let { current ->
        var name by remember(current.id) { mutableStateOf(current.name) }
        AlertDialog(
            onDismissRequest = { editing = null },
            title = { Text("Rename label") },
            text = {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    if (name.isNotBlank() && name != current.name) onRename(current.id, name.trim())
                    editing = null
                }) { Text("Save") }
            },
            dismissButton = {
                TextButton(onClick = { editing = null }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun EmptyBox(text: String) {
    Box(modifier = Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

private fun parseLabelColor(hex: String): Color = runCatching {
    val cleaned = hex.removePrefix("#")
    val value = if (cleaned.length == 6) "FF$cleaned" else cleaned
    Color(value.toLong(radix = 16))
}.getOrDefault(Color.Gray)
