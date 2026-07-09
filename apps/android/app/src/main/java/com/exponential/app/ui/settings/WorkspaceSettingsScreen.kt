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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
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
import com.exponential.app.data.db.WorkspaceInviteEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.InitialsAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.onboarding.CreateProjectSheet
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection

// One confirm target per destructive/consequential settings action. Each tab
// holds a nullable [SettingsConfirm] and renders a single [SettingsConfirmDialog]
// so the mutation only fires after an explicit confirm — the one-tap
// project-delete that wiped the dogfood board is what motivated this.
private sealed interface SettingsConfirm {
    data class DeleteProject(val project: ProjectEntity) : SettingsConfirm
    data class DeleteLabel(val label: LabelEntity) : SettingsConfirm
    // isSelf distinguishes "Leave workspace" from "Remove member".
    data class RemoveMember(val row: MemberRow, val isSelf: Boolean) : SettingsConfirm
    data class ChangeRole(val row: MemberRow, val newRole: String) : SettingsConfirm
    data class RevokeInvite(val invite: WorkspaceInviteEntity) : SettingsConfirm
}

private data class ConfirmCopy(
    val title: String,
    val message: String,
    val button: String,
    val destructive: Boolean = true,
)

@Composable
private fun SettingsConfirmDialog(
    confirm: SettingsConfirm,
    state: WorkspaceSettingsState,
    viewModel: WorkspaceSettingsViewModel,
    onDismiss: () -> Unit,
) {
    val copy = when (confirm) {
        is SettingsConfirm.DeleteProject -> ConfirmCopy(
            title = "Delete project?",
            message = "Move \"${confirm.project.name}\" and all its issues, comments and " +
                "attachments to trash? You can restore it from workspace settings for 48 " +
                "hours; after that it is permanently deleted.",
            button = "Delete",
        )
        is SettingsConfirm.DeleteLabel -> ConfirmCopy(
            title = "Delete label?",
            message = "\"${confirm.label.name}\" will be removed from all issues. This cannot be undone.",
            button = "Delete",
        )
        is SettingsConfirm.RemoveMember -> if (confirm.isSelf) {
            ConfirmCopy(
                title = "Leave workspace?",
                message = "You will lose access to \"${state.workspace?.name ?: "this workspace"}\". " +
                    "An owner must invite you back.",
                button = "Leave",
            )
        } else {
            val name = userDisplayName(confirm.row.user, confirm.row.member.userId)
            ConfirmCopy(
                title = "Remove member?",
                message = "Remove $name from this workspace? They immediately lose access.",
                button = "Remove",
            )
        }
        is SettingsConfirm.ChangeRole -> {
            val name = userDisplayName(confirm.row.user, confirm.row.member.userId)
            if (confirm.newRole == DomainContract.workspaceRoleOwner) {
                ConfirmCopy(
                    title = "Make $name an owner?",
                    message = "Owners can delete projects, manage members and billing, and delete the workspace.",
                    button = "Change role",
                    destructive = false,
                )
            } else {
                ConfirmCopy(
                    title = "Change $name to member?",
                    message = "They will no longer be able to manage members, repositories, or delete projects.",
                    button = "Change role",
                    destructive = false,
                )
            }
        }
        is SettingsConfirm.RevokeInvite -> ConfirmCopy(
            title = "Revoke invite?",
            message = "The invite link stops working immediately.",
            button = "Revoke",
        )
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(copy.title) },
        text = { Text(copy.message) },
        confirmButton = {
            TextButton(onClick = {
                when (confirm) {
                    is SettingsConfirm.DeleteProject -> viewModel.deleteProject(confirm.project.id)
                    is SettingsConfirm.DeleteLabel -> viewModel.deleteLabel(confirm.label.id)
                    is SettingsConfirm.RemoveMember -> viewModel.removeMember(confirm.row.member.id)
                    is SettingsConfirm.ChangeRole -> viewModel.updateRole(confirm.row.member.id, confirm.newRole)
                    is SettingsConfirm.RevokeInvite -> viewModel.revokeInvite(confirm.invite.id)
                }
                onDismiss()
            }) {
                if (copy.destructive) {
                    Text(copy.button, color = MaterialTheme.colorScheme.error)
                } else {
                    Text(copy.button)
                }
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkspaceSettingsScreen(
    onBack: () -> Unit,
    viewModel: WorkspaceSettingsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
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

    // Owner-only controls are HIDDEN for non-owners (full web parity) — the
    // server enforces workspace-owner on these mutations anyway.
    val isOwner = state.isOwner
    // A single confirm target shared across every section, funnelled through the
    // one SettingsConfirmDialog so no mutation fires without an explicit confirm.
    var confirm by remember { mutableStateOf<SettingsConfirm?>(null) }

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
        // One scrolling sectioned screen (iOS WorkspaceSettingsView parity):
        // Projects → Repositories → Members → Invite Members → Labels → Danger.
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            ProjectsSection(state, isOwner, onConfirm = { confirm = it })
            RepositoriesSection(state, viewModel, isOwner)
            MembersSection(state, isOwner, onConfirm = { confirm = it })
            if (isOwner) InviteSection(state, viewModel, onConfirm = { confirm = it })
            LabelsSection(state, viewModel, onConfirm = { confirm = it })
            DangerZone(state, viewModel, isOwner)
        }
    }

    confirm?.let { SettingsConfirmDialog(it, state, viewModel) { confirm = null } }
}

@Composable
private fun ProjectsSection(
    state: WorkspaceSettingsState,
    isOwner: Boolean,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    var showCreateProject by remember { mutableStateOf(false) }
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
                    // Protected projects (the dogfood board) show no delete
                    // affordance to anyone; the server rejects it regardless.
                    if (isOwner && !project.isProtected) {
                        IconButton(onClick = { onConfirm(SettingsConfirm.DeleteProject(project)) }) {
                            Icon(Icons.Filled.Delete, contentDescription = "Delete project")
                        }
                    }
                }
            }
        }
        // "New project" is owner-only in workspace settings (web parity); the
        // empty-state and switcher create entries elsewhere stay open (they
        // target the user's own personal workspace via ensureDefault).
        if (isOwner) {
            OutlinedButton(onClick = { showCreateProject = true }) {
                Icon(Icons.Filled.Add, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text("New project")
            }
        }
    }

    if (showCreateProject) {
        CreateProjectSheet(
            workspaceId = state.workspace?.id,
            onCreated = { showCreateProject = false },
            onDismiss = { showCreateProject = false },
        )
    }
}

