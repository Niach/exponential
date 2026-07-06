package com.exponential.app.ui.settings

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.ProjectEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.InitialsAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.onboarding.CreateProjectSheet
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection

private enum class WsTab(val label: String) { General("General"), Members("Members"), Labels("Labels") }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkspaceSettingsScreen(
    onBack: () -> Unit,
    viewModel: WorkspaceSettingsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var tab by remember { mutableStateOf(WsTab.General) }
    val snackbarHostState = remember { SnackbarHostState() }

    // Surface transient mutation errors, and pop back once the workspace is
    // actually deleted (parity with the previous screen's behavior).
    LaunchedEffect(state.transient) {
        state.transient?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeTransient()
        }
    }
    LaunchedEffect(state.workspaceDeleted) {
        if (state.workspaceDeleted) onBack()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(state.workspace?.name ?: "Workspace") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent),
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
        containerColor = Color.Transparent,
    ) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxWidth()) {
            TabRow(selectedTabIndex = tab.ordinal, containerColor = Color.Transparent) {
                WsTab.entries.forEach { t ->
                    Tab(
                        selected = tab == t,
                        onClick = { tab = t },
                        text = { Text(t.label) },
                    )
                }
            }
            when (tab) {
                WsTab.General -> GeneralTab(state, viewModel)
                WsTab.Members -> MembersTab(state, viewModel)
                WsTab.Labels -> LabelsTab(state, viewModel)
            }
        }
    }
}

@Composable
private fun GeneralTab(state: WorkspaceSettingsState, viewModel: WorkspaceSettingsViewModel) {
    var confirmDelete by remember { mutableStateOf(false) }
    var showCreateProject by remember { mutableStateOf(false) }
    // Repository management is owner-only (the server enforces workspace-owner
    // on the `repositories` router mutations); everyone else reads the registry.
    val isOwner = state.currentUserId != null && state.members.any {
        it.member.userId == state.currentUserId &&
            it.member.role == DomainContract.workspaceRoleOwner
    }
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionHeader("Visibility")
            Column(Modifier.fillMaxWidth().glassSection().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (state.workspace?.isPublic == true) "Public" else "Private",
                        modifier = Modifier.weight(1f),
                    )
                    OutlinedButton(onClick = { viewModel.setPublic(state.workspace?.isPublic != true) }) {
                        Text(if (state.workspace?.isPublic == true) "Make private" else "Make public")
                    }
                }
                if (state.workspace?.isPublic == true) {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf(
                            DomainContract.publicWritePolicyMembers to "Members",
                            DomainContract.publicWritePolicyEveryone to "Anyone",
                        ).forEach { (policy, label) ->
                            val selected = state.workspace?.publicWritePolicy == policy
                            OutlinedButton(onClick = { viewModel.setPublicWritePolicy(policy) }) {
                                if (selected) {
                                    Icon(Icons.Filled.Check, null, modifier = Modifier.size(16.dp))
                                    Spacer(Modifier.width(4.dp))
                                }
                                Text(label)
                            }
                        }
                    }
                }
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionHeader("Projects")
            Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                if (state.projects.isEmpty()) {
                    Text(
                        "No projects yet.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                }
                state.projects.forEachIndexed { i, project ->
                    if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
                    ) {
                        Box(Modifier.size(10.dp).background(parseColor(project.color), CircleShape))
                        Spacer(Modifier.width(10.dp))
                        Text(project.name, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        IconButton(onClick = { viewModel.deleteProject(project.id) }) {
                            Icon(Icons.Filled.Delete, contentDescription = "Delete project")
                        }
                    }
                }
            }
            OutlinedButton(onClick = { showCreateProject = true }) {
                Icon(Icons.Filled.Add, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text("New project")
            }
        }

        RepositoriesSection(state, viewModel, isOwner)

        OutlinedButton(onClick = { confirmDelete = true }) {
            Icon(Icons.Filled.Delete, null, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(8.dp))
            Text("Delete workspace", color = MaterialTheme.colorScheme.error)
        }
    }

    if (showCreateProject) {
        CreateProjectSheet(
            workspaceId = state.workspace?.id,
            onCreated = { showCreateProject = false },
            onDismiss = { showCreateProject = false },
        )
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete workspace?") },
            text = { Text("This permanently deletes the workspace and all its issues. This cannot be undone.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    viewModel.deleteWorkspace()
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}

// The server-only repositories registry (masterplan v4 §3/§6): a pure registry
// listing connected repos with the projects that use each (a repo backs one or
// more projects). Owners can remove a repo — blocked (CONFLICT) while any
// project still points at it. Primary-star / per-project link editing is gone
// (a project = a repository now). Connecting NEW repos (the GitHub-App install
// flow) is web-only — we link out to the web workspace settings for that.
@Composable
private fun RepositoriesSection(
    state: WorkspaceSettingsState,
    viewModel: WorkspaceSettingsViewModel,
    isOwner: Boolean,
) {
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeader("Repositories")
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            if (state.repos.isEmpty()) {
                Text(
                    "No repositories connected.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                )
            }
            state.repos.forEachIndexed { i, repo ->
                if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                RepositoryRow(
                    repo = repo,
                    projects = state.projects,
                    allRepos = state.repos,
                    isOwner = isOwner,
                    viewModel = viewModel,
                )
            }
        }
        if (isOwner) {
            val webSettingsUrl = state.instanceUrl?.trimEnd('/')?.let { base ->
                state.workspace?.slug?.let { slug -> "$base/w/$slug/settings" }
            }
            TextButton(
                onClick = {
                    webSettingsUrl?.let { url ->
                        runCatching {
                            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
                                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                            context.startActivity(intent)
                        }
                    }
                },
                enabled = webSettingsUrl != null,
            ) {
                Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null, modifier = Modifier.size(14.dp))
                Spacer(Modifier.width(4.dp))
                Text("Connect repositories on the web", style = MaterialTheme.typography.labelMedium)
            }
        }
    }
}

