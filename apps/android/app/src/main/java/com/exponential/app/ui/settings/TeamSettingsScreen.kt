package com.exponential.app.ui.settings

import com.exponential.app.ui.components.EmptyState
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.TeamImage
import com.exponential.app.domain.TeamZone
import com.exponential.app.domain.MergeTrain
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.SectionBand
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.buildAnnotatedString
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material3.CenterAlignedTopAppBar
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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.platform.testTag
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
import com.exponential.app.domain.SettingsNav
import com.exponential.app.domain.inviteExpiryLabel
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.data.api.PutAwayBoard
import com.exponential.app.domain.PutAwayBoardCaption
import com.exponential.app.ui.components.boardIconName
import androidx.compose.runtime.produceState
import com.exponential.app.ui.components.DangerZoneSection
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.BoardRepoField
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.InviteLinkCard
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassOAuthButton
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.ScheduleZonePicker
import com.exponential.app.ui.components.TeamImageRow
import com.exponential.app.ui.components.InlineStatus
import com.exponential.app.ui.components.InlineStatusState
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.components.BranchPickerSheet
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.theme.glassGroup

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

/**
 * #EXP-1381 (settings-nav.json): ONE team settings category as its own page
 * (`team-settings/{section}`, [TeamSettingsViewModel.section]): the row's
 * label as the title + back, only that category's content. General = the
 * team's identity + the Danger zone; Members, Labels, Boards, Repositories =
 * their section. (Agents, Prompts and Devices are their own screens.)
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamSettingsScreen(
    onBack: () -> Unit,
    viewModel: TeamSettingsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val section = viewModel.section

    // FEED-79: a failed action is an error toast; pop back once the team is
    // actually deleted (parity with the previous screen's behavior).
    FailedActionToast(state.transient, onShown = viewModel::consumeTransient)
    LaunchedEffect(state.teamDeleted) {
        if (state.teamDeleted) onBack()
    }

    // Owner-only controls are HIDDEN for non-owners (full web parity) — the
    // server enforces team-owner on these mutations anyway.
    val isOwner = state.isOwner
    // A single confirm target, funnelled through the one SettingsConfirmDialog
    // so no mutation fires without an explicit confirm.
    var confirm by remember { mutableStateOf<SettingsConfirm?>(null) }
    val title = section?.let { id -> SettingsNav.ROWS.firstOrNull { it.id == id }?.label } ?: "Settings"

    Scaffold(
        topBar = {
            // EXP-1554: the phone page title is centred (iOS, the app editor).
            CenterAlignedTopAppBar(
                title = { TopBarTitle(title) },
                navigationIcon = {
                    TopBarBackButton(onClick = onBack)
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
        containerColor = Color.Transparent,
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            val all = section == null
            if (all || section == SettingsNav.GENERAL) TeamGeneral(state, viewModel)
            if (all || section == SettingsNav.BOARDS) {
                BoardsSection(state, viewModel, isOwner, onConfirm = { confirm = it })
                if (isOwner) PutAwayBoardsBands(viewModel)
            }
            if (all || section == SettingsNav.REPOSITORIES) RepositoriesSection(state, viewModel, isOwner, onConfirm = { confirm = it })
            if (all || section == SettingsNav.MEMBERS) {
                MembersSection(
                    state,
                    viewModel,
                    isOwner,
                    onConfirm = { confirm = it },
                    onRevokeInvite = viewModel::revokeInvite,
                )
            }
            if (all || section == SettingsNav.LABELS) LabelsSection(state, viewModel, onConfirm = { confirm = it })
            if (all || section == SettingsNav.GENERAL) DangerZone(state, viewModel, isOwner)
        }
    }

    confirm?.let { SettingsConfirmDialog(it, state, viewModel) { confirm = null } }
}

/**
 * General (web `TeamGeneralSection`): the General band over the Name row
 * (saves itself on blur and Done, EXP-818), the Merge select (#EXP-1710,
 * merge-train.json MODE), then the Team prompt band (its autosave caption) over the raw
 * monospace field and its byte / token counter (EXP-1025).
 */
