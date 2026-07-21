package com.exponential.app.ui.settings

import android.content.Intent
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
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
import androidx.compose.material.icons.filled.Business
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Warning
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.onboarding.GithubRepoPickerSheet
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection

// One confirm target per destructive/consequential settings action. Each tab
// holds a nullable [SettingsConfirm] and renders a single [SettingsConfirmDialog]
// so the mutation only fires after an explicit confirm — the one-tap
// board-delete that wiped the dogfood board is what motivated this.
private sealed interface SettingsConfirm {
    data class DeleteBoard(val board: BoardEntity) : SettingsConfirm
    data class DeleteLabel(val label: LabelEntity) : SettingsConfirm
    // isSelf distinguishes "Leave team" from "Remove member".
    data class RemoveMember(val row: MemberRow, val isSelf: Boolean) : SettingsConfirm
    data class ChangeRole(val row: MemberRow, val newRole: String) : SettingsConfirm
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
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
    onDismiss: () -> Unit,
) {
    val copy = when (confirm) {
        is SettingsConfirm.DeleteBoard -> ConfirmCopy(
            title = "Delete board?",
            message = "Move \"${confirm.board.name}\" and all its issues, comments and " +
                "attachments to trash? You can restore it from team settings for 48 " +
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
                title = "Leave team?",
                message = "You will lose access to \"${state.team?.name ?: "this team"}\". " +
                    "An owner must invite you back.",
                button = "Leave",
            )
        } else {
            val name = userDisplayName(confirm.row.user, confirm.row.member.userId)
            ConfirmCopy(
                title = "Remove member?",
                message = "Remove $name from this team? They immediately lose access.",
                button = "Remove",
            )
        }
        is SettingsConfirm.ChangeRole -> {
            val name = userDisplayName(confirm.row.user, confirm.row.member.userId)
            if (confirm.newRole == DomainContract.teamRoleOwner) {
                ConfirmCopy(
                    title = "Make $name an owner?",
                    message = "Owners can delete boards, manage members and billing, and delete the team.",
                    button = "Change role",
                    destructive = false,
                )
            } else {
                ConfirmCopy(
                    title = "Change $name to member?",
                    message = "They will no longer be able to manage members, repositories, or delete boards.",
                    button = "Change role",
                    destructive = false,
                )
            }
        }
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(copy.title) },
        text = { Text(copy.message) },
        confirmButton = {
            TextButton(onClick = {
                when (confirm) {
                    is SettingsConfirm.DeleteBoard -> viewModel.deleteBoard(confirm.board.id)
                    is SettingsConfirm.DeleteLabel -> viewModel.deleteLabel(confirm.label.id)
                    is SettingsConfirm.RemoveMember -> viewModel.removeMember(confirm.row.member.id)
                    is SettingsConfirm.ChangeRole -> viewModel.updateRole(confirm.row.member.id, confirm.newRole)
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
fun TeamSettingsScreen(
    onBack: () -> Unit,
    viewModel: TeamSettingsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val snackbarHostState = remember { SnackbarHostState() }

    // Surface transient mutation errors, and pop back once the team is
    // actually deleted (parity with the previous screen's behavior).
    LaunchedEffect(state.transient) {
        state.transient?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeTransient()
        }
    }
    LaunchedEffect(state.teamDeleted) {
        if (state.teamDeleted) onBack()
    }

    // Owner-only controls are HIDDEN for non-owners (full web parity) — the
    // server enforces team-owner on these mutations anyway.
    val isOwner = state.isOwner
    // A single confirm target shared across every section, funnelled through the
    // one SettingsConfirmDialog so no mutation fires without an explicit confirm.
    var confirm by remember { mutableStateOf<SettingsConfirm?>(null) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(state.team?.name ?: "Team") },
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
        // One scrolling sectioned screen (iOS TeamSettingsView parity):
        // Boards → Repositories → Members → Labels → Danger. Inviting members
        // is a web-only flow (EXP-216) — the app never offers it.
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            BoardsSection(state, viewModel, isOwner, onConfirm = { confirm = it })
            RepositoriesSection(state, viewModel, isOwner)
            MembersSection(state, isOwner, onConfirm = { confirm = it })
            LabelsSection(state, viewModel, onConfirm = { confirm = it })
            DangerZone(state, viewModel, isOwner)
        }
    }

    confirm?.let { SettingsConfirmDialog(it, state, viewModel) { confirm = null } }
}

@Composable
private fun BoardsSection(
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
    isOwner: Boolean,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    var showCreateBoard by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeader("Boards")
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            if (state.boards.isEmpty()) {
                Text(
                    "No boards yet.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                )
            }
            state.boards.forEachIndexed { i, board ->
                if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    Box(Modifier.size(10.dp).background(parseColor(board.color), CircleShape))
                    Spacer(Modifier.width(10.dp))
                    Text(board.name, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    // Protected boards (the dogfood board) show no delete
                    // affordance to anyone; the server rejects it regardless.
                    if (isOwner && !board.isProtected) {
                        IconButton(onClick = { onConfirm(SettingsConfirm.DeleteBoard(board)) }) {
                            Icon(Icons.Filled.Delete, contentDescription = "Delete board")
                        }
                    }
                }
            }
        }
        // "New board" is owner-only in team settings (web parity); the
        // empty-state and switcher create entries elsewhere stay open (they
        // target the user's default team via getDefault).
        if (isOwner) {
            OutlinedButton(onClick = { showCreateBoard = true }) {
                Icon(Icons.Filled.Add, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text("New board")
            }
        }
    }

    if (showCreateBoard) {
        CreateBoardSheet(
            teamId = state.team?.id,
            onCreated = {
                showCreateBoard = false
                // A board created with an inline repo choice upserts a registry
                // row server-side, and the registry is tRPC-only (no shape to
                // sync it back) — without this the Repositories section keeps
                // saying "No repositories connected" (EXP-187).
                viewModel.refreshRepos()
            },
            onDismiss = { showCreateBoard = false },
        )
    }
}

@Composable
private fun DangerZone(
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
    isOwner: Boolean,
) {
    var confirmDelete by remember { mutableStateOf(false) }
    // Delete team: owner-only, and never for the bootstrap feedback
    // team (the server rejects it too). An owner may delete ANY of their
    // teams including the last one (EXP-188) — a team-less account lands
    // back in the create-or-join flow.
    if (isOwner && state.team?.slug != "feedback") {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionHeader("Danger zone")
            OutlinedButton(
                onClick = { confirmDelete = true },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(Icons.Filled.Delete, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text("Delete team", color = MaterialTheme.colorScheme.error)
            }
        }
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete team?") },
            text = { Text("This permanently deletes the team and all its issues. This cannot be undone.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    viewModel.deleteTeam()
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}

// The server-only repositories registry (masterplan v4 §3/§6): a pure registry
// listing connected repos with the boards that use each (a repo backs one or
// more boards). Owners can remove a repo — blocked (CONFLICT) while any
// board still points at it. Primary-star / per-board link editing is gone
// (a board = a repository now). Connecting NEW repos happens in-app (EXP-45):
// the OAuth connect / App install hop runs in a Custom Tab, exactly like the
// repo picker — the web team settings link survives only as a fallback
// when the GitHub grant state can't be loaded at all.
/**
 * Compact capsule action (icon + label) on a glass pill — the inline header /
 * card affordance the iOS settings use for "Add repository" / "Connect GitHub".
 * Dims to quaternary emphasis and ignores taps when [enabled] is false.
 */
@Composable
private fun GlassPillButton(
    label: String,
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val fg = MaterialTheme.colorScheme.onSurface.copy(
        alpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary,
    )
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = modifier
            .glassButton()
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 12.dp, vertical = 6.dp),
    ) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(14.dp), tint = fg)
        Text(label, style = MaterialTheme.typography.labelMedium, color = fg)
    }
}