@Composable
private fun DangerZone(
    state: WorkspaceSettingsState,
    viewModel: WorkspaceSettingsViewModel,
    isOwner: Boolean,
) {
    var confirmDelete by remember { mutableStateOf(false) }
    // Delete workspace: owner-only, and never for the bootstrap feedback
    // workspace (the server rejects it too).
    if (isOwner && state.workspace?.slug != "feedback") {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionHeader("Danger zone")
            OutlinedButton(onClick = { confirmDelete = true }, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Filled.Delete, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text("Delete workspace", color = MaterialTheme.colorScheme.error)
            }
        }
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
    // A repo backing a protected project can't be removed (removal is blocked
    // server-side while any project uses it, and a protected project can't be
    // deleted to free it) — hide the affordance entirely.
    val usedByProtected = projects.any { it.isProtected && it.repositoryId == repo.id }
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
            if (isOwner && !usedByProtected) {
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
private fun MembersSection(
    state: WorkspaceSettingsState,
    isOwner: Boolean,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SectionHeader("Members")
            Spacer(Modifier.width(8.dp))
            Text(
                state.members.size.toString(),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        // A workspace must always keep at least one owner.
        val ownerCount = state.members.count { it.member.role == DomainContract.workspaceRoleOwner }
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            state.members.forEachIndexed { i, row ->
                if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                val isYou = row.member.userId == state.currentUserId
                val isLastOwner = row.member.role == DomainContract.workspaceRoleOwner && ownerCount <= 1
                // Menu contents are all owner-gated except a member's own
                // "Leave"; hide the trigger entirely when it would be empty.
                val hasActions = isOwner || (isYou && !isLastOwner)
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
                        val email = row.user?.email
                        if (!email.isNullOrBlank()) {
                            Text(
                                email,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    // Role badge pill (iOS parity).
                    Text(
                        row.member.role,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier
                            .glassButton()
                            .padding(horizontal = 8.dp, vertical = 3.dp),
                    )
                    if (hasActions) {
                        var rowMenu by remember { mutableStateOf(false) }
                        Box {
                            IconButton(onClick = { rowMenu = true }) {
                                Icon(Icons.Filled.MoreVert, contentDescription = "Member actions")
                            }
                            DropdownMenu(expanded = rowMenu, onDismissRequest = { rowMenu = false }) {
                                // Role changes + removing others are owner-only.
                                // The last owner can't be demoted or leave.
                                if (isOwner) {
                                    if (row.member.role != DomainContract.workspaceRoleOwner) {
                                        DropdownMenuItem(
                                            text = { Text("Make owner") },
                                            onClick = {
                                                rowMenu = false
                                                onConfirm(SettingsConfirm.ChangeRole(row, DomainContract.workspaceRoleOwner))
                                            },
                                        )
                                    }
                                    if (row.member.role != DomainContract.workspaceRoleMember) {
                                        DropdownMenuItem(
                                            text = { Text("Make member") },
                                            enabled = !isLastOwner,
                                            onClick = {
                                                rowMenu = false
                                                onConfirm(SettingsConfirm.ChangeRole(row, DomainContract.workspaceRoleMember))
                                            },
                                        )
                                    }
                                }
                                if (isYou) {
                                    if (!isLastOwner) {
                                        if (isOwner) HorizontalDivider()
                                        DropdownMenuItem(
                                            text = { Text("Leave workspace", color = MaterialTheme.colorScheme.error) },
                                            onClick = {
                                                rowMenu = false
                                                onConfirm(SettingsConfirm.RemoveMember(row, isSelf = true))
                                            },
                                        )
                                    }
                                } else if (isOwner) {
                                    HorizontalDivider()
                                    DropdownMenuItem(
                                        text = { Text("Remove", color = MaterialTheme.colorScheme.error) },
                                        onClick = {
                                            rowMenu = false
                                            onConfirm(SettingsConfirm.RemoveMember(row, isSelf = false))
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
}

// Owner-only invite management (iOS WorkspaceMembersSection.inviteSection): a
// "Generate invite link" button, the freshly-minted link with copy, then the
// pending-invite list with revoke.
@Composable
private fun InviteSection(
    state: WorkspaceSettingsState,
    viewModel: WorkspaceSettingsViewModel,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    // Invite links are https deep links into the web invite page; a null
    // instance URL disables Copy rather than minting a broken link.
    val inviteBase = state.instanceUrl?.trimEnd('/')
    val createdLink = state.createdInviteToken?.let { t -> inviteBase?.let { "$it/invite/$t" } }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeader("Invite members")
        Text(
            "Generate a link to invite someone to this workspace.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        OutlinedButton(onClick = { viewModel.createInvite() }, modifier = Modifier.fillMaxWidth()) {
            Icon(Icons.Filled.Add, null, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(8.dp))
            Text("Generate invite link")
        }
        if (state.createdInviteToken != null) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth().glassRow().padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text(
                    createdLink ?: state.createdInviteToken,
                    style = MaterialTheme.typography.bodySmall,
                    fontFamily = FontFamily.Monospace,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                TextButton(
                    onClick = { createdLink?.let { clipboard.setText(AnnotatedString(it)) } },
                    enabled = createdLink != null,
                ) { Text("Copy") }
            }
        }

        if (state.invites.isNotEmpty()) {
            SectionHeader("Pending")
            Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
                state.invites.forEachIndexed { i, invite ->
                    if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                    val link = inviteBase?.let { "$it/invite/${invite.token}" }
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                link ?: invite.token,
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
                        TextButton(
                            onClick = { link?.let { clipboard.setText(AnnotatedString(it)) } },
                            enabled = link != null,
                        ) { Text("Copy") }
                        IconButton(onClick = { onConfirm(SettingsConfirm.RevokeInvite(invite)) }) {
                            Icon(Icons.Filled.Delete, contentDescription = "Revoke invite")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun LabelsSection(
    state: WorkspaceSettingsState,
    viewModel: WorkspaceSettingsViewModel,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    var showCreate by remember { mutableStateOf(false) }
    // Labels are member-level (not owner-gated) — a confirmation dialog is the
    // only guard on delete.
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            SectionHeader("Labels", modifier = Modifier.weight(1f))
            OutlinedButton(onClick = { showCreate = true }) {
                Icon(Icons.Filled.Add, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(4.dp))
                Text("New label")
            }
        }
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            if (state.labels.isEmpty()) {
                Text(
                    "No labels yet.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                )
            }
            state.labels.forEachIndexed { i, label ->
                if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                LabelRow(
                    label = label,
                    viewModel = viewModel,
                    onDelete = { onConfirm(SettingsConfirm.DeleteLabel(it)) },
                )
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
private fun LabelRow(
    label: LabelEntity,
    viewModel: WorkspaceSettingsViewModel,
    onDelete: (LabelEntity) -> Unit,
) {
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
        IconButton(onClick = { onDelete(label) }) {
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