@Composable
private fun TeamGeneral(state: TeamSettingsState, viewModel: TeamSettingsViewModel) {
    val team = state.team ?: return
    val nameSaving by viewModel.teamNameSaving.collectAsStateWithLifecycle()
    val mergeModeBusy by viewModel.mergeModeBusy.collectAsStateWithLifecycle()
    val zoneBusy by viewModel.zoneBusy.collectAsStateWithLifecycle()
    val imageBusy by viewModel.imageBusy.collectAsStateWithLifecycle()
    // #EXP-1645: the composer's two system pickers (the files one is the way
    // to an SVG, which the photo picker never offers).
    val photoPicker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri: Uri? ->
        uri?.let(viewModel::uploadTeamImage)
    }
    val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        uri?.let(viewModel::uploadTeamImage)
    }
    val prompt by viewModel.prompt.collectAsStateWithLifecycle()
    val focus = LocalFocusManager.current
    var name by remember(team.id, team.name) { mutableStateOf(team.name) }
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

    Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Column {
            SectionHeader("General")
            Column(Modifier.fillMaxWidth().glassGroup()) {
                TextFieldRow(
                    label = "Name",
                    value = name,
                    onValueChange = { name = it.take(255) },
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                    onFocusLost = { viewModel.renameTeam(name) },
                    trailing = if (nameSaving) ({ InlineStatus(InlineStatusState.Saving) }) else null,
                    modifier = Modifier.testTag("team-name"),
                )
                // #EXP-1645 (team-image.json): directly under Name; owners only.
                GroupDivider()
                TeamImageRow(
                    iconUrl = team.iconUrl,
                    busy = imageBusy,
                    enabled = state.isOwner,
                    onPickPhoto = {
                        photoPicker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                    },
                    onPickFile = { filePicker.launch(TeamImage.Upload.ACCEPT.toTypedArray()) },
                    onRemove = { viewModel.removeTeamImage() },
                )
                // VAPP-201 (team-zone.json): THE schedule zone picker,
                // `Device time zone` (= null) first; owners only.
                GroupDivider()
                ScheduleZonePicker(
                    title = TeamZone.Copy.ROW,
                    value = team.timeZone,
                    nullLabel = TeamZone.Copy.DEVICE,
                    onChange = { viewModel.setTimeZone(it) },
                    enabled = state.isOwner && !zoneBusy,
                    modifier = Modifier.testTag(TeamZone.TEST_ID_ROW),
                )
            }
        }
        // #EXP-1710 (merge-train.json MODE): the merge train's ONE select
        // row (replaces Yolo mode), owners only: Automatic | After approval.
        Column(Modifier.fillMaxWidth().glassGroup()) {
            val modes = remember {
                listOf(
                    PickerItem(MergeTrain.MODE_AUTO, MergeTrain.Copy.Settings.AUTO),
                    PickerItem(MergeTrain.MODE_APPROVAL, MergeTrain.Copy.Settings.APPROVAL),
                )
            }
            val enabled = state.isOwner && !mergeModeBusy
            Picker(
                items = modes,
                mode = PickerMode.Single,
                value = setOf(team.mergeMode),
                onChange = { picked -> picked.firstOrNull()?.takeIf { it != team.mergeMode }?.let(viewModel::setMergeMode) },
                title = MergeTrain.Copy.Settings.LABEL,
                enabled = enabled,
            ) { open ->
                PickerValueRow(
                    label = MergeTrain.Copy.Settings.LABEL,
                    value = modes.firstOrNull { it.value == team.mergeMode }?.label ?: MergeTrain.Copy.Settings.AUTO,
                    onClick = open,
                    enabled = enabled,
                    modifier = Modifier.testTag(MergeTrain.TestIds.SETTINGS_ROW),
                )
            }
        }
        Column {
            val status = when {
                prompt.saving -> InlineStatusState.Saving
                prompt.justSaved -> InlineStatusState.Saved
                else -> InlineStatusState.Idle
            }
            SectionHeader(TEAM_PROMPT_TITLE) { InlineStatus(status) }
            val text = prompt.text
            val bytes = (text ?: "").toByteArray(Charsets.UTF_8).size
            val over = bytes > TEAM_PROMPT_MAX_BYTES
            Column(Modifier.fillMaxWidth().glassGroup()) {
                when {
                    text == null && !prompt.loadFailed -> SkeletonRows(rows = 2, dot = false)
                    text == null -> Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
                        modifier = Modifier.fillMaxWidth().heightIn(min = 192.dp),
                    ) {
                        Text("Prompt unavailable", style = MaterialTheme.typography.bodyMedium, color = muted)
                        GlassPill(com.exponential.app.domain.FeedbackStates.RETRY, size = PillSize.Sm, onClick = viewModel::retryPromptLoad)
                    }
                    else -> GlassTextField(
                        value = text,
                        onValueChange = viewModel::editPrompt,
                        placeholder = TEAM_PROMPT_PLACEHOLDER,
                        minLines = 10,
                        bordered = false,
                        containerColor = Color.Transparent,
                        textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("team-agent-prompt")
                            .onFocusChanged { if (!it.isFocused) viewModel.flushPrompt() },
                    )
                }
                GroupDivider()
                Text(
                    "${formatByteCount(bytes)} / ${formatByteCount(TEAM_PROMPT_MAX_BYTES)} bytes · ${approxTokens(bytes)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = if (over) MaterialTheme.colorScheme.error else muted,
                    textAlign = TextAlign.End,
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp)
                        .testTag("team-agent-prompt-counter"),
                )
            }
            if (over) {
                FieldError("Too long: $bytes of $TEAM_PROMPT_MAX_BYTES bytes", modifier = Modifier.padding(top = 8.dp))
            }
        }
    }
}

