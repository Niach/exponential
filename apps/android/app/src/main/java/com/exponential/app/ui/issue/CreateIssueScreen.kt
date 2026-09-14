package com.exponential.app.ui.issue

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
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
import androidx.compose.runtime.saveable.rememberSaveable
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
import com.exponential.app.data.api.IssueDraftAttachment
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.isInlineImage
import com.exponential.app.domain.isInlineMedia
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.domain.priorityIcon
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LabelsPickerBlock
import com.exponential.app.ui.components.MetaRow
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.formatDueDate
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.domain.PreparedMedia
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import com.exponential.app.ui.markdown.markdownEmbedUrls
import com.exponential.app.ui.markdown.removeMarkdownImagesByUrl
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.share.ShareBoardPickerSheet
import com.exponential.app.ui.share.ShareBoardSelector
import com.exponential.app.ui.share.SharePrefill
import com.exponential.app.ui.share.TeamBoards
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.dueDateColor
import com.exponential.app.ui.theme.glassGroup
import java.util.UUID
import kotlinx.coroutines.launch

// Full-screen issue creation (iOS CreateIssueSheet parity): a "New Issue" nav
// title with Cancel/Create actions over the shared AppBackground, then the
// title field, description editor, and one grouped card of metadata rows.
// Reuses the same pickers, payload and createIssue path the bottom sheet used —
// only the container and layout changed (a route screen, not a ModalBottomSheet).
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CreateIssueScreen(
    onBack: () -> Unit,
    // The issue was filed: land on it (EXP-596).
    onCreated: (String) -> Unit,
    // EXP-878: the draft being resumed (`board/{boardId}/new?draft={id}`).
    // Null = a fresh create — the screen still mints an id, but writes nothing
    // until it closes with content in it.
    draftId: String? = null,
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
    var statusMenuOpen by remember { mutableStateOf(false) }
    var priorityMenuOpen by remember { mutableStateOf(false) }
    var assigneeMenuOpen by remember { mutableStateOf(false) }
    var datePickerOpen by remember { mutableStateOf(false) }
    var labelSheetOpen by remember { mutableStateOf(false) }
    var boardSheetOpen by remember { mutableStateOf(false) }

    val initialPendingImages = remember { sharePrefill?.pendingImages ?: emptyMap() }
    val pendingImages = remember { mutableStateMapOf<String, Uri>().apply { putAll(initialPendingImages) } }
    // EXP-824: video / audio picks, already normalised (720p MP4 + poster),
    // keyed by the `draft://` placeholder their media-link block carries;
    // uploaded right after the create like the images above.
    val pendingMedia = remember { mutableStateMapOf<String, PreparedMedia>() }
    // Draft file attachments (EXP-327): the issue doesn't exist yet, so —
    // exactly like draft images — the picks are held here and uploaded right
    // after the create.
    val pendingFiles = remember { mutableStateListOf<Uri>() }
    // EXP-878: on the DRAFT path (everything but share mode) the screen owns a
    // client-minted draft id from the first frame, so an attachment can be
    // uploaded the moment it is picked — the issue-detail paste model — and
    // the description only ever carries final `/api/attachments/{id}` URLs.
    val draftKey = rememberSaveable { draftId ?: UUID.randomUUID().toString() }
    val draftRow by viewModel.draft.collectAsStateWithLifecycle()
    val draftMaterialized by viewModel.draftMaterialized.collectAsStateWithLifecycle()
    // The draft's already-uploaded files (the Files section when resuming).
    val draftFiles = remember { mutableStateListOf<IssueDraftAttachment>() }
    // One-shot seed latch: a resumed draft fills the form exactly once, so a
    // later sync of the same row can never overwrite what is being typed.
    var seeded by rememberSaveable { mutableStateOf(draftId == null) }
    // Held until the team's status ROWS arrive — the draft stores a status_id,
    // which can only be re-pointed once issue_statuses has synced.
    var pendingDraftStatusId by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(draftRow, seeded) {
        if (seeded) return@LaunchedEffect
        val row = draftRow ?: return@LaunchedEffect
        title = row.title
        description = row.description
        priority = IssuePriority.fromWire(row.priority)
        assigneeId = row.assigneeId
        dueDate = row.dueDate
        selectedLabelIds = row.labelIds.toSet()
        pendingDraftStatusId = row.statusId
        seeded = true
        // The draft's attachments are deliberately NOT in the attachments
        // shape (it is scoped to issue-owned rows), so they come over tRPC.
        // Only the FILE rows belong here: an inline image or clip is already
        // rendered from the description it is embedded in (EXP-297/824).
        val files = viewModel.draftAttachments(row.id).filterNot {
            isInlineImage(it.contentType) || isInlineMedia(it.contentType)
        }
        draftFiles.clear()
        draftFiles.addAll(files)
    }
    // EXP-487: assignee picker + @-mention candidates are the target team's
    // members only — state.users is the account-wide display lookup.
    val users = state.teamUsers
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
    LaunchedEffect(teamStatuses, pendingDraftStatusId) {
        if (teamStatuses.isEmpty()) return@LaunchedEffect
        // A resumed draft's own status row wins until it resolves (EXP-878);
        // it stays pending while the team's rows are still landing.
        val fromDraft = pendingDraftStatusId?.let { id -> teamStatuses.firstOrNull { it.rowId == id } }
        if (fromDraft != null) {
            status = fromDraft
            pendingDraftStatusId = null
            return@LaunchedEffect
        }
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

    // What makes this form worth keeping (EXP-878): a real title, a
    // description, or at least one file already uploaded against the draft.
    val hasDraftContent = title.isNotBlank() || description.isNotBlank() || draftFiles.isNotEmpty()

    // The form as a draft row. A `draft://` placeholder is never persisted —
    // on the draft path uploads are eager, so one can only come from a share
    // prefill's still-pending images.
    fun snapshot(): IssueDraftSnapshot {
        val pending = markdownEmbedUrls(description).filter { it.startsWith(DRAFT_URL_PREFIX) }
        return IssueDraftSnapshot(
            id = draftKey,
            title = title.trim(),
            description = removeMarkdownImagesByUrl(description, pending),
            statusId = status.rowId,
            priority = priority.wire,
            assigneeId = assigneeId,
            // Drop selections for labels deleted while drafting, like create.
            labelIds = selectedLabelIds.filter { id -> state.labels.any { it.id == id } },
            dueDate = dueDate,
        )
    }

    // The share prefill is NOT consumed on entry: it lives in an app-singleton
    // (TeamSelection.pendingShare), so backing out and re-entering re-fills
    // the form. It's consumed exactly once — on a successful create (below) or
    // once its content has been written somewhere else (a saved draft).
    fun close(consumePrefill: Boolean) {
        if (consumePrefill && sharePrefill != null) onSharePrefillConsumed()
        onBack()
    }

    fun attemptClose() {
        if (isCreating) return
        // EXACTLY ONE write per close, never while typing (EXP-878): content
        // is saved silently as a draft, an emptied existing draft is deleted,
        // and a blank untouched form writes nothing at all. Share mode keeps
        // its own pending-upload pipeline and writes no drafts.
        var wrote = false
        if (!shareMode) {
            if (hasDraftContent) {
                viewModel.persistDraft(snapshot())
                wrote = true
            } else if (draftMaterialized) {
                viewModel.discardDraft(draftKey)
                wrote = true
            }
        }
        close(consumePrefill = wrote)
    }

    // System back always routes through the save-or-discard path; it is a
    // no-op while a create is in flight (leaving would cancel the route's
    // ViewModel scope mid-request).
    BackHandler(enabled = true) { attemptClose() }

    // In share mode a board must be chosen before the create can target it.
    val canSubmit = title.isNotBlank() && !isCreating && (!shareMode || selectedBoardId != null)

    val scope = rememberCoroutineScope()
    fun submit() {
        if (!canSubmit) return
        // Await the create on the screen's scope, then leave — leaving cancels
        // the route's ViewModel scope, so a fire-and-forget create would be
        // dropped.
        scope.launch {
            val createdId = viewModel.createIssueAwait(
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
                pendingMedia = pendingMedia.toMap(),
                pendingFiles = pendingFiles.toList(),
                // EXP-878: the server reparents this draft's attachments onto
                // the new issue and deletes the draft in the same transaction.
                // Only sent once the row actually EXISTS (resumed, or
                // materialised by an eager upload) — `issues.create` answers
                // NOT_FOUND for a draft id nothing was ever written under.
                draftId = draftKey.takeIf { !shareMode && draftMaterialized },
            )
            if (createdId != null) {
                // The share prefill (if any) made it into this issue — consume
                // it now so it can't prefill another create.
                if (sharePrefill != null) onSharePrefillConsumed()
                onCreated(createdId)
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
                        TopBarBackButton(
                            onClick = ::attemptClose,
                            contentDescription = "Cancel",
                            enabled = !isCreating,
                        )
                    },
                    actions = {
                        // iOS 26 renders the confirmation item as a glass
                        // capsule — same pill as every inline action (EXP-577).
                        GlassPill(
                            if (isCreating) "Creating…" else "Create",
                            onClick = ::submit,
                            enabled = canSubmit,
                            modifier = Modifier.padding(end = 8.dp),
                        )
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

                GlassTextField(
                    value = title,
                    onValueChange = { title = it },
                    placeholder = "Issue title",
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
                    // EXP-878: on the draft path the pick is uploaded RIGHT
                    // AWAY against the draft (creating it if needed) and the
                    // editor gets the final `/api/attachments/{id}` URL — the
                    // issue-detail model. Share mode keeps the placeholder
                    // pipeline that uploads after the create.
                    onUploadImage = { uri ->
                        if (shareMode) {
                            val placeholder = "$DRAFT_URL_PREFIX${UUID.randomUUID()}"
                            pendingImages[placeholder] = uri
                            placeholder
                        } else {
                            viewModel.uploadDraftImage(snapshot(), uri)
                        }
                    },
                    onUploadMedia = { prepared ->
                        if (shareMode) {
                            val placeholder = "$DRAFT_URL_PREFIX${UUID.randomUUID()}"
                            pendingMedia[placeholder] = prepared
                            placeholder
                        } else {
                            viewModel.uploadDraftMedia(snapshot(), prepared)
                        }
                    },
                    imageUploadEnabled = true,
                    minHeight = 120.dp,
                    initialPendingImages = initialPendingImages,
                    mentionMembers = remember(users) {
                        users
                            .map { MentionMember(it.name ?: it.email, it.email) }
                    },
                    // EXP-327: the same attach menu as issue detail — images go
                    // into the description, other files become draft
                    // attachments uploaded once the issue exists.
                    onAttachFile = { uri ->
                        if (shareMode) {
                            pendingFiles.add(uri)
                        } else {
                            scope.launch {
                                viewModel.uploadDraftFile(snapshot(), uri)
                                    ?.let { draftFiles.add(it) }
                            }
                        }
                    },
                )

                // Files, only once there is one (the section never announces
                // its own emptiness — EXP-327). Share mode still holds URIs;
                // the draft path shows the rows it already uploaded.
                if (pendingFiles.isNotEmpty()) {
                    DraftFilesSection(
                        files = pendingFiles,
                        onRemove = { pendingFiles.remove(it) },
                    )
                }
                if (draftFiles.isNotEmpty()) {
                    DraftAttachmentsSection(
                        files = draftFiles,
                        onRemove = { attachment ->
                            draftFiles.remove(attachment)
                            viewModel.deleteDraftAttachment(attachment.id)
                        },
                    )
                }

                // Status / Priority / Assignee — one grouped glass card.
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassGroup()
                        .padding(vertical = 4.dp)
                        .alpha(if (isModerator) 1f else 0.55f),
                ) {
                    MetaRow(label = "Status", enabled = isModerator, onClick = { statusMenuOpen = true }) {
                        StatusIcon(status, size = 14.dp)
                        Spacer(Modifier.width(6.dp))
                        Text(status.name, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                    }
                    GroupDivider()
                    MetaRow(label = "Priority", enabled = isModerator, onClick = { priorityMenuOpen = true }) {
                        PriorityIcon(priority, size = 14.dp)
                        Spacer(Modifier.width(6.dp))
                        Text(priority.label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                    }
                    // EXP-50: hidden in a solo team (no one else to assign to).
                    if (!isSoloTeam) {
                        GroupDivider()
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
                    GroupDivider()
                    MetaRow(label = "Due date", enabled = isModerator, onClick = { datePickerOpen = true }) {
                        Icon(ExpIcons.uiDueDate, null, modifier = Modifier.size(14.dp), tint = dueDate?.let { dueDateColor(it) } ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary))
                        Spacer(Modifier.width(6.dp))
                        Text(
                            // EXP-698: an EMPTY value says so in words, like
                            // "Unassigned" / "No priority" two rows up — a bare
                            // em-dash read as a second glyph beside the
                            // calendar rather than as "nothing picked".
                            dueDate?.let { formatDueDate(it) } ?: "No date",
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
                LabelsPickerBlock(
                    labels = state.labels,
                    selectedIds = selectedLabelIds,
                    onToggle = { labelId, selected ->
                        selectedLabelIds =
                            if (selected) selectedLabelIds - labelId else selectedLabelIds + labelId
                    },
                    onOpenPicker = { labelSheetOpen = true },
                    modifier = Modifier.padding(horizontal = 4.dp),
                )

                if (state.error != null) {
                    Text(state.error!!, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
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

}

/** The placeholder scheme an image carries while its upload is still pending. */
private const val DRAFT_URL_PREFIX = "draft://"


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

/**
 * The files ALREADY uploaded against the draft (EXP-878). Unlike the share
 * path's pending URIs these are real attachment rows — uploaded the moment they
 * were picked, so they survive leaving the screen — and removing one deletes it.
 */
@Composable
private fun DraftAttachmentsSection(
    files: List<IssueDraftAttachment>,
    onRemove: (IssueDraftAttachment) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Text(
            "Files",
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        for (file in files) {
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
                    text = file.filename,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = { onRemove(file) }) {
                    Icon(
                        ExpIcons.uiClose,
                        contentDescription = "Remove ${file.filename}",
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
    }
}