@Composable
private fun RepositoriesSection(
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
    isOwner: Boolean,
) {
    val context = LocalContext.current
    var showAddRepo by remember { mutableStateOf(false) }
    val github = state.github
    val installations = github?.installations.orEmpty()
    val needsReauth = installations.any { it.needsReauth }
    val configured = github != null && github.configured
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // Header row: title + repo count + a compact "Add repository" button
        // (owner + ≥1 linked installation), mirroring the Labels header's
        // inline action and the iOS Repositories header.
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            SectionHeader("Repositories")
            Spacer(Modifier.width(8.dp))
            Text(
                state.repos.size.toString(),
                style = MaterialTheme.typography.labelMedium,
                color = tertiary,
            )
            Spacer(Modifier.weight(1f))
            // repositories.add is owner-only server-side, and only meaningful
            // once an account is linked (the picker draws from linked repos).
            if (isOwner && installations.isNotEmpty()) {
                GlassPillButton(
                    label = "Add repository",
                    icon = Icons.Filled.Add,
                    onClick = { showAddRepo = true },
                )
            }
        }
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
                    boards = state.boards,
                    allRepos = state.repos,
                    isOwner = isOwner,
                    viewModel = viewModel,
                )
            }
        }

        // One grouped "GitHub" card (EXP-228, iOS parity): the connected-accounts
        // caption + connect/reconnect action, the installation chips, the
        // reconnect notice, and the "Manage on GitHub" link all live inside a
        // single glassRow instead of a loose vertical stack. Visible to every
        // member when an account is linked, and always to an owner (so they keep
        // the connect entry point); hidden for non-owners with no installations.
        if (installations.isNotEmpty() || isOwner) {
            // Grant-model connect (web parity, same hop as the repo picker): the
            // single-consent OAuth connect claims the installation for this team
            // AND captures the repo grants; the server's post-connect page fires
            // exponential://github-connected, which refreshes this section without
            // leaving the screen. A linked installation with no captured grants
            // (linked before grants existed, or OAuth revoked) flags `needsReauth`.
            val connectUrl = github?.connectUrl ?: github?.installUrl
            val webSettingsUrl = state.instanceUrl?.trimEnd('/')?.let { base ->
                state.team?.slug?.let { slug -> "$base/t/$slug/settings" }
            }
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .glassRow()
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                    Text(
                        "Connected GitHub accounts",
                        style = MaterialTheme.typography.labelSmall,
                        color = tertiary,
                    )
                    Spacer(Modifier.weight(1f))
                    if (isOwner) {
                        if (configured) {
                            GlassPillButton(
                                label = if (needsReauth) "Reconnect" else "Connect GitHub",
                                icon = if (needsReauth) Icons.Filled.Refresh else Icons.Filled.Code,
                                enabled = connectUrl != null,
                                onClick = {
                                    connectUrl?.let {
                                        CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(it))
                                    }
                                },
                            )
                        } else {
                            // Grant state unavailable (query failed) or the server
                            // has no GitHub App — fall back to the web team
                            // settings, which explain/handle both.
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
                                contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 8.dp, vertical = 0.dp),
                            ) {
                                Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null, modifier = Modifier.size(14.dp))
                                Spacer(Modifier.width(4.dp))
                                Text("Connect on the web", style = MaterialTheme.typography.labelMedium)
                            }
                        }
                    }
                }
                if (installations.isNotEmpty()) {
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        installations.forEach { inst ->
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp),
                                modifier = Modifier.glassButton().padding(horizontal = 10.dp, vertical = 6.dp),
                            ) {
                                Icon(
                                    if (inst.accountType == "Organization") Icons.Filled.Business else Icons.Filled.Person,
                                    contentDescription = null,
                                    modifier = Modifier.size(14.dp),
                                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                )
                                Text(
                                    inst.accountLogin ?: "Installation ${inst.installationId}",
                                    style = MaterialTheme.typography.labelMedium,
                                    maxLines = 1,
                                )
                                if (inst.needsReauth) {
                                    Icon(
                                        Icons.Filled.Warning,
                                        contentDescription = "Needs reconnect",
                                        modifier = Modifier.size(14.dp),
                                        tint = Color(0xFFEAB308),
                                    )
                                }
                            }
                        }
                    }
                } else if (isOwner) {
                    Text(
                        "No GitHub account connected yet.",
                        style = MaterialTheme.typography.labelSmall,
                        color = tertiary,
                    )
                }
                if (isOwner && needsReauth) {
                    Text(
                        "GitHub needs to be reconnected to load this team's repositories.",
                        style = MaterialTheme.typography.bodySmall,
                        color = Color(0xFFEAB308),
                    )
                }
                // Repo access itself (which repos the App may see) is granted and
                // managed on GitHub's install page — mirror the repo picker's
                // footer (Android-only extra beyond the iOS card).
                val installUrl = if (configured) github?.installUrl else null
                if (installUrl != null) {
                    TextButton(
                        onClick = {
                            CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(installUrl))
                        },
                        contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 8.dp, vertical = 0.dp),
                    ) {
                        Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null, modifier = Modifier.size(14.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Manage repositories on GitHub", style = MaterialTheme.typography.labelMedium)
                    }
                }
            }
        }
    }

    // Same picker sheet as board creation (RepositorySelector); here the pick
    // lands in the registry directly via repositories.add. The sheet calls
    // onPick then dismisses itself on selection.
    val accountId = state.accountId
    val teamId = state.team?.id
    if (showAddRepo && accountId != null && teamId != null) {
        GithubRepoPickerSheet(
            accountId = accountId,
            teamId = teamId,
            onPick = { repo ->
                viewModel.addRepository(repo.fullName, repo.defaultBranch, repo.isPrivate)
            },
            onDismiss = { showAddRepo = false },
        )
    }
}