/** EXP-1025: the editor's copy, byte-identical with web's `general-section.tsx`. */
private const val TEAM_PROMPT_TITLE = "Team prompt"
private const val TEAM_PROMPT_PLACEHOLDER = "Rules every coding session of this team follows, as markdown."

/** `12.3k` / `840` — the counter's short form (web `formatByteCount`). */
internal fun formatByteCount(bytes: Int): String =
    if (bytes < 1000) "$bytes" else String.format(java.util.Locale.ROOT, "%.1fk", bytes / 1024.0)

/** The rough chars÷4 token readout (web `approxTokens`). */
internal fun approxTokens(bytes: Int): String {
    val tokens = Math.round(bytes / 4.0).toInt()
    return if (tokens < 1000) "≈$tokens tokens" else String.format(java.util.Locale.ROOT, "≈%.1fk tokens", tokens / 1000.0)
}

/**
 * #NEW-37 (settings-nav.json `boards`): the archived boards and the 48h trash
 * under the live list, each band drawn only while non-empty (web
 * `PutAwayBoards`, desktop `all_boards.rs`, iOS `PutAwayBoardsBands`).
 */
@Composable
private fun PutAwayBoardsBands(viewModel: TeamSettingsViewModel) {
    val putAway by viewModel.putAway.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { viewModel.refreshPutAway() }
    // The purge countdown re-renders every minute.
    val now by produceState(java.time.Instant.now()) {
        while (true) {
            kotlinx.coroutines.delay(60_000)
            value = java.time.Instant.now()
        }
    }
    if (putAway.archived.isNotEmpty()) {
        PutAwayBand("Archived", putAway.archived, putAway.pendingId, "Unarchive", { PutAwayBoardCaption.archived(it.archivedAt) }, viewModel::unarchiveBoard)
    }
    if (putAway.trashed.isNotEmpty()) {
        PutAwayBand("Trash", putAway.trashed, putAway.pendingId, "Restore", { PutAwayBoardCaption.purge(it.purgeAt, now) }, viewModel::restoreBoard)
    }
}