@Composable
private fun RepositoryRow(
    repo: WorkspaceRepo,
    projects: List<ProjectEntity>,
    allRepos: List<WorkspaceRepo>,
    isOwner: Boolean,
    viewModel: WorkspaceSettingsViewModel,
) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Icon(Icons.Filled.Code, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
            Spacer(Modifier.width(8.dp))
            Text(
                repo.fullName,
                style = MaterialTheme.typography.bodyMedium,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            Text(repo.defaultBranch, style = MaterialTheme.typography.labelSmall, color = tertiary)
            if (repo.isPrivate) {
                Spacer(Modifier.width(6.dp))
                Icon(Icons.Filled.Lock, contentDescription = "Private", modifier = Modifier.size(13.dp), tint = tertiary)
            }
            if (isOwner) {
                IconButton(onClick = { viewModel.removeRepo(repo.id) }) {
                    Icon(Icons.Filled.Delete, contentDescription = "Remove repository")
                }
            }
        }
        // "Used by" chips: the projects backed by this repo (masterplan §6).
        // Each chip carries the project's palette dot; no link/unlink/primary
        // controls — a project is a repository now.
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "Used by",
                style = MaterialTheme.typography.labelSmall,
                color = tertiary,
            )
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (repo.projects.isEmpty()) {
                Text(
                    "No projects",
                    style = MaterialTheme.typography.labelSmall,
                    color = tertiary,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
            }
            repo.projects.forEach { ref ->
                val project = projects.firstOrNull { it.id == ref.id }
                // Owners can retarget a project to a different connected repo
                // (projects.setRepository) — tap the chip to pick another repo.
                val otherRepos = allRepos.filter { it.id != repo.id }
                var retargetMenu by remember(ref.id) { mutableStateOf(false) }
                val chipClickable = isOwner && otherRepos.isNotEmpty()
                Box {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier
                            .glassRow()
                            .then(if (chipClickable) Modifier.clickable { retargetMenu = true } else Modifier)
                            .padding(horizontal = 8.dp, vertical = 4.dp),
                    ) {
                        if (project != null) {
                            Box(Modifier.size(10.dp).background(parseColor(project.color), CircleShape))
                            Spacer(Modifier.width(6.dp))
                        }
                        Text(
                            ref.name,
                            style = MaterialTheme.typography.labelMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.widthIn(max = 160.dp),
                        )
                    }
                    if (chipClickable) {
                        DropdownMenu(expanded = retargetMenu, onDismissRequest = { retargetMenu = false }) {
                            Text(
                                "Change repository",
                                style = MaterialTheme.typography.labelSmall,
                                color = tertiary,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                            )
                            otherRepos.forEach { target ->
                                DropdownMenuItem(
                                    text = { Text(target.fullName, fontFamily = FontFamily.Monospace) },
                                    leadingIcon = { Icon(Icons.Filled.Code, contentDescription = null, modifier = Modifier.size(14.dp)) },
                                    onClick = {
                                        retargetMenu = false
                                        viewModel.setProjectRepository(ref.id, target.id)
                                    },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MembersTab(state: WorkspaceSettingsState, viewModel: WorkspaceSettingsViewModel) {
    var showInvite by remember { mutableStateOf(false) }
    val clipboard = LocalClipboardManager.current
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            SectionHeader("Members", modifier = Modifier.weight(1f))
            OutlinedButton(onClick = { showInvite = true }) {
                Icon(Icons.Filled.Add, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text("Invite")
            }
        }
        // A workspace must always keep at least one owner.
        val ownerCount = state.members.count { it.member.role == DomainContract.workspaceRoleOwner }
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            state.members.forEachIndexed { i, row ->
                if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                val isYou = row.member.userId == state.currentUserId
                val isLastOwner = row.member.role == DomainContract.workspaceRoleOwner && ownerCount <= 1
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                ) {
                    InitialsAvatar(userDisplayName(row.user, row.member.userId), size = 32.dp)
                    Spacer(Modifier.width(10.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            buildString {
                                append(userDisplayName(row.user, row.member.userId))
                                if (isYou) append(" (you)")
                            },
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            row.member.role,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    var rowMenu by remember { mutableStateOf(false) }
                    Box {
                        IconButton(onClick = { rowMenu = true }) {
                            Icon(Icons.Filled.MoreVert, contentDescription = "Member actions")
                        }
                        DropdownMenu(expanded = rowMenu, onDismissRequest = { rowMenu = false }) {
                            // Roles are owner or member. The last owner can't be
                            // demoted or leave.
                            if (row.member.role != DomainContract.workspaceRoleOwner) {
                                DropdownMenuItem(
                                    text = { Text("Make owner") },
                                    onClick = {
                                        rowMenu = false
                                        viewModel.updateRole(row.member.id, DomainContract.workspaceRoleOwner)
                                    },
                                )
                            }
                            if (row.member.role != DomainContract.workspaceRoleMember) {
                                DropdownMenuItem(
                                    text = { Text("Make member") },
                                    enabled = !isLastOwner,
                                    onClick = {
                                        rowMenu = false
                                        viewModel.updateRole(row.member.id, DomainContract.workspaceRoleMember)
                                    },
                                )
                            }
                            if (isYou) {
                                if (!isLastOwner) {
                                    HorizontalDivider()
                                    DropdownMenuItem(
                                        text = { Text("Leave workspace", color = MaterialTheme.colorScheme.error) },
                                        onClick = {
                                            rowMenu = false
                                            viewModel.removeMember(row.member.id)
                                        },
                                    )
                                }
                            } else {
                                HorizontalDivider()
                                DropdownMenuItem(
                                    text = { Text("Remove", color = MaterialTheme.colorScheme.error) },
                                    onClick = {
                                        rowMenu = false
                                        viewModel.removeMember(row.member.id)
                                    },
                                )
                            }
                        }
                    }
                }
            }
        }

        if (state.invites.isNotEmpty()) {
            SectionHeader("Pending invites")
            Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                state.invites.forEachIndexed { i, invite ->
                    if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                    val link = "exponential://invite/${invite.token}"
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                link,
                                style = MaterialTheme.typography.bodySmall,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                "${invite.role} · expires ${invite.expiresAt.substringBefore('T')}",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                        TextButton(onClick = { clipboard.setText(AnnotatedString(link)) }) { Text("Copy") }
                        IconButton(onClick = { viewModel.revokeInvite(invite.id) }) {
                            Icon(Icons.Filled.Delete, contentDescription = "Revoke invite")
                        }
                    }
                }
            }
        }

        if (showInvite) {
            InviteDialog(
                state = state,
                viewModel = viewModel,
                onCopy = { clipboard.setText(AnnotatedString(it)) },
                onDismiss = {
                    showInvite = false
                    viewModel.consumeCreatedInvite()
                },
            )
        }
    }
}

@Composable
private fun InviteDialog(
    state: WorkspaceSettingsState,
    viewModel: WorkspaceSettingsViewModel,
    onCopy: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val token = state.createdInviteToken
    val link = token?.let { "exponential://invite/$it" }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Invite member") },
        text = {
            Column {
                Text("Generate an invite link to share.")
                if (link != null) {
                    Spacer(Modifier.height(8.dp))
                    Text(link, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary)
                }
            }
        },
        confirmButton = {
            if (link != null) {
                TextButton(onClick = { onCopy(link) }) { Text("Copy link") }
            } else {
                TextButton(onClick = { viewModel.createInvite() }) { Text("Generate link") }
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Close") } },
    )
}

@Composable
private fun LabelsTab(state: WorkspaceSettingsState, viewModel: WorkspaceSettingsViewModel) {
    var showCreate by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            SectionHeader("Labels", modifier = Modifier.weight(1f))
            OutlinedButton(onClick = { showCreate = true }) {
                Icon(Icons.Filled.Add, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text("New label")
            }
        }
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            state.labels.forEachIndexed { i, label ->
                if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                LabelRow(label = label, viewModel = viewModel)
            }
        }
    }

    if (showCreate) {
        LabelEditorDialog(
            title = "New label",
            initialName = "",
            initialColor = LabelPalette.colors.first(),
            onConfirm = { name, color ->
                if (name.isNotBlank()) viewModel.createLabel(name, color)
                showCreate = false
            },
            onDismiss = { showCreate = false },
        )
    }
}

@Composable
private fun LabelRow(label: LabelEntity, viewModel: WorkspaceSettingsViewModel) {
    var editing by remember { mutableStateOf(false) }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
    ) {
        Box(Modifier.size(12.dp).background(parseColor(label.color), CircleShape))
        Spacer(Modifier.width(10.dp))
        Text(label.name, modifier = Modifier.weight(1f))
        IconButton(onClick = { editing = true }) {
            Icon(Icons.Filled.MoreVert, contentDescription = "Edit label")
        }
        IconButton(onClick = { viewModel.deleteLabel(label.id) }) {
            Icon(Icons.Filled.Delete, contentDescription = "Delete label")
        }
    }

    if (editing) {
        LabelEditorDialog(
            title = "Edit label",
            initialName = label.name,
            initialColor = label.color,
            onConfirm = { name, color ->
                if (name.isNotBlank() && name != label.name) viewModel.renameLabel(label.id, name)
                if (!color.equals(label.color, ignoreCase = true)) viewModel.recolorLabel(label.id, color)
                editing = false
            },
            onDismiss = { editing = false },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LabelEditorDialog(
    title: String,
    initialName: String,
    initialColor: String,
    onConfirm: (name: String, color: String) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf(initialName) }
    var color by remember { mutableStateOf(initialColor) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    singleLine = true,
                    label = { Text("Name") },
                    modifier = Modifier.fillMaxWidth(),
                )
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    LabelPalette.colors.forEach { swatch ->
                        val selected = swatch.equals(color, ignoreCase = true)
                        Box(
                            modifier = Modifier
                                .size(28.dp)
                                .background(parseColor(swatch), CircleShape)
                                .then(
                                    if (selected) {
                                        Modifier.border(2.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                                    } else Modifier,
                                )
                                .clickable { color = swatch },
                            contentAlignment = Alignment.Center,
                        ) {
                            if (selected) {
                                Icon(
                                    Icons.Filled.Check,
                                    contentDescription = null,
                                    tint = Color.White,
                                    modifier = Modifier.size(16.dp),
                                )
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = { onConfirm(name.trim(), color) }) { Text("Save") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
