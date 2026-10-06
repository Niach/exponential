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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.data.api.GithubInstallation
import com.exponential.app.data.api.GithubStatusResult
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.domain.BoardRepoLabel
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.GithubCopy
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.BoardRepoField
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.InviteLinkCard
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.CreateBoardSheet
import com.exponential.app.ui.onboarding.GithubRepoPickerSheet
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.domain.Prompts

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
    data class RemoveRepo(val repo: TeamRepo) : SettingsConfirm
    // SLOP-26: disconnect the viewer's OWN GitHub account (the ✕ beside
    // "Connected as …"). Repositories already added keep working.
    data object DisconnectGithub : SettingsConfirm
}

private fun installationLabel(inst: GithubInstallation) =
    GithubCopy.installationLabel(inst.accountLogin, inst.installationId)

@Composable
private fun SettingsConfirmDialog(
    confirm: SettingsConfirm,
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
    onDismiss: () -> Unit,
) {
    // EXP-1215: the copy is `prompts.json`'s ([Prompts]); the GitHub
    // disconnect keeps its own locked strings ([GithubCopy]).
    val prompt = when (confirm) {
        is SettingsConfirm.DeleteBoard -> Prompts.TrashBoard.prompt(confirm.board.name)
        is SettingsConfirm.DeleteLabel -> Prompts.DeleteLabel.prompt(confirm.label.name)
        is SettingsConfirm.RemoveMember -> if (confirm.isSelf) {
            // The team row can lag the roster: web (`members-section.tsx`)
            // asks about "this team" until the name is synced, never a
            // literal "Team".
            Prompts.LeaveTeam.prompt(state.team?.name ?: "this team")
        } else {
            Prompts.RemoveMember.prompt(userDisplayName(confirm.row.user, confirm.row.member.userId))
        }
        is SettingsConfirm.RemoveRepo -> Prompts.RemoveRepository.prompt(confirm.repo.fullName)
        is SettingsConfirm.DisconnectGithub -> Prompts.Prompt(
            title = GithubCopy.DISCONNECT_TITLE,
            body = GithubCopy.DISCONNECT_BODY,
            actions = listOf(
                Prompts.Action("cancel", "Cancel", Prompts.Role.Cancel),
                Prompts.Action("disconnect", GithubCopy.DISCONNECT, Prompts.Role.Destructive),
            ),
            focus = "cancel",
        )
        is SettingsConfirm.ChangeRole -> {
            val name = userDisplayName(confirm.row.user, confirm.row.member.userId)
            if (confirm.newRole == DomainContract.teamRoleOwner) {
                Prompts.MakeOwner.prompt(name)
            } else {
                Prompts.MakeMember.prompt(name)
            }
        }
    }
    val answer = {
        when (confirm) {
            is SettingsConfirm.DeleteBoard -> viewModel.deleteBoard(confirm.board.id)
            is SettingsConfirm.DeleteLabel -> viewModel.deleteLabel(confirm.label.id)
            is SettingsConfirm.RemoveMember -> viewModel.removeMember(confirm.row.member.id)
            is SettingsConfirm.ChangeRole -> viewModel.updateRole(confirm.row.member.id, confirm.newRole)
            is SettingsConfirm.RemoveRepo -> viewModel.removeRepo(confirm.repo.id)
            is SettingsConfirm.DisconnectGithub -> viewModel.disconnectGithub()
        }
        onDismiss()
    }
    PromptAlert(
        prompt = prompt,
        onDismiss = onDismiss,
        // Every prompt here has ONE answer beside Cancel.
        handlers = prompt.actions.filter { it.role != Prompts.Role.Cancel }.associate { it.id to answer },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamSettingsScreen(
    onBack: () -> Unit,
    viewModel: TeamSettingsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val toaster = LocalToaster.current

    // Surface transient mutation errors, and pop back once the team is
    // actually deleted (parity with the previous screen's behavior).
    LaunchedEffect(state.transient) {
        state.transient?.let {
            toaster.error(it)
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
                    TopBarBackButton(onClick = onBack)
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        // One scrolling sectioned screen (iOS TeamSettingsView parity):
        // Boards → Repositories → Members → Labels → Danger. Members carries
        // the invite-link creator (EXP-725), owner-only and absent at the seat
        // cap.
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            BoardsSection(state, viewModel, isOwner, onConfirm = { confirm = it })
            RepositoriesSection(state, viewModel, isOwner, onConfirm = { confirm = it })
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
        // Header row: title + board count + a compact "New board" pill — the
        // iOS Boards header's shape (EXP-331), now the shared [SectionHeader]
        // with its own count slot. "New board" is owner-only in team settings
        // (web parity); the empty-state and switcher create entries elsewhere
        // stay open (they target the user's default team via getDefault).
        SectionHeader("Boards") {
            if (isOwner) {
                GlassPill(
                    "New board",
                    icon = ExpIcons.uiAdd,
                    onClick = { showCreateBoard = true },
                )
            }
        }
        if (state.boards.isEmpty()) {
            Text(
                "No boards yet.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.fillMaxWidth().glassRow().padding(horizontal = 12.dp, vertical = 10.dp),
            )
        }
        // Slim per-row glass cards (iOS parity, EXP-331) instead of one tall
        // grouped section.
        var repoTarget by remember { mutableStateOf<BoardEntity?>(null) }
        state.boards.forEach { board ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.fillMaxWidth().flatRow().padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                BoardIcon(board)
                // EXP-698: the NAME owns the first line and the repo chip sits
                // under it. Side by side, a `owner/repo` chip is wide enough
                // that the board it belongs to was ellipsized to "Mobile …" —
                // the chip pushed out the one string the row exists to show.
                Column(
                    modifier = Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text(board.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    // Backing repo (one board = one repo): a chip resolving the
                    // synced repositoryId against the tRPC registry — iOS
                    // RepoNameChip parity (EXP-577).
                    val repo = state.repos.firstOrNull { it.id == board.repositoryId }
                    val repositoryId = board.repositoryId
                    if (repo != null) {
                        RepoNameChip(repo)
                    } else if (repositoryId != null) {
                        // FEED-32: a linked repo the registry copy doesn't know
                        // (connected on another client, or here a moment ago)
                        // — re-list once for it and never render NOTHING.
                        LaunchedEffect(repositoryId, state.reposLoaded) {
                            if (state.reposLoaded) viewModel.ensureRepoKnown(repositoryId)
                        }
                        Text(
                            BoardRepoLabel.trigger(
                                selectedName = null,
                                repositoryId = repositoryId,
                                loading = !state.reposLoaded,
                                resolving = state.resolvingRepoId == repositoryId,
                            ),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                // Member-level retarget → boards.setRepository (iOS parity:
                // the swap glyph opens the connected-repos picker).
                CircleIconButton(
                    ExpIcons.uiSwap,
                    contentDescription = "Change repository",
                    onClick = { repoTarget = board },
                    glyphSize = 16.dp,
                )
                // Deleting a board is owner-only (the server enforces it too);
                // the tap opens the destructive confirm dialog.
                if (isOwner) {
                    CircleIconButton(
                        ExpIcons.uiDelete,
                        contentDescription = "Delete board",
                        onClick = { onConfirm(SettingsConfirm.DeleteBoard(board)) },
                        tint = DesignTokens.Semantic.Red.copy(alpha = 0.5f),
                        glyphSize = 16.dp,
                        borderless = true,
                    )
                }
            }
        }
        repoTarget?.let { board ->
            BoardRepositorySheet(
                board = board,
                state = state,
                viewModel = viewModel,
                onDismiss = { repoTarget = null },
            )
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
    // Delete team: owner-only. An owner may delete ANY of their teams
    // including the last one (EXP-188; EXP-364 killed the feedback-team
    // special case, so no slug is exempt) — a team-less account lands
    // back in the create-or-join flow.
    if (isOwner && state.team != null) {
        // iOS TeamSettingsView parity (EXP-577): red section title and a
        // full-width red-on-glass capsule with the delete glyph.
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                "Danger zone",
                style = MaterialTheme.typography.titleSmall,
                color = DesignTokens.Semantic.Red.copy(alpha = 0.8f),
            )
            GlassPill(
                "Delete team",
                icon = ExpIcons.uiDelete,
                onClick = { confirmDelete = true },
                contentColor = DesignTokens.Semantic.Red,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }

    // The capsule above only renders with the team row, so the prompt names
    // it; should the row vanish underneath (the delete landed), the prompt
    // goes with it rather than asking about a literal "Team".
    val team = state.team
    if (confirmDelete && team != null) {
        PromptAlert(
            prompt = Prompts.DeleteTeam.prompt(team.name),
            onDismiss = { confirmDelete = false },
            handlers = mapOf(
                "delete" to {
                    confirmDelete = false
                    viewModel.deleteTeam()
                },
            ),
        )
    }
}

// The server-only repositories registry (masterplan v4 §3/§6) with the ONE
// GitHub connection block (SLOP-7/SLOP-26 canonical form, byte-identical copy
// ×4 in [GithubCopy]): header pill → intro → connection block → inline errors
// → the list. The connection is the VIEWER's OWN GitHub account; any member
// connects/adds repos (adding shares them with the team), and per-row
// management (remove) is sharer-or-owner. Connect = the link-ticket hop in a
// Custom Tab (`oauth-return?linked=github` refreshes); Install = `installUrl`
// in a Custom Tab (the guided page's "Return to the app" fires
// exponential://github-connected, which refreshes too).
@Composable
private fun RepositoriesSection(
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
    isOwner: Boolean,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    var showAddRepo by remember { mutableStateOf(false) }

    // Resume-refresh fallback (EXP-365): if the exponential://github-connected
    // deep link never arrives (older server, swallowed handoff, user closed the
    // Custom Tab), returning to this screen still re-detects. First composition
    // is covered by the ViewModel's initial load — skip it.
    var hasResumed by remember { mutableStateOf(false) }
    LifecycleResumeEffect(Unit) {
        if (hasResumed) viewModel.refreshGithub()
        hasResumed = true
        onPauseOrDispose {}
    }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // Always shown (member-level since EXP-557): the picker itself handles
        // the not-configured / not-connected states.
        SectionHeader(GithubCopy.SECTION_TITLE) {
            GlassPill(
                GithubCopy.ADD_REPOSITORY,
                icon = ExpIcons.uiGithub,
                onClick = { showAddRepo = true },
            )
        }
        val context = LocalContext.current
        GithubStatusBlock(
            status = state.githubStatus,
            failed = state.githubFailed,
            connecting = state.githubConnecting,
            onRetry = viewModel::refreshGithub,
            onConnect = {
                viewModel.connectGithub { url ->
                    CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
                }
            },
            onDisconnect = { onConfirm(SettingsConfirm.DisconnectGithub) },
        )

        // FEED-42: load/link/remove failures render inline above the list.
        state.repositoriesError?.let { message ->
            Text(
                message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier
                    .fillMaxWidth()
                    .glassRow()
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            )
        }

        // EXP-721: one self-bordered glass row per repository.
        if (state.reposLoaded && state.repos.isEmpty()) {
            Text(
                GithubCopy.NO_REPOSITORIES,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.fillMaxWidth().glassRow().padding(horizontal = 12.dp, vertical = 10.dp),
            )
        }
        state.repos.forEach { repo ->
            RepositoryRow(
                repo = repo,
                boards = state.boards,
                allRepos = state.repos,
                state = state,
                // Sharer-or-owner (EXP-557): remove. Everyone else gets a
                // read-only row (they can still code on the shared repo).
                canManage = isOwner ||
                    (state.currentUserId != null && repo.sharedBy?.id == state.currentUserId),
                viewModel = viewModel,
                onConfirm = onConfirm,
            )
        }
    }

    // FEED-42: tap adds INSIDE the sheet — it awaits repositories.add, keeps
    // itself open with the failure inline, and dismisses on success.
    val accountId = state.accountId
    val teamId = state.team?.id
    if (showAddRepo && accountId != null && teamId != null) {
        GithubRepoPickerSheet(
            accountId = accountId,
            teamId = teamId,
            onAdd = { repo ->
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
    // The whole settings state, for the board repository/branch sheet a
    // "Used by" chip opens (EXP-712 — it needs the account + team ids).
    state: TeamSettingsState,
    // Sharer-or-owner (EXP-557): gates the remove action. Non-managers see
    // the row read-only.
    canManage: Boolean,
    viewModel: TeamSettingsViewModel,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    // Tapping a "Used by" chip retargets that board — the same sheet the
    // Boards section's swap glyph opens (EXP-607), instead of a second
    // hand-rolled menu that only listed the OTHER repos.
    var retargetBoard by remember { mutableStateOf<BoardEntity?>(null) }
    // EXP-721: the row IS the card — self-bordered glass at the board/label
    // row padding, gapped from its siblings by the section.
    Column(
        modifier = Modifier.fillMaxWidth().flatRow().padding(horizontal = 12.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Icon(ExpIcons.uiRepository, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
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
                Icon(ExpIcons.uiPrivate, contentDescription = "Private", modifier = Modifier.size(13.dp), tint = tertiary)
            }
            if (canManage) {
                Spacer(Modifier.width(8.dp))
                CircleIconButton(
                    ExpIcons.uiDelete,
                    contentDescription = "Remove repository",
                    onClick = { onConfirm(SettingsConfirm.RemoveRepo(repo)) },
                    tint = DesignTokens.Semantic.Red.copy(alpha = 0.5f),
                    glyphSize = 16.dp,
                    borderless = true,
                )
            }
        }
        // "Used by" chips: the boards backed by this repo (masterplan §6).
        // Each chip carries the board's palette dot; no link/unlink/primary
        // controls — a board is a repository now.
        if (repo.boards.isNotEmpty()) {
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
                    "Not used by any board",
                    style = MaterialTheme.typography.labelSmall,
                    color = tertiary,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
            }
            repo.boards.forEach { ref ->
                val board = boards.firstOrNull { it.id == ref.id }
                // Any member can retarget a board to a different connected repo
                // (boards.setRepository is member-level since EXP-557 — the
                // registry is shared) — tap the chip to pick another repo. The
                // sheet lists ALL connected repos with this one check-marked.
                val chipClickable = board != null && allRepos.size > 1
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .glassRow()
                        .then(
                            if (chipClickable) {
                                Modifier.clickable { retargetBoard = board }
                            } else {
                                Modifier
                            },
                        )
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                ) {
                    if (board != null) {
                        BoardIcon(board, size = 14.dp)
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
            }
            // Informational for everyone (EXP-557): who shared this repo with
            // the team. Web parity: "· Shared by <name or email>" riding the
            // used-by line.
            repo.sharedBy?.let { sharer ->
                Text(
                    "· Shared by ${sharer.name?.takeIf { it.isNotBlank() } ?: sharer.email ?: "a teammate"}",
                    style = MaterialTheme.typography.labelSmall,
                    color = tertiary,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
            }
        }
    }
    retargetBoard?.let { board ->
        BoardRepositorySheet(
            board = board,
            state = state,
            viewModel = viewModel,
            onDismiss = { retargetBoard = null },
        )
    }
}

@Composable
private fun MembersSection(
    state: TeamSettingsState,
    isOwner: Boolean,
    onConfirm: (SettingsConfirm) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeader("Members")
        // A team must always keep at least one owner.
        val ownerCount = state.members.count { it.member.role == DomainContract.teamRoleOwner }
        // EXP-721: one self-bordered glass row per member (the Boards and
        // Labels idiom, now the rule on every client) — not a grouped card
        // divided by hairlines. The section's spacedBy(8.dp) owns the gaps.
        state.members.forEach { row ->
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
                // EXP-698: 12dp between the avatar, the name column, the
                // role pill and the overflow circle — the pill used to sit
                // flush against both of its neighbours.
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.fillMaxWidth().flatRow().padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                UserAvatar(user = row.user, nameOrEmail = displayName, size = 32.dp)
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
                GlassPill(row.member.role, size = PillSize.Sm, mode = PillMode.Readonly)
                // EXP-630: an email invite put this member on the roster before
                // the person signed in — a MUTED mail pill right after the role
                // says so ("Invited" / "Invite expired"), exactly as web;
                // EXP-1076: a row the Linear import seated without ever mailing
                // reads "Not invited". The invite surface itself stays web-only
                // (EXP-725).
                row.placeholder?.let { placeholder ->
                    GlassPill(
                        placeholder.label,
                        size = PillSize.Sm,
                        mode = PillMode.Readonly,
                        icon = ExpIcons.uiMail,
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
                if (hasActions) {
                    var rowMenu by remember { mutableStateOf(false) }
                    Box {
                        // Horizontal `⋯` at tertiary emphasis — iOS
                        // TeamMembersSection parity (EXP-577).
                        CircleIconButton(
                            ExpIcons.uiMore,
                            contentDescription = "Member actions",
                            onClick = { rowMenu = true },
                            // EXP-721: one glyph size across every settings
                            // entity row (boards, repos, labels, members).
                            glyphSize = 16.dp,
                            borderless = true,
                        )
                        GlassDropdownMenu(expanded = rowMenu, onDismissRequest = { rowMenu = false }) {
                            // Role changes + removing others are owner-only.
                            // The last owner can't be demoted or leave — the
                            // "Make member" item is hidden (not disabled) then.
                            if (canMakeOwner) {
                                GlassMenuItem(
                                    leadingIcon = { Icon(ExpIcons.uiOwner, contentDescription = null) },
                                    text = { Text("Make owner") },
                                    onClick = {
                                        rowMenu = false
                                        onConfirm(SettingsConfirm.ChangeRole(row, DomainContract.teamRoleOwner))
                                    },
                                )
                            }
                            if (canMakeMember) {
                                GlassMenuItem(
                                    leadingIcon = { Icon(ExpIcons.uiMember, contentDescription = null) },
                                    text = { Text("Make member") },
                                    onClick = {
                                        rowMenu = false
                                        onConfirm(SettingsConfirm.ChangeRole(row, DomainContract.teamRoleMember))
                                    },
                                )
                            }
                            if (canLeave) {
                                GlassMenuItem(
                                    leadingIcon = { Icon(ExpIcons.navSignOut, contentDescription = null) },
                                    text = { Text("Leave team") },
                                    onClick = {
                                        rowMenu = false
                                        onConfirm(SettingsConfirm.RemoveMember(row, isSelf = true))
                                    },
                                    destructive = true,
                                )
                            }
                            if (canRemove) {
                                GlassMenuItem(
                                    leadingIcon = { Icon(ExpIcons.uiRemoveMember, contentDescription = null) },
                                    text = { Text("Remove") },
                                    onClick = {
                                        rowMenu = false
                                        onConfirm(SettingsConfirm.RemoveMember(row, isSelf = false))
                                    },
                                    destructive = true,
                                )
                            }
                        }
                    }
                }
            }
        }
        // EXP-725: minting an invite link is owner-only server-side, and the
        // card REMOVES itself once the team is at its seat cap — it never
        // degrades into a disabled control or an explanation (store billing
        // policy, EXP-216).
        val teamId = state.team?.id
        if (isOwner && teamId != null) {
            InviteLinkCard(teamId = teamId)
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
        // Header row: title + label count + a compact "New label" pill — the
        // same recipe as the Boards/Repositories headers and iOS (EXP-331).
        SectionHeader("Labels") {
            GlassPill(
                "New label",
                icon = ExpIcons.uiAdd,
                onClick = { showCreate = true },
            )
        }
        if (state.labels.isEmpty()) {
            Text(
                "No labels yet.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier.fillMaxWidth().glassRow().padding(horizontal = 12.dp, vertical = 10.dp),
            )
        }
        // Slim per-row glass cards (iOS parity, EXP-331).
        state.labels.forEach { label ->
            LabelRow(
                label = label,
                viewModel = viewModel,
                onDelete = { onConfirm(SettingsConfirm.DeleteLabel(it)) },
            )
        }
    }

    if (showCreate) {
        LabelEditorDialog(
            title = "New label",
            confirmLabel = "Create",
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
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.fillMaxWidth().flatRow().padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Box(Modifier.size(12.dp).background(parseColor(label.color), CircleShape))
        Text(label.name, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        // The pencil, not an overflow glyph: the tap opens the editor sheet
        // directly, there is no menu behind it (iOS twin does the same).
        CircleIconButton(
            ExpIcons.uiEdit,
            contentDescription = "Edit label",
            onClick = { editing = true },
            glyphSize = 16.dp,
            borderless = true,
        )
        CircleIconButton(
            ExpIcons.uiDelete,
            contentDescription = "Delete label",
            onClick = { onDelete(label) },
            tint = DesignTokens.Palette.Destructive.copy(alpha = 0.7f),
            glyphSize = 16.dp,
            borderless = true,
        )
    }

    if (editing) {
        LabelEditorDialog(
            title = "Edit label",
            confirmLabel = "Save",
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
    // "Create" for the new-label flow, "Save" for edits (wording parity with
    // iOS — EXP-331).
    confirmLabel: String,
    initialName: String,
    initialColor: String,
    onConfirm: (name: String, color: String) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf(initialName) }
    var color by remember { mutableStateOf(initialColor) }
    val canConfirm = name.isNotBlank()
    // A glass bottom sheet (iOS LabelEditorSheet parity, EXP-577) instead of
    // the Material alert dialog: name field, swatch grid, and the pinned
    // confirm button the shell owns (EXP-687 — no Cancel, swipe down instead).
    GlassSheet(
        title = title,
        onDismiss = onDismiss,
        primaryAction = SheetPrimaryAction(
            label = confirmLabel,
            enabled = canConfirm,
            onClick = { onConfirm(name.trim(), color) },
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            GlassTextField(
                value = name,
                onValueChange = { name = it },
                singleLine = true,
                placeholder = "Label name",
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
                                ExpIcons.uiCheck,
                                contentDescription = null,
                                tint = Color.White,
                                modifier = Modifier.size(16.dp),
                            )
                        }
                    }
                }
            }
        }
    }
}

/**
 * `owner/name` glass chip for a board's backing repo — iOS `RepoNameChip`:
 * repository glyph, monospaced name, external-link glyph; tap opens GitHub.
 */
@Composable
private fun RepoNameChip(repo: TeamRepo) {
    val context = LocalContext.current
    GlassPill(
        repo.fullName,
        size = PillSize.Sm,
        icon = ExpIcons.uiRepository,
        fontFamily = FontFamily.Monospace,
        maxLines = 1,
        modifier = Modifier.widthIn(max = 180.dp),
        trailing = {
            Icon(
                ExpIcons.uiExternalLink,
                contentDescription = "Open on GitHub",
                modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
            )
        },
        onClick = {
            runCatching {
                context.startActivity(
                    Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/${repo.fullName}"))
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            }
        },
    )
}

/**
 * A board's repository + branch (EXP-712) — the shared [BoardRepoField] block
 * in a sheet. Every change persists immediately: the repository through
 * `boards.setRepository` (which RESETS the board's branch pin — it belonged to
 * the old repo), the branch through `boards.update`. Picking a repo the team
 * hasn't connected yet adds it to the registry first.
 */
@Composable
private fun BoardRepositorySheet(
    board: BoardEntity,
    state: TeamSettingsState,
    viewModel: TeamSettingsViewModel,
    onDismiss: () -> Unit,
) {
    val accountId = state.accountId
    val teamId = state.team?.id
    if (accountId == null || teamId == null) return
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

    // The captured [board] is a snapshot — Electric re-delivers the row after
    // each write, but not into this sheet's copy, so the picked values are
    // tracked locally and re-seeded whenever a different board opens it.
    var repositoryId by remember(board.id) { mutableStateOf(board.repositoryId) }
    var branch by remember(board.id) { mutableStateOf(board.defaultBranch) }
    var pendingInline by remember(board.id) { mutableStateOf<BoardRepositoryChoice.Inline?>(null) }

    // A repo connected from inside this sheet lands in the registry a moment
    // later; adopt its id then, so the Branch picker can list its branches.
    LaunchedEffect(state.repos, pendingInline) {
        val connected = pendingInline?.let { picked ->
            state.repos.firstOrNull { it.fullName == picked.fullName }
        }
        if (connected != null) {
            repositoryId = connected.id
            pendingInline = null
        }
    }
    // FEED-32: the board's linked repo may be one this registry copy has never
    // seen (connected on another client) — re-list ONCE for it while the field
    // reads "Loading repository…", never a blank or "No repository".
    val currentRepositoryId = repositoryId
    LaunchedEffect(currentRepositoryId, state.reposLoaded) {
        if (currentRepositoryId != null && state.reposLoaded &&
            state.repos.none { it.id == currentRepositoryId }
        ) {
            viewModel.ensureRepoKnown(currentRepositoryId)
        }
    }

    GlassSheet(title = "Repository", onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(board.name, style = MaterialTheme.typography.labelMedium, color = secondary)
            BoardRepoField(
                accountId = accountId,
                teamId = teamId,
                repos = state.repos,
                loading = !state.reposLoaded,
                resolving = repositoryId != null && state.resolvingRepoId == repositoryId,
                selection = pendingInline
                    ?: repositoryId?.let { BoardRepositoryChoice.Registry(it) },
                onSelect = { choice ->
                    // A retarget resets the pin server-side; mirror that here.
                    branch = null
                    when (choice) {
                        null -> {
                            pendingInline = null
                            repositoryId = null
                            viewModel.setBoardRepository(board.id, null)
                        }
                        is BoardRepositoryChoice.Registry -> {
                            pendingInline = null
                            repositoryId = choice.repositoryId
                            viewModel.setBoardRepository(board.id, choice.repositoryId)
                        }
                        is BoardRepositoryChoice.Inline -> {
                            pendingInline = choice
                            repositoryId = null
                            viewModel.connectBoardRepository(
                                boardId = board.id,
                                fullName = choice.fullName,
                                defaultBranch = choice.defaultBranch ?: DEFAULT_BRANCH_FALLBACK,
                                isPrivate = choice.isPrivate ?: false,
                            )
                        }
                    }
                },
                branch = branch,
                onBranchChange = { picked ->
                    branch = picked
                    viewModel.setBoardBranch(board.id, picked)
                },
            )
        }
    }
}

/** Only reached for a repo the picker returned without one (it defaults to `main` itself). */
private const val DEFAULT_BRANCH_FALLBACK = "main"

/**
 * SLOP-26 (web GithubStatusLine, iOS TeamRepositoriesSection — one canonical
 * form): the viewer's own connection from `integrations.github.status`.
 * Loading → nothing; failed → line + Retry; not configured → a line; not
 * linked → Connect GitHub; expired → Reconnect + ✕; connected → "Connected as
 * login" + ✕, the suspended line (Manage), then the accounts where the app is
 * installed (one indented row each with Configure), the caption and Install
 * on another account — or the install nudge when there are none.
 */
@Composable
private fun GithubStatusBlock(
    status: GithubStatusResult?,
    failed: Boolean,
    connecting: Boolean,
    onRetry: () -> Unit,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
) {
    val context = LocalContext.current
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val open: (String) -> Unit = { url ->
        CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
    }
    if (status == null && !failed) return
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        when {
            status == null -> StatusLine(ExpIcons.uiGithub, GithubCopy.STATUS_FAILED, secondary) {
                GlassPill(GithubCopy.RETRY, icon = ExpIcons.uiRefresh, size = PillSize.Sm, onClick = onRetry)
            }
            !status.configured -> StatusLine(ExpIcons.uiGithub, GithubCopy.NOT_CONFIGURED, secondary)
            !status.isLinked -> StatusLine(ExpIcons.uiGithub, GithubCopy.NOT_LINKED, secondary) {
                GlassPill(
                    GithubCopy.CONNECT_GITHUB,
                    icon = ExpIcons.uiGithub,
                    size = PillSize.Sm,
                    primary = true,
                    enabled = !connecting,
                    onClick = onConnect,
                )
            }
            status.needsReconnect -> StatusLine(
                ExpIcons.uiWarning,
                GithubCopy.RECONNECT_NEEDED,
                secondary,
                iconTint = DesignTokens.Semantic.Yellow,
                trailing = { DisconnectButton(onDisconnect) },
            ) {
                GlassPill(
                    GithubCopy.RECONNECT,
                    icon = ExpIcons.uiRefresh,
                    size = PillSize.Sm,
                    primary = true,
                    enabled = !connecting,
                    onClick = onConnect,
                )
            }
            else -> ConnectedAccounts(status, secondary, tertiary, open, onDisconnect)
        }
    }
}

/** One status line: leading glyph, the sentence, an optional trailing control, and pills that wrap. */
@Composable
private fun StatusLine(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    text: String,
    color: Color,
    iconTint: Color = color,
    trailing: (@Composable () -> Unit)? = null,
    actions: (@Composable () -> Unit)? = null,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(14.dp), tint = iconTint)
            Spacer(Modifier.width(8.dp))
            Text(text, style = MaterialTheme.typography.bodySmall, color = color, modifier = Modifier.weight(1f))
            trailing?.invoke()
        }
        if (actions != null) {
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.padding(start = AccountIndent),
            ) { actions() }
        }
    }
}

private val AccountIndent = 22.dp

/** The ✕ that disconnects the viewer's GitHub account (confirm-first). */
@Composable
private fun DisconnectButton(onDisconnect: () -> Unit) {
    CircleIconButton(
        ExpIcons.uiClose,
        contentDescription = GithubCopy.DISCONNECT,
        onClick = onDisconnect,
        glyphSize = 14.dp,
        borderless = true,
    )
}

@Composable
private fun ConnectedAccounts(
    status: GithubStatusResult,
    secondary: Color,
    tertiary: Color,
    open: (String) -> Unit,
    onDisconnect: () -> Unit,
) {
    val installations = status.installations
    val suspended = installations.filter { it.suspended }

    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(14.dp), contentAlignment = Alignment.Center) {
            Box(Modifier.size(8.dp).background(DesignTokens.Semantic.Green, CircleShape))
        }
        Spacer(Modifier.width(8.dp))
        Text(
            status.login?.let(GithubCopy::connectedAs) ?: GithubCopy.CONNECTED,
            style = MaterialTheme.typography.bodySmall,
            color = secondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        DisconnectButton(onDisconnect)
    }
    // GitHub suspended the app for an account (REV2-29): until it is
    // unsuspended no token mints — say so instead of looking healthy. A
    // reconnect cannot fix it; Manage opens the installation on GitHub.
    if (suspended.isNotEmpty()) {
        val manageUrl = suspended.first().manageUrl.ifEmpty { null } ?: status.installUrl
        val error = MaterialTheme.colorScheme.error
        Column(modifier = Modifier.padding(start = AccountIndent)) {
            StatusLine(
                ExpIcons.uiWarning,
                GithubCopy.suspendedLine(suspended.map(::installationLabel)),
                error,
                iconTint = error,
            ) {
                if (manageUrl != null) {
                    GlassPill(
                        GithubCopy.MANAGE,
                        size = PillSize.Sm,
                        trailing = {
                            Icon(
                                ExpIcons.uiExternalLink,
                                contentDescription = null,
                                modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
                            )
                        },
                        onClick = { open(manageUrl) },
                    )
                }
            }
        }
    }
    if (installations.isEmpty()) {
        Column(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.padding(start = AccountIndent),
        ) {
            Text(GithubCopy.NOT_INSTALLED, style = MaterialTheme.typography.bodySmall, color = secondary)
            status.installUrl?.let { installUrl ->
                GlassPill(
                    GithubCopy.INSTALL_APP,
                    icon = ExpIcons.uiAdd,
                    size = PillSize.Sm,
                    primary = true,
                    onClick = { open(installUrl) },
                )
            }
        }
        return
    }
    Text(
        GithubCopy.ACCOUNTS_HEADER,
        style = MaterialTheme.typography.bodySmall,
        color = tertiary,
        modifier = Modifier.padding(start = AccountIndent),
    )
    installations.forEach { inst ->
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth().padding(start = AccountIndent),
        ) {
            Icon(
                if (inst.accountType == "Organization") ExpIcons.uiOrganization else ExpIcons.uiAvatarPlaceholder,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = secondary,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                installationLabel(inst),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (inst.manageUrl.isNotEmpty()) {
                Spacer(Modifier.width(8.dp))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .clickable { open(inst.manageUrl) }
                        .semantics { contentDescription = GithubCopy.configureTitle(installationLabel(inst)) }
                        .padding(horizontal = 4.dp, vertical = 6.dp),
                ) {
                    Text(GithubCopy.CONFIGURE, style = MaterialTheme.typography.labelMedium, color = secondary)
                    Spacer(Modifier.width(4.dp))
                    Icon(ExpIcons.uiExternalLink, contentDescription = null, modifier = Modifier.size(12.dp), tint = secondary)
                }
            }
        }
    }
    Text(
        GithubCopy.INSTALLATION_CAPTION,
        style = MaterialTheme.typography.bodySmall,
        color = tertiary,
        modifier = Modifier.padding(start = AccountIndent),
    )
    status.installUrl?.let { installUrl ->
        GlassPill(
            GithubCopy.INSTALL_ANOTHER,
            icon = ExpIcons.uiAdd,
            size = PillSize.Sm,
            onClick = { open(installUrl) },
            modifier = Modifier.padding(start = AccountIndent),
        )
    }
}