@Composable
private fun PutAwayBand(
    title: String,
    rows: List<PutAwayBoard>,
    pendingId: String?,
    action: String,
    caption: (PutAwayBoard) -> String,
    onAction: (PutAwayBoard) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeader(title)
        rows.forEach { board ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.fillMaxWidth().flatRow().padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Icon(
                    checkNotNull(ExpIcons.byName(boardIconName(board.icon, board.repositoryId))),
                    contentDescription = null,
                    tint = parseColor(board.color ?: "#888888"),
                    modifier = Modifier.size(16.dp),
                )
                Text(
                    board.name,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    caption(board),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                )
                GlassPill(
                    action,
                    size = PillSize.Sm,
                    enabled = pendingId != board.id,
                    onClick = { onAction(board) },
                )
            }
        }
    }
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
                    size = PillSize.Sm, // P105: band pills are Sm (iOS)
                    onClick = { showCreateBoard = true },
                )
            }
        }
        if (state.boards.isEmpty()) {
            EmptyState(EmptyStateCopy.teamBoards)
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
                    // iOS / web-mobile: the row's name in the list body type.
                    Text(board.name, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
                    // P104: a ghost glyph — only primary actions get circles.
                    borderless = true,
                    size = 32.dp,
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
                        size = 32.dp,
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
        // EXP-1554: web's Danger zone row, THE destructive button trailing.
        DangerZoneSection(
            description = "Permanently delete this team and all its data.",
            label = "Delete team",
            onClick = { confirmDelete = true },
        )
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
// ×4 in [GithubCopy]): header pill → intro → connection block
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
    // deep link never arrives (swallowed handoff, user closed the
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
        SectionHeader(GithubCopy.SECTION_TITLE, loading = state.reposRefreshing) {
            GlassPill(
                GithubCopy.ADD_REPOSITORY,
                icon = ExpIcons.uiGithub,
                size = PillSize.Sm, // P105: band pills are Sm (iOS)
                onClick = { showAddRepo = true },
            )
        }
        val context = LocalContext.current
        GithubStatusBlock(
            status = state.githubStatus,
            failed = state.githubStatusFailed,
            onRetry = { viewModel.refreshGithub(retried = true) },
            connecting = state.githubConnecting,
            onConnect = {
                viewModel.connectGithub { url ->
                    CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
                }
            },
            onDisconnect = { onConfirm(SettingsConfirm.DisconnectGithub) },
        )

        // EXP-721: one self-bordered glass row per repository. EXP-1275: the
        // first load = skeleton rows in the repository row's geometry.
        if (!state.reposLoaded) {
            SkeletonRows(rows = 2, rowHeight = 44.dp, dot = false)
        } else if (state.repos.isEmpty()) {
            // EXP-1274: the shared list empty state ×4.
            EmptyState(
                title = GithubCopy.NO_REPOSITORIES,
                icon = ExpIcons.settingsRepositories,
                size = at.exponential.ui.primitives.EmptyStateSize.List,
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
    var branchSheet by remember { mutableStateOf(false) }
    val inUse = repo.boards.isNotEmpty()
    // Web `RepoRow`: GitHub glyph · the name · the branch chip (a picker for
    // the sharer-or-owner) · Private · the ghost trash (disabled while a
    // board uses the repo); then the indented Used by line.
    Column(
        modifier = Modifier.fillMaxWidth().flatRow().padding(start = 12.dp, end = 4.dp, top = 6.dp, bottom = 8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Icon(ExpIcons.uiGithub, contentDescription = null, modifier = Modifier.size(16.dp), tint = secondary)
            Text(
                repo.fullName,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = androidx.compose.ui.text.font.FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            GlassPill(
                repo.defaultBranch,
                size = PillSize.Sm,
                fontFamily = FontFamily.Monospace,
                maxLines = 1,
                modifier = Modifier.widthIn(max = 140.dp),
                trailing = if (canManage) ({
                    Icon(
                        ExpIcons.uiChevronDown,
                        contentDescription = null,
                        tint = tertiary,
                        modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
                    )
                }) else null,
                onClick = if (canManage && state.accountId != null) ({ branchSheet = true }) else null,
            )
            if (repo.isPrivate) {
                GlassPill("Private", size = PillSize.Sm, icon = ExpIcons.uiPrivate)
            }
            if (canManage) {
                CircleIconButton(
                    ExpIcons.uiDelete,
                    contentDescription = "Remove repository",
                    onClick = { onConfirm(SettingsConfirm.RemoveRepo(repo)) },
                    enabled = !inUse,
                    glyphSize = 16.dp,
                    borderless = true,
                    size = 32.dp,
                )
            }
        }
        // "Used by" chips: the boards backed by this repo (masterplan §6),
        // each with its board glyph; indented under the name (web `pl-6`).
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
            itemVerticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(start = 24.dp),
        ) {
            Text(
                if (inUse) "Used by" else "Not used by any board",
                style = MaterialTheme.typography.labelMedium,
                color = tertiary,
            )
            repo.boards.forEach { ref ->
                val board = boards.firstOrNull { it.id == ref.id }
                // Any member can retarget a board to a different connected repo
                // (boards.setRepository is member-level since EXP-557) — tap
                // the chip to pick another repo.
                val chipClickable = board != null && allRepos.size > 1
                GlassPill(
                    ref.name,
                    size = PillSize.Sm,
                    maxLines = 1,
                    modifier = Modifier.widthIn(max = 180.dp),
                    leading = board?.let { b -> { BoardIcon(b, size = 12.dp) } },
                    onClick = if (chipClickable) ({ retargetBoard = board }) else null,
                )
            }
            // Informational for everyone (EXP-557): who shared this repo.
            repo.sharedBy?.let { sharer ->
                Text(
                    "· Shared by ${sharer.name?.takeIf { it.isNotBlank() } ?: sharer.email ?: "a teammate"}",
                    style = MaterialTheme.typography.labelMedium,
                    color = tertiary,
                )
            }
        }
    }
    val accountId = state.accountId
    if (branchSheet && accountId != null) {
        BranchPickerSheet(
            accountId = accountId,
            repositoryId = repo.id,
            repoDefault = repo.githubDefaultBranch ?: repo.defaultBranch,
            branch = repo.defaultBranch,
            onPick = { picked -> viewModel.setRepoDefaultBranch(repo.id, picked) },
            onDismiss = { branchSheet = false },
        )
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
    viewModel: TeamSettingsViewModel,
    isOwner: Boolean,
    onConfirm: (SettingsConfirm) -> Unit,
    onRevokeInvite: (inviteId: String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // Web `members-section.tsx`: the Members band over the roster.
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
                UserAvatar(user = row.user, nameOrEmail = displayName, size = 32.dp, userId = row.member.userId)
                Column(modifier = Modifier.weight(1f)) {
                    // P82/P83: name · muted "(you)" · the role pill WITH its
                    // role glyph right after the name (web members-section).
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(
                            buildAnnotatedString {
                                append(displayName)
                                if (isYou) {
                                    withStyle(
                                        SpanStyle(
                                            color = MaterialTheme.colorScheme.onSurface.copy(
                                                alpha = TextEmphasis.Tertiary,
                                            ),
                                        ),
                                    ) { append(" (you)") }
                                }
                            },
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        GlassPill(
                            row.member.role,
                            size = PillSize.Sm,
                            mode = PillMode.Readonly,
                            icon = if (row.member.role == DomainContract.teamRoleOwner) {
                                ExpIcons.uiOwner
                            } else {
                                ExpIcons.uiMember
                            },
                        )
                        // EXP-630: an email invite put this member on the
                        // roster before the person signed in — a MUTED mail
                        // pill after the role says so ("Invited" / "Invite
                        // expired"), exactly as web; EXP-1076: a row the
                        // Linear import seated without ever mailing reads
                        // "Not invited".
                        row.placeholder?.let { placeholder ->
                            GlassPill(
                                placeholder.label,
                                size = PillSize.Sm,
                                mode = PillMode.Readonly,
                                icon = ExpIcons.uiMail,
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                    }
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
        // Web `InviteControls`: a hairline, the Invite members band over THE
        // invite-by-email form (Send invite · Add without inviting), then the
        // outline Generate invite link.
        val teamId = state.team?.id
        if (isOwner && teamId != null) {
            GroupDivider()
            Spacer(Modifier.size(8.dp))
            InviteMemberForm(viewModel)
            Spacer(Modifier.size(8.dp))
            InviteLinkCard(teamId = teamId, bare = true)
        }
        // EXP-1267 ×4 (web `InviteControls`): the open links, owner-only like
        // the invite controls they belong to. Row anatomy = mail/link glyph ·
        // email or "Link invite" · role pill · "Expires Mon D" · revoke.
        if (isOwner && state.pendingInvites.isNotEmpty()) {
            Spacer(Modifier.size(8.dp))
            SectionHeader("Pending invites")
            state.pendingInvites.forEach { invite ->
                val email = invite.email?.takeIf { it.isNotBlank() }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .flatRow()
                        .padding(horizontal = 12.dp, vertical = 10.dp)
                        .testTag("pending-invite-${invite.id}"),
                ) {
                    Icon(
                        if (email != null) ExpIcons.uiMail else ExpIcons.uiLink,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.size(16.dp),
                    )
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.weight(1f),
                    ) {
                        // EXP-698: a link invite carries no address; it says
                        // what it is instead of collapsing the slot.
                        Text(
                            email ?: "Link invite",
                            style = MaterialTheme.typography.bodyMedium,
                            color = if (email != null) {
                                MaterialTheme.colorScheme.onSurface
                            } else {
                                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                            },
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        GlassPill(
                            invite.role,
                            size = PillSize.Sm,
                            mode = PillMode.Readonly,
                            icon = if (invite.role == DomainContract.teamRoleOwner) {
                                ExpIcons.uiOwner
                            } else {
                                ExpIcons.uiMember
                            },
                        )
                        Text(
                            inviteExpiryLabel(invite.expiresAt),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                        )
                    }
                    CircleIconButton(
                        ExpIcons.uiDelete,
                        contentDescription = "Revoke invite",
                        onClick = { onRevokeInvite(invite.id) },
                        glyphSize = 16.dp,
                        borderless = true,
                    )
                }
            }
        }
    }
}

/**
 * EXP-630 / FEED-108 (web `InviteMemberForm`, Members settings): Name, the
 * address, the primary Send invite (the placeholder member + the mail) and
 * the outline Add without inviting (the placeholder, no mail, no seat). A
 * refused address stays under its field; a mail that could not go out shows
 * the link to share. At the seat cap the form is gone (EXP-725).
 */
@Composable
private fun InviteMemberForm(viewModel: TeamSettingsViewModel) {
    val form by viewModel.inviteForm.collectAsStateWithLifecycle()
    if (form.capped) return
    val toaster = LocalToaster.current
    var name by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    LaunchedEffect(form.sent) {
        if (form.sent > 0) {
            name = ""
            email = ""
        }
    }
    val busy = form.sending || form.adding
    val canSubmit = email.isNotBlank()
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        SectionHeader("Invite members")
        GlassTextField(
            value = name,
            onValueChange = { name = it },
            placeholder = "Name",
            singleLine = true,
            modifier = Modifier.fillMaxWidth().testTag("invite-name"),
        )
        GlassTextField(
            value = email,
            onValueChange = {
                email = it
                viewModel.clearInviteError()
            },
            placeholder = "teammate@example.com",
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = {
                if (canSubmit && !busy) viewModel.sendInvite(email, name) { toaster.success(it) }
            }),
            modifier = Modifier.fillMaxWidth().testTag("invite-email"),
        )
        FieldError(form.emailError)
        GlassSubmitButton(
            label = if (form.sending) com.exponential.app.domain.FeedbackStates.PENDING_WORDS.getValue("invite") else "Send invite",
            onClick = { viewModel.sendInvite(email, name) { toaster.success(it) } },
            enabled = canSubmit && !busy,
            icon = { Icon(ExpIcons.uiMail, contentDescription = null, modifier = Modifier.size(16.dp)) },
            modifier = Modifier.testTag("invite-send"),
        )
        GlassOAuthButton(
            label = "Add without inviting",
            onClick = { if (canSubmit && !busy) viewModel.addWithoutInviting(email, name) { toaster.success(it) } },
            icon = { Icon(ExpIcons.uiInvite, contentDescription = null, modifier = Modifier.size(16.dp)) },
            // Dimmed until it can act, like its twins ×4.
            modifier = Modifier
                .alpha(if (canSubmit && !busy) 1f else 0.5f)
                .testTag("invite-add-without-inviting"),
        )
        form.inviteUrl?.let { url ->
            Text(
                url,
                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.testTag("invite-url-input"),
            )
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
                size = PillSize.Sm, // P105: band pills are Sm (iOS)
                onClick = { showCreate = true },
            )
        }
        if (state.labels.isEmpty()) {
            EmptyState(EmptyStateCopy.labels)
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
    // Web `labels-section.tsx`: the 16dp colour dot, the name, ONE ghost
    // trash; the row itself (dot + name) opens the editor sheet.
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier
            .fillMaxWidth()
            .flatRow()
            .clickable { editing = true }
            .padding(start = 12.dp, end = 4.dp, top = 4.dp, bottom = 4.dp),
    ) {
        Box(Modifier.size(16.dp).background(parseColor(label.color), CircleShape))
        Text(
            label.name,
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        CircleIconButton(
            ExpIcons.uiDelete,
            contentDescription = "Delete label ${label.name}",
            onClick = { onDelete(label) },
            glyphSize = 16.dp,
            borderless = true,
            size = 32.dp,
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
 * Loading or failed → nothing (a failure toasts); not configured → a line; not
 * linked → Connect GitHub; expired → Reconnect + ✕; connected → "Connected as
 * login" + ✕, the suspended line (Manage), then the accounts where the app is
 * installed (one indented row each with Configure), the caption and Install
 * on another account — or the install nudge when there are none.
 */
@Composable
private fun GithubStatusBlock(
    status: GithubStatusResult?,
    failed: Boolean,
    onRetry: () -> Unit,
    connecting: Boolean,
    onConnect: () -> Unit,
    onDisconnect: () -> Unit,
) {
    val context = LocalContext.current
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val open: (String) -> Unit = { url ->
        CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url))
    }
    // A failed probe with nothing loaded (FEED-79: quiet, no toast) = the
    // failed line with Retry, iOS parity; loading = nothing.
    if (status == null) {
        if (!failed) return
        // Web `GithubStatusLine`: a plain line under the band, never boxed.
        Box(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
            StatusLine(
                ExpIcons.uiGithub,
                GithubCopy.STATUS_FAILED,
                secondary,
                trailing = {
                    GlassPill(GithubCopy.RETRY, icon = ExpIcons.uiRefresh, size = PillSize.Sm, onClick = onRetry)
                },
            )
        }
        return
    }
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
    ) {
        when {
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
