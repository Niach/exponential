package com.exponential.app.ui.issue

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.share.SharePrefill
import com.exponential.app.ui.share.ShareBoardPickerSheet
import com.exponential.app.ui.share.ShareBoardSelector
import com.exponential.app.ui.share.TeamBoards
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassSection
import java.util.UUID
import kotlinx.coroutines.launch

// Full-screen issue creation (iOS CreateIssueSheet parity): a "New Issue" nav
// title with Cancel/Create actions over the shared AppBackground, then the
// title field, description editor, and stacked glassSection metadata rows.
// Reuses the same pickers, payload and createIssue path the bottom sheet used —
// only the container and layout changed (a route screen, not a ModalBottomSheet).
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun CreateIssueScreen(
    onBack: () -> Unit,
    sharePrefill: SharePrefill? = null,
    onSharePrefillConsumed: () -> Unit = {},
    // Share mode (system "Share into Exponential"): the screen has no board
    // route arg, so it renders a "Share to" destination selector at the TOP of
    // the form (EXP-60) and re-points the ViewModel to the picked board.
    // [shareGroups] are the account's teams→boards,
    // [shareRecentBoardId] the last-used default.
    shareMode: Boolean = false,
    shareGroups: List<TeamBoards> = emptyList(),
    shareRecentBoardId: String? = null,
    // True while the share picker VM is still loading [shareGroups] — gates
    // the "no boards" empty state so it can't flash before the list arrives.
    shareGroupsLoading: Boolean = false,
    viewModel: IssueListViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val isModerator = permissions.isModerator
    // EXP-50: solo teams (one human member) hide the assignee picker and
    // default the new issue to that member.
    val soloMemberId by viewModel.soloMemberId.collectAsStateWithLifecycle()
    val isSoloTeam = soloMemberId != null

    // In share mode the ViewModel starts with no board; track the chosen one
    // locally and re-point the VM to it (setBoard re-scopes labels/members/
    // permissions and the create target).
    var selectedBoardId by remember { mutableStateOf<String?>(null) }
    // Seed the default once the board list arrives: last-used if it still
    // exists, else the first board.
    LaunchedEffect(shareGroups, shareRecentBoardId) {
        if (!shareMode || selectedBoardId != null) return@LaunchedEffect
        val allIds = shareGroups.flatMap { g -> g.boards.map { it.id } }
        val default = shareRecentBoardId?.takeIf { it in allIds } ?: allIds.firstOrNull()
        if (default != null) {
            selectedBoardId = default
            viewModel.setBoard(default)
        }
    }

    var title by remember { mutableStateOf(sharePrefill?.title ?: "") }
    var description by remember { mutableStateOf(sharePrefill?.description ?: "") }
    // Default = the team's Backlog builtin (EXP-314); re-seeded whenever the
    // team's status rows arrive or a share-mode board switch re-scopes them.
    var status by remember {
        mutableStateOf(
            IssueStatusResolver.builtinDefaults.first { it.builtinKey == IssueStatus.Backlog }
        )
    }
    var priority by remember { mutableStateOf(IssuePriority.None) }
    var assigneeId by remember { mutableStateOf<String?>(null) }
    var dueDate by remember { mutableStateOf<String?>(null) }
    var selectedLabelIds by remember { mutableStateOf<Set<String>>(emptySet()) }
    var createMore by remember { mutableStateOf(false) }
    var statusMenuOpen by remember { mutableStateOf(false) }
    var priorityMenuOpen by remember { mutableStateOf(false) }
    var assigneeMenuOpen by remember { mutableStateOf(false) }
    var datePickerOpen by remember { mutableStateOf(false) }
    var labelSheetOpen by remember { mutableStateOf(false) }
    var boardSheetOpen by remember { mutableStateOf(false) }

    val initialPendingImages = remember { sharePrefill?.pendingImages ?: emptyMap() }
    val pendingImages = remember { mutableStateMapOf<String, Uri>().apply { putAll(initialPendingImages) } }
    // Draft file attachments (EXP-327): the issue doesn't exist yet, so —
    // exactly like draft images — the picks are held here and uploaded right
    // after the create.
    val pendingFiles = remember { mutableStateListOf<Uri>() }
    val users = state.users
    // In a solo team the picker is hidden, so seed (and keep) the assignee
    // pinned to the lone member — including after a share-mode board switch
    // re-scopes to another solo team.
    LaunchedEffect(soloMemberId) {
        if (soloMemberId != null) assigneeId = soloMemberId
    }
    // Re-point the picked status at the TEAM's rows once they arrive (and
    // again after a share-mode board switch): keep the user's own pick when it
    // still exists, else fall back to that team's backlog status.
    val teamStatuses = state.teamStatuses
    LaunchedEffect(teamStatuses) {
        if (teamStatuses.isEmpty()) return@LaunchedEffect
        val picked = status
        status = teamStatuses.firstOrNull { it.id == picked.id }
            // The pick was made against the CONSTRUCTED fallback set, whose ids
            // are `builtin:<key>` — once the real rows land, re-key it through
            // the builtin key instead of silently resetting to Backlog.
            ?: picked.builtinKey?.let { key -> teamStatuses.firstOrNull { it.builtinKey == key } }
            ?: teamStatuses.firstOrNull { it.builtinKey == IssueStatus.Backlog }
            ?: teamStatuses.firstOrNull { it.category == IssueStatusCategory.Backlog }
            ?: teamStatuses.first()
    }

    val assigneeUser = users.firstOrNull { it.id == assigneeId }
    val isCreating = state.isCreating
    var confirmDiscard by remember { mutableStateOf(false) }

    // Anything worth a "discard?" prompt: typed/prefilled content or images
    // queued for upload.
    val hasUnsavedContent = title.isNotBlank() || description.isNotBlank() ||
        pendingImages.isNotEmpty() || pendingFiles.isNotEmpty()

    // The share prefill is NOT consumed on entry: it lives in an app-singleton
    // (TeamSelection.pendingShare), so backing out and re-entering re-fills
    // the form. It's consumed exactly once — on a successful create (below) or
    // an explicit discard.
    fun close(discarding: Boolean) {
        if (discarding && sharePrefill != null) onSharePrefillConsumed()
        onBack()
    }

    fun attemptClose() {
        if (isCreating) return
        if (hasUnsavedContent) confirmDiscard = true else close(discarding = false)
    }

    // System back: blocked while a create is in flight (it would cancel the
    // route's ViewModel scope mid-request), and gated behind a discard
    // confirmation while the form holds unsaved content.
    BackHandler(enabled = isCreating || hasUnsavedContent) {
        if (!isCreating) confirmDiscard = true
    }

    // In share mode a board must be chosen before the create can target it.
    val canSubmit = title.isNotBlank() && !isCreating && (!shareMode || selectedBoardId != null)

    val scope = rememberCoroutineScope()
    fun submit() {
        if (!canSubmit) return
        // Await the create on the screen's scope, then pop — popping cancels the
        // route's ViewModel scope, so a fire-and-forget create would be dropped.
        scope.launch {
            val ok = viewModel.createIssueAwait(
                title = title,
                status = status,
                priority = priority,
                description = description,
                dueDate = dueDate,
                assigneeId = assigneeId,
                // Drop selections for labels deleted while drafting — the
                // server rejects the whole create on an unknown label id.
                labelIds = selectedLabelIds.filter { id -> state.labels.any { it.id == id } },
                pendingImages = pendingImages.toMap(),
                pendingFiles = pendingFiles.toList(),
            )
            if (ok) {
                // The share prefill (if any) made it into this issue — consume
                // it now so it can't prefill another create.
                if (sharePrefill != null) onSharePrefillConsumed()
                if (createMore) {
                    title = ""
                    description = ""
                    selectedLabelIds = emptySet()
                    pendingImages.clear()
                    pendingFiles.clear()
                } else {
                    onBack()
                }
            }
        }
    }

    // #issue-ref autocomplete in the description editor (masterplan §5e):
    // same-team candidates, newest first, from the target board's team. There is
    // nowhere to navigate from a half-written issue, so the handler is marked
    // non-navigable and a tap on a chip falls through to the caret (EXP-423).
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    val issueRefHandler = remember(issueRefCandidates) {
        IssueRefHandler(issueRefCandidates, canOpen = false) { }
    }

    CompositionLocalProvider(LocalIssueRefs provides issueRefHandler) {
    ProvideMarkdownToolbar {
        Scaffold(
            topBar = {
                CenterAlignedTopAppBar(
                    title = { Text("New Issue") },
                    navigationIcon = {
                        IconButton(onClick = ::attemptClose, enabled = !isCreating) {
                            Icon(ExpIcons.uiBack, contentDescription = "Cancel")
                        }
                    },
                    actions = {
                        TextButton(onClick = ::submit, enabled = canSubmit) {
                            Text(if (isCreating) "Creating…" else "Create")
                        }
                    },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
                )
            },
            containerColor = Color.Transparent,
        ) { padding ->
            // Tap-outside keyboard dismissal (EXP-246): taps on dead space in
            // the form clear focus and drop the IME; interactive children
            // consume their own taps first.
            val focusManager = LocalFocusManager.current
            val keyboard = LocalSoftwareKeyboardController.current
            Column(
                modifier = Modifier
                    .padding(padding)
                    // Shrink the scrollport above the keyboard (EXP-135) —
                    // with edge-to-edge, adjustResize alone never resizes the
                    // window, so the description editor would stay hidden
                    // behind the IME while typing.
                    .consumeWindowInsets(padding)
                    .imePadding()
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        detectTapGestures(onTap = {
                            focusManager.clearFocus()
                            keyboard?.hide()
                        })
                    }
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // Destination first (EXP-60): in share mode the target board
                // leads the form — a compact "Share to" card that opens the
                // grouped picker sheet. Picking a board re-scopes the
                // ViewModel (labels/permissions) and the create target.
                if (shareMode) {
                    ShareBoardSelector(
                        groups = shareGroups,
                        selectedBoardId = selectedBoardId,
                        loading = shareGroupsLoading,
                        onClick = { boardSheetOpen = true },
                    )
                }

                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    placeholder = { Text("Issue title") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("create-issue-title-field"),
                )

                MarkdownEditor(
                    markdown = description,
                    editable = true,
                    onChange = { description = it },
                    onUploadImage = { uri ->
                        val placeholder = "draft://${UUID.randomUUID()}"
                        pendingImages[placeholder] = uri
                        placeholder
                    },
                    imageUploadEnabled = true,
                    minHeight = 120.dp,
                    initialPendingImages = initialPendingImages,
                    mentionMembers = remember(users) {
                        users
                            .map { MentionMember(it.name ?: it.email, it.email) }
                    },
                    mentionEnabled = !isSoloTeam,
                    // EXP-327: the same attach menu as issue detail — images go
                    // into the description, other files become draft
                    // attachments uploaded once the issue exists.
                    onAttachFile = { pendingFiles.add(it) },
                )

                // Draft files, only once there is one (the section never
                // announces its own emptiness — EXP-327).
                if (pendingFiles.isNotEmpty()) {
                    DraftFilesSection(
                        files = pendingFiles,
                        onRemove = { pendingFiles.remove(it) },
                    )
                }

                // Status / Priority / Assignee — one grouped glass card.
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassSection()
                        .padding(vertical = 4.dp)
                        .alpha(if (isModerator) 1f else 0.55f),
                ) {
                    MetaRow(label = "Status", enabled = isModerator, onClick = { statusMenuOpen = true }) {
                        StatusIcon(status, size = 14.dp)
                        Spacer(Modifier.width(6.dp))
                        Text(status.name, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                    }
                    MetaDivider()
                    MetaRow(label = "Priority", enabled = isModerator, onClick = { priorityMenuOpen = true }) {
                        PriorityIcon(priority, size = 14.dp)
                        Spacer(Modifier.width(6.dp))
                        Text(priority.label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                    }
                    // EXP-50: hidden in a solo team (no one else to assign to).
                    if (!isSoloTeam) {
                        MetaDivider()
                        MetaRow(label = "Assignee", enabled = isModerator, onClick = { assigneeMenuOpen = true }) {
                            Icon(ExpIcons.uiAssignee, null, modifier = Modifier.size(14.dp), tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary))
                            Spacer(Modifier.width(6.dp))
                            Text(
                                assigneeUser?.name ?: assigneeUser?.email ?: "Unassigned",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    // Due date — same grouped card (EXP-247).
                    MetaDivider()
                    MetaRow(label = "Due date", enabled = isModerator, onClick = { datePickerOpen = true }) {
                        Icon(ExpIcons.uiDueDate, null, modifier = Modifier.size(14.dp), tint = dueDate?.let { dueDateColor(it) } ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary))
                        Spacer(Modifier.width(6.dp))
                        Text(
                            dueDate?.let { formatDueDate(it) } ?: "—",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = if (dueDate != null) TextEmphasis.Primary else TextEmphasis.Tertiary),
                        )
                    }
                }

                // Labels (masterplan §3 client parity: every client supports
                // labels at create). All team labels as colored-dot toggle
                // chips + a "+ Label" chip opening the shared picker sheet —
                // the same chip pattern as the issue-detail property box,
                // toggling a local selection instead of issueLabels mutations.
                // Not moderator-gated: issues.create lets any creator set
                // title/description/labels (web create dialog parity).
                Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp)) {
                    Text(
                        "Labels",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Spacer(Modifier.height(8.dp))
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        state.labels.forEach { label ->
                            val selected = label.id in selectedLabelIds
                            Row(
                                modifier = Modifier
                                    .glassButton(active = selected)
                                    .clickable {
                                        selectedLabelIds =
                                            if (selected) selectedLabelIds - label.id
                                            else selectedLabelIds + label.id
                                    }
                                    .padding(horizontal = 10.dp, vertical = 6.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Box(modifier = Modifier.size(8.dp).background(parseColor(label.color), CircleShape))
                                Spacer(Modifier.width(5.dp))
                                Text(label.name, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurface)
                            }
                        }
                        Row(
                            modifier = Modifier
                                .glassButton()
                                .clickable { labelSheetOpen = true }
                                .padding(horizontal = 10.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(
                                ExpIcons.uiAdd,
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                            Spacer(Modifier.width(4.dp))
                            Text(
                                "Label",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    }
                }

                if (state.error != null) {
                    Text(state.error!!, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                }

                // "Create more" is a batch-entry affordance for in-app creation;
                // a system share is a one-shot, so it's hidden in share mode.
                if (!shareMode) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Create more", style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                        Switch(checked = createMore, onCheckedChange = { createMore = it })
                    }
                }
                Spacer(Modifier.height(8.dp))
            }
        }
    }
    }

    if (boardSheetOpen && shareMode) {
        ShareBoardPickerSheet(
            groups = shareGroups,
            selectedBoardId = selectedBoardId,
            onSelect = { id ->
                if (id != selectedBoardId) {
                    selectedBoardId = id
                    viewModel.setBoard(id)
                }
            },
            onDismiss = { boardSheetOpen = false },
        )
    }

    if (statusMenuOpen && isModerator) {
        IssuePickerSheet(
            title = "Status",
            // Duplicate = status interception (L27): a new issue can't be a
            // duplicate (nothing to link yet), so it's not a create option.
            items = teamStatuses.filter { it.category != IssueStatusCategory.Duplicate },
            selected = status,
            keyOf = { it.id },
            labelOf = { it.name },
            leadingContent = { StatusIcon(it, size = 18.dp) },
            onSelect = { status = it },
            onDismiss = { statusMenuOpen = false },
        )
    }

    if (priorityMenuOpen && isModerator) {
        IssuePickerSheet(
            title = "Priority",
            items = issuePriorityOrder,
            selected = priority,
            labelOf = { it.label },
            iconOf = { priorityIcon(it) },
            onSelect = { priority = it },
            onDismiss = { priorityMenuOpen = false },
        )
    }

    if (assigneeMenuOpen && isModerator) {
        val assigneeItems = listOf<com.exponential.app.data.db.UserEntity?>(null) + users
        IssuePickerSheet(
            title = "Assignee",
            items = assigneeItems,
            selected = assigneeItems.firstOrNull { it?.id == assigneeId },
            keyOf = { it?.id ?: "__unassigned__" },
            labelOf = { user -> user?.name ?: user?.email ?: "Unassigned" },
            onSelect = { assigneeId = it?.id },
            onDismiss = { assigneeMenuOpen = false },
        )
    }

    if (datePickerOpen) {
        IssueDatePickerDialog(
            initialDate = dueDate,
            onConfirm = { dueDate = it; datePickerOpen = false },
            onDismiss = { datePickerOpen = false },
        )
    }

    if (labelSheetOpen) {
        LabelPickerSheet(
            teamLabels = state.labels,
            selectedLabelIds = selectedLabelIds,
            onToggle = { id, selected ->
                selectedLabelIds = if (selected) selectedLabelIds - id else selectedLabelIds + id
            },
            // A label created here is real immediately (labels.create); only
            // its assignment waits for the issue to exist — pre-select it so
            // the create carries it via labelIds.
            onCreate = { name, color ->
                scope.launch {
                    viewModel.createLabel(name, color)?.let { created ->
                        selectedLabelIds = selectedLabelIds + created.id
                    }
                }
            },
            onDismiss = { labelSheetOpen = false },
        )
    }

    if (confirmDiscard) {
        AlertDialog(
            onDismissRequest = { confirmDiscard = false },
            title = { Text("Discard this issue?") },
            text = { Text("Your title, description and attached images will be lost.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDiscard = false
                    close(discarding = true)
                }) {
                    Text("Discard", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDiscard = false }) { Text("Keep editing") }
            },
        )
    }
}

// One row of a grouped glass card: fixed-width label + trailing value (iOS
// metadataRow). Tappable when [enabled].
@Composable
private fun MetaRow(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    value: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.width(84.dp),
        )
        Spacer(Modifier.weight(1f))
        value()
    }
}

@Composable
private fun MetaDivider() {
    HorizontalDivider(thickness = 0.5.dp, color = Color.White.copy(alpha = 0.06f))
}

/**
 * Draft file attachments on the create screen (EXP-327). The issue has no id
 * yet, so these are held locally and uploaded straight after the create — the
 * same deferred shape draft images already use. Rendered only when non-empty,
 * matching the issue-detail Files section.
 */
@Composable
private fun DraftFilesSection(files: List<Uri>, onRemove: (Uri) -> Unit) {
    val context = LocalContext.current
    Column(modifier = Modifier.fillMaxWidth()) {
        Text(
            "Files",
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        for (uri in files) {
            val name = remember(uri) { MarkdownMediaUtils.guessFilename(context, uri) }
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    ExpIcons.uiFile,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.width(10.dp))
                Text(
                    text = name,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = { onRemove(uri) }) {
                    Icon(
                        ExpIcons.uiClose,
                        contentDescription = "Remove $name",
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
    }
}