@Composable
private fun RepositoryRow(
    repo: TeamRepo,
    boards: List<BoardEntity>,
    allRepos: List<TeamRepo>,
    isOwner: Boolean,
    viewModel: TeamSettingsViewModel,
) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    // A repo backing a protected board can't be removed (removal is blocked
    // server-side while any board uses it, and a protected board can't be
    // deleted to free it) — hide the affordance entirely.
    val usedByProtected = boards.any { it.isProtected && it.repositoryId == repo.id }
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
        // "Used by" chips: the boards backed by this repo (masterplan §6).
        // Each chip carries the board's palette dot; no link/unlink/primary
        // controls — a board is a repository now.
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
            if (repo.boards.isEmpty()) {
                Text(
                    "No boards",
                    style = MaterialTheme.typography.labelSmall,
                    color = tertiary,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
            }
            repo.boards.forEach { ref ->
                val board = boards.firstOrNull { it.id == ref.id }
                // Owners can retarget a board to a different connected repo
                // (boards.setRepository) — tap the chip to pick another repo.
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
                        if (board != null) {
                            Box(Modifier.size(10.dp).background(parseColor(board.color), CircleShape))
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
                                        viewModel.setBoardRepository(ref.id, target.id)
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
    state: TeamSettingsState,
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
        // A team must always keep at least one owner.
        val ownerCount = state.members.count { it.member.role == DomainContract.teamRoleOwner }
        Column(Modifier.fillMaxWidth().glassSection().padding(vertical = 4.dp)) {
            state.members.forEachIndexed { i, row ->
                if (i > 0) HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
                val isYou = row.member.userId == state.currentUserId
                val isLastOwner = row.member.role == DomainContract.teamRoleOwner && ownerCount <= 1
                // Each menu item gates on an explicit capability; the trigger is
                // hidden entirely when none apply (e.g. the sole owner's own row,
                // which used to show a single disabled "Make member").
                val canMakeOwner = isOwner && row.member.role != DomainContract.teamRoleOwner
                val canMakeMember = isOwner && row.member.role != DomainContract.teamRoleMember && !isLastOwner
                val canLeave = isYou && !isLastOwner
                val canRemove = isOwner && !isYou
                val hasActions = canMakeOwner || canMakeMember || canLeave || canRemove
                val displayName = userDisplayName(row.user, row.member.userId)
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                ) {
                    UserAvatar(user = row.user, nameOrEmail = displayName, size = 32.dp)
                    Spacer(Modifier.width(10.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            buildString {
                                append(displayName)
                                if (isYou) append(" (you)")
                            },
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        // Hide the sub-line when it would just repeat the primary
                        // line — a name-less Apple user's display name IS the email.
                        val email = row.user?.email
                        if (!email.isNullOrBlank() && email != displayName) {
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
                                // The last owner can't be demoted or leave — the
                                // "Make member" item is hidden (not disabled) then.
                                if (canMakeOwner) {
                                    DropdownMenuItem(
                                        text = { Text("Make owner") },
                                        onClick = {
                                            rowMenu = false
                                            onConfirm(SettingsConfirm.ChangeRole(row, DomainContract.teamRoleOwner))
                                        },
                                    )
                                }
                                if (canMakeMember) {
                                    DropdownMenuItem(
                                        text = { Text("Make member") },
                                        onClick = {
                                            rowMenu = false
                                            onConfirm(SettingsConfirm.ChangeRole(row, DomainContract.teamRoleMember))
                                        },
                                    )
                                }
                                if (canLeave) {
                                    if (canMakeOwner || canMakeMember) HorizontalDivider()
                                    DropdownMenuItem(
                                        text = { Text("Leave team", color = MaterialTheme.colorScheme.error) },
                                        onClick = {
                                            rowMenu = false
                                            onConfirm(SettingsConfirm.RemoveMember(row, isSelf = true))
                                        },
                                    )
                                }
                                if (canRemove) {
                                    if (canMakeOwner || canMakeMember) HorizontalDivider()
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

@Composable
private fun LabelsSection(
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
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
    viewModel: TeamSettingsViewModel,
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
