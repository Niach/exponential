package com.exponential.app.ui.settings

import com.exponential.app.domain.DomainContract

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
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
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
import com.exponential.app.data.db.ProjectEntity
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
    LaunchedEffect(state.workspaceDeleted) {
        if (state.workspaceDeleted) onBack()
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
                    listOf("General", "Members", "Labels").forEachIndexed { i, label ->
                        Tab(
                            selected = selectedTab == i,
                            onClick = { selectedTab = i },
                            text = { Text(label) },
                        )
                    }
                }
            }
        },
        floatingActionButton = {},
        snackbarHost = { SnackbarHost(snackbarHostState) },
        containerColor = MaterialTheme.colorScheme.background,
    ) { padding ->
        Box(modifier = Modifier.padding(padding).fillMaxSize()) {
            when (selectedTab) {
                0 -> GeneralTab(
                    workspace = state.workspace,
                    projects = state.projects,
                    onTogglePublic = viewModel::setPublic,
                    onSetPolicy = viewModel::setPublicWritePolicy,
                    onDeleteProject = viewModel::deleteProject,
                    onDeleteWorkspace = viewModel::deleteWorkspace,
                )
                1 -> MembersTab(
                    rows = state.members,
                    currentUserId = state.currentUserId,
                    onChangeRole = viewModel::updateRole,
                    onRemove = viewModel::removeMember,
                    invites = state.invites,
                    instanceUrl = state.instanceUrl,
                    onCreateInvite = viewModel::createInvite,
                    onRevokeInvite = viewModel::revokeInvite,
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
private fun GeneralTab(
    workspace: com.exponential.app.data.db.WorkspaceEntity?,
    projects: List<ProjectEntity>,
    onTogglePublic: (Boolean) -> Unit,
    onSetPolicy: (String) -> Unit,
    onDeleteProject: (String) -> Unit,
    onDeleteWorkspace: () -> Unit,
) {
    if (workspace == null) {
        Text(
            "Loading…",
            modifier = Modifier.padding(16.dp),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    val policy = workspace.publicWritePolicy ?: DomainContract.publicWritePolicyMembers
    var deleteProjectTarget by remember { mutableStateOf<ProjectEntity?>(null) }
    var showDeleteWorkspace by remember { mutableStateOf(false) }

    LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        // Public workspace toggle
        item {
            Column(modifier = Modifier.fillMaxWidth()) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Public workspace", style = MaterialTheme.typography.titleSmall)
                        Text(
                            "Anyone with the link can read this workspace.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(
                        checked = workspace.isPublic,
                        onCheckedChange = onTogglePublic,
                    )
                }
                if (workspace.isPublic) {
                    Spacer(Modifier.height(16.dp))
                    Text("Who can create issues?", style = MaterialTheme.typography.titleSmall)
                    Spacer(Modifier.height(8.dp))
                    SingleChoiceSegmentedButtonRow {
                        listOf(
                            DomainContract.publicWritePolicyMembers to "Members only",
                            DomainContract.publicWritePolicyEveryone to "Anyone signed in",
                        ).forEachIndexed { i, (value, label) ->
                            SegmentedButton(
                                selected = policy == value,
                                onClick = { onSetPolicy(value) },
                                shape = SegmentedButtonDefaults.itemShape(index = i, count = 2),
                            ) { Text(label) }
                        }
                    }
                    Spacer(Modifier.height(8.dp))
                    Text(
                        if (policy == DomainContract.publicWritePolicyEveryone)
                            "Signed-in users can create issues; non-members may only set title, description, and labels."
                        else "Only workspace members can create or edit issues.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        // Projects section
        item {
            Text(
                "Projects (${projects.size})",
                style = MaterialTheme.typography.titleSmall,
            )
        }
        if (projects.isEmpty()) {
            item {
                Text(
                    "No projects in this workspace yet.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        items(projects, key = { it.id }) { project ->
            ListItem(
                headlineContent = { Text(project.name) },
                leadingContent = {
                    Box(
                        modifier = Modifier
                            .size(10.dp)
                            .background(parseLabelColor(project.color), CircleShape),
                    )
                },
                supportingContent = {
                    Text(
                        project.prefix,
                        style = MaterialTheme.typography.bodySmall.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace),
                    )
                },
                trailingContent = {
                    IconButton(onClick = { deleteProjectTarget = project }) {
                        Icon(
                            Icons.Filled.DeleteOutline,
                            contentDescription = "Delete project",
                            tint = MaterialTheme.colorScheme.error.copy(alpha = 0.7f),
                        )
                    }
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
            )
            HorizontalDivider()
        }

        // Delete workspace (only for non-public)
        if (!workspace.isPublic) {
            item {
                Spacer(Modifier.height(16.dp))
                Text(
                    "Danger Zone",
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.error,
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    "Permanently delete this workspace and all its data.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(8.dp))
                TextButton(
                    onClick = { showDeleteWorkspace = true },
                    colors = androidx.compose.material3.ButtonDefaults.textButtonColors(
                        contentColor = MaterialTheme.colorScheme.error,
                    ),
                ) {
                    Icon(Icons.Filled.DeleteOutline, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(4.dp))
                    Text("Delete workspace")
                }
            }
        }
    }

    // Delete project confirmation dialog
    deleteProjectTarget?.let { project ->
        AlertDialog(
            onDismissRequest = { deleteProjectTarget = null },
            title = { Text("Delete project") },
            text = {
                Text("This will permanently delete ${project.name} and all its issues. This cannot be undone.")
            },
            confirmButton = {
                TextButton(onClick = {
                    onDeleteProject(project.id)
                    deleteProjectTarget = null
                }) { Text("Delete", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { deleteProjectTarget = null }) { Text("Cancel") }
            },
        )
    }

    // Delete workspace confirmation dialog
    if (showDeleteWorkspace) {
        AlertDialog(
            onDismissRequest = { showDeleteWorkspace = false },
            title = { Text("Delete workspace") },
            text = {
                Text("This will permanently delete ${workspace.name} and all its projects, issues, and data. This cannot be undone.")
            },
            confirmButton = {
                TextButton(onClick = {
                    onDeleteWorkspace()
                    showDeleteWorkspace = false
                }) { Text("Delete", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { showDeleteWorkspace = false }) { Text("Cancel") }
            },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MembersTab(
    rows: List<MemberRow>,
    currentUserId: String?,
    onChangeRole: (String, String) -> Unit,
    onRemove: (String) -> Unit,
    invites: List<WorkspaceInviteEntity>,
    instanceUrl: String?,
    onCreateInvite: () -> Unit,
    onRevokeInvite: (String) -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    if (rows.isEmpty() && invites.isEmpty()) {
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
                            listOf(DomainContract.workspaceRoleOwner, DomainContract.workspaceRoleMember).forEach { role ->
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

        // Invite section
        item {
            Spacer(Modifier.height(16.dp))
            HorizontalDivider()
            Spacer(Modifier.height(16.dp))
            Column(modifier = Modifier.padding(horizontal = 16.dp)) {
                Text("Invite Members", style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.height(4.dp))
                Text(
                    "Generate a link to invite someone to this workspace.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(12.dp))
                ExtendedFloatingActionButton(
                    onClick = onCreateInvite,
                    icon = { Icon(Icons.Filled.Add, null) },
                    text = { Text("New invite") },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            Spacer(Modifier.height(16.dp))
        }

        if (invites.isNotEmpty()) {
            item {
                Text(
                    "Pending invites",
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(horizontal = 16.dp),
                )
                Spacer(Modifier.height(8.dp))
            }
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
                            IconButton(onClick = { onRevokeInvite(invite.id) }) {
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
