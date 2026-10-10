package com.exponential.app.ui.issue

import android.content.Intent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueDraftPage
import com.exponential.app.domain.IssueRelationsView
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.navigation.LocalLeaveGuard
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.detailHazeSource
import com.exponential.app.ui.components.issuePriorityPickerOptions
import com.exponential.app.ui.components.pickedPriority
import com.exponential.app.ui.components.picker.AssigneePicker
import com.exponential.app.ui.components.picker.BoardPicker
import com.exponential.app.ui.components.picker.PriorityPicker
import com.exponential.app.ui.components.picker.StatusPicker
import com.exponential.app.ui.components.toPickerBoard
import com.exponential.app.ui.components.toPickerMember
import com.exponential.app.ui.components.toPickerRow
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.LocalMentions
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.MentionResolver
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.share.ShareBoardPickerSheet
import com.exponential.app.ui.share.SharePrefill
import com.exponential.app.ui.share.TeamBoards
import com.exponential.app.ui.work.LocalTitleCollapse
import com.exponential.app.ui.work.TitleCollapseState
import com.exponential.app.ui.work.WorkTopBar
import dev.chrisbanes.haze.rememberHazeState
import kotlinx.coroutines.android.awaitFrame
import kotlinx.coroutines.launch

private enum class DraftSheet { Status, Priority, Assignee, Labels, DueDate, Board }

/**
 * EXP-1170: the New issue PAGE — the phone issue face (`IssueFace` inside
 * `WorkScreen`) in draft mode, ONE view ×4. The header is the Work screen's
 * (`New issue` collapsing to the typed title; Create + an `×` that is
 * Discard draft, EXP-1191); the body is title → property chips (+ board) → description →
 * Files, nothing else. Everything autosaves into the draft row
 * ([IssueDraftViewModel]); share mode and parent mode write no row.
 */
@Composable
fun IssueDraftScreen(
    onBack: () -> Unit,
    /** Filed: land on it (`openCreatedIssue`, so Back returns to the origin). */
    onCreated: (String) -> Unit,
    shareMode: Boolean = false,
    sharePrefill: SharePrefill? = null,
    onSharePrefillConsumed: () -> Unit = {},
    shareGroups: List<TeamBoards> = emptyList(),
    shareRecentBoardId: String? = null,
    shareGroupsLoading: Boolean = false,
    viewModel: IssueDraftViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val statuses by viewModel.statuses.collectAsStateWithLifecycle()
    val labels by viewModel.labels.collectAsStateWithLifecycle()
    val teamUsers by viewModel.teamUsers.collectAsStateWithLifecycle()
    val soloMemberId by viewModel.soloMemberId.collectAsStateWithLifecycle()
    val board by viewModel.board.collectAsStateWithLifecycle()
    val teamBoards by viewModel.teamBoards.collectAsStateWithLifecycle()
    val parentIssue by viewModel.parentIssue.collectAsStateWithLifecycle()
    val busyIds by viewModel.busyIds.collectAsStateWithLifecycle()
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    val isModerator = permissions.isModerator
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val toaster = LocalToaster.current
    var sheet by remember { mutableStateOf<DraftSheet?>(null) }

    // Share mode: the last-used board (if it still exists), else the first.
    LaunchedEffect(shareGroups, shareRecentBoardId) {
        if (!shareMode || state.boardId != null) return@LaunchedEffect
        val ids = shareGroups.flatMap { g -> g.boards.map { it.id } }
        (shareRecentBoardId?.takeIf { it in ids } ?: ids.firstOrNull())?.let(viewModel::setBoard)
    }
    LaunchedEffect(sharePrefill) {
        if (shareMode && sharePrefill != null) viewModel.applySharePrefill(sharePrefill)
    }

    val message by viewModel.message.collectAsStateWithLifecycle()
    LaunchedEffect(message) {
        message?.let {
            toaster.error(it)
            viewModel.consumeMessage()
        }
    }
    LaunchedEffect(state.loadFailed) {
        if (state.loadFailed) onBack()
    }

    val uploadsInFlight by viewModel.uploadsInFlight.collectAsStateWithLifecycle()
    val canCreate = IssueDraftPage.createEnabled(state.title, state.boardId != null, state.creating, uploadsInFlight)

    // EXP-1212: a draft WITH content never goes silently, and nothing is
    // asked while a Create is in flight. Share and parent mode write no draft
    // row: their leave prompt has no Keep (`leaveChoices`).
    val hasContent = IssueDraftPage.hasContent(
        state.title,
        state.description,
        state.attachments.size + state.heldFiles.size + state.pendingUploads.size,
        attachmentsKnown = state.attachmentsKnown,
    )
    fun prompt() = IssueDraftPage.prompt(hasContent, creating = state.creating)
    val leaveChoices = IssueDraftPage.leaveChoices(canKeep = !viewModel.deferUploads)
    // The navigation the leave prompt holds; null = no prompt.
    var heldLeave by remember { mutableStateOf<(() -> Unit)?>(null) }
    // The leave prompt's Create: continue THIS instead of opening the issue.
    var afterCreate by remember { mutableStateOf<(() -> Unit)?>(null) }
    // Keep as draft's awaited save: the page holds still until it lands.
    var keeping by remember { mutableStateOf(false) }

    fun leave() {
        if (state.creating || keeping) return
        if (prompt() == IssueDraftPage.Prompt.Leave) {
            heldLeave = onBack
            return
        }
        viewModel.leave()
        onBack()
    }
    BackHandler(enabled = true) { leave() }
    // Every other way out (a push tap, a share, a deep link) asks AppNavHost's
    // guard, which hands it here while the draft has content.
    val leaveGuard = LocalLeaveGuard.current
    val holdsLeave by rememberUpdatedState(
        !keeping && prompt() == IssueDraftPage.Prompt.Leave,
    )
    DisposableEffect(leaveGuard) {
        val unregister = leaveGuard?.register { proceed ->
            if (holdsLeave) heldLeave = proceed
            holdsLeave
        }
        onDispose { unregister?.invoke() }
    }
    // Every way out flushes; the model's own clearing is the last write.
    DisposableEffect(viewModel) { onDispose { viewModel.flush() } }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { viewModel.flush() }

    // The create runs on the model's scope (a rotation never cancels it);
    // its one-shot result lands here.
    val createdIssueId by viewModel.createdIssueId.collectAsStateWithLifecycle()
    LaunchedEffect(createdIssueId) {
        val id = createdIssueId ?: return@LaunchedEffect
        viewModel.consumeCreated()
        if (sharePrefill != null) onSharePrefillConsumed()
        val held = afterCreate
        afterCreate = null
        if (held != null) held() else onCreated(id)
    }
    // EXP-1231: another client consumed this draft. The page's OWN exits,
    // like its Create: no leave prompt, no LeaveGuard, and the model is
    // already sealed + left, so the dispose flush / leave write nothing.
    val consumedElsewhere by viewModel.consumedElsewhere.collectAsStateWithLifecycle()
    LaunchedEffect(consumedElsewhere) {
        val consumed = consumedElsewhere ?: return@LaunchedEffect
        viewModel.consumeConsumed()
        heldLeave = null
        afterCreate = null
        when (consumed) {
            // Created elsewhere: this page becomes that issue, no prompt, no toast.
            is IssueDraftViewModel.Consumed.Created -> onCreated(consumed.issueId)
            IssueDraftViewModel.Consumed.Discarded -> {
                toaster.info(IssueDraftPage.DISCARDED_ELSEWHERE)
                onBack()
            }
        }
    }
    // A failed create stays on the page (its error toasts) and drops the
    // held navigation.
    LaunchedEffect(state.creating) {
        if (!state.creating) afterCreate = null
    }
    // The page's own Create lands on the issue: never a stale held navigation.
    fun create() {
        afterCreate = null
        if (canCreate) viewModel.create()
    }
    // Never while a Create is in flight (it would delete the row the create
    // reparents from).
    fun discard() {
        if (state.creating) return
        viewModel.discard()
        if (shareMode && sharePrefill != null) onSharePrefillConsumed()
    }

    val issueRefHandler = remember(issueRefCandidates) {
        // Nowhere to navigate from an unsent issue: a chip tap falls through
        // to the caret (EXP-423).
        IssueRefHandler(issueRefCandidates, canOpen = false, searchServer = viewModel::searchIssueRefs) { }
    }
    val mentionMembers = remember(teamUsers) { teamUsers.map { MentionMember(it.name ?: it.email, it.email) } }
    val mentionResolver = remember(mentionMembers) { MentionResolver(mentionMembers) }

    // The Work screen's detail chrome: one blurred backdrop + the title collapse.
    val hazeState = rememberHazeState()
    val titleCollapse = remember { TitleCollapseState() }
    val titleFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        awaitFrame()
        runCatching { titleFocus.requestFocus() }
    }

    val boardLabel: String? = when {
        shareMode -> shareGroups.flatMap { it.boards }.firstOrNull { it.id == state.boardId }?.name
            ?: if (shareGroupsLoading) "…" else "Choose board"
        teamBoards.size > 1 -> board?.name
        else -> null
    }

    ProvideMarkdownToolbar {
        CompositionLocalProvider(
            LocalDetailHaze provides hazeState,
            LocalTitleCollapse provides titleCollapse,
            LocalIssueRefs provides issueRefHandler,
            LocalMentions provides mentionResolver,
        ) {
            Scaffold(
                containerColor = Color.Transparent,
                topBar = {
                    WorkTopBar(
                        title = IssueDraftPage.HEADER,
                        collapsedTitle = state.title.trim().ifEmpty { IssueDraftPage.UNTITLED },
                        collapsed = titleCollapse.issueTitleCollapsed,
                        onHeaderBottom = titleCollapse::reportHeaderBottom,
                        onBack = ::leave,
                        verb = null,
                        verbEnabled = false,
                        onVerb = {},
                        action = {
                            GlassPill(
                                IssueDraftPage.CREATE,
                                onClick = ::create,
                                size = PillSize.Sm,
                                primary = true,
                                enabled = canCreate,
                                loading = state.creating,
                                modifier = Modifier.padding(end = 4.dp).testTag("create-issue-submit"),
                            )
                        },
                        // EXP-1247: Back + Create only; discarding is the leave prompt's answer.
                        menu = null,
                        tabs = null,
                    )
                },
            ) { padding ->
                val focusManager = LocalFocusManager.current
                val keyboard = LocalSoftwareKeyboardController.current
                val layoutDirection = LocalLayoutDirection.current
                Box(
                    modifier = Modifier
                        .padding(
                            start = padding.calculateStartPadding(layoutDirection),
                            end = padding.calculateEndPadding(layoutDirection),
                        )
                        .fillMaxSize()
                        // Tap-outside keyboard dismissal (EXP-246).
                        .pointerInput(Unit) {
                            detectTapGestures(onTap = {
                                focusManager.clearFocus()
                                keyboard?.hide()
                            })
                        },
                ) {
                    Column(
                        modifier = Modifier
                            .imePadding()
                            .detailHazeSource()
                            .verticalScroll(rememberScrollState())
                            .padding(top = padding.calculateTopPadding())
                            .padding(horizontal = 20.dp, vertical = 8.dp)
                            .fillMaxWidth(),
                    ) {
                        // Parent mode: "Sub-issue of [chip]" above the title.
                        parentIssue?.let { parent ->
                            Spacer(Modifier.height(8.dp))
                            SubIssueOfLine(
                                parent = IssueRelationsView.Row(
                                    id = parent.id,
                                    identifier = parent.identifier,
                                    title = parent.title,
                                    status = parent.status,
                                    open = IssueRelationsView.isOpenAnchor(parent.status),
                                ),
                                parentIssue = parent,
                                statuses = statuses,
                                onOpen = {},
                                onRemove = null,
                            )
                        }

                        Spacer(Modifier.height(8.dp))
                        IssueTitleField(
                            text = state.title,
                            onChange = viewModel::onTitleChange,
                            placeholder = IssueDraftPage.TITLE_PLACEHOLDER,
                            onFocusChanged = { focused -> if (!focused) viewModel.flush() },
                            modifier = Modifier
                                .focusRequester(titleFocus)
                                .testTag("create-issue-title-field"),
                        )

                        Spacer(Modifier.height(12.dp))
                        IssuePropertyChips(
                            subject = IssuePropertySubject(
                                priority = state.priority,
                                assigneeId = state.assigneeId,
                                dueDate = state.dueDate,
                                estimate = null,
                            ),
                            status = state.status,
                            assignee = teamUsers.firstOrNull { it.id == state.assigneeId },
                            issueLabels = labels.filter { it.id in state.labelIds },
                            isModerator = isModerator,
                            hideAssignee = soloMemberId != null,
                            estimationType = DomainContract.issueEstimationNone,
                            onOpenStatus = { sheet = DraftSheet.Status },
                            onOpenPriority = { sheet = DraftSheet.Priority },
                            onOpenAssignee = { sheet = DraftSheet.Assignee },
                            onOpenDueDate = { sheet = DraftSheet.DueDate },
                            onOpenEstimate = {},
                            onOpenLabels = { sheet = DraftSheet.Labels },
                            onOpenProperties = null,
                            board = boardLabel,
                            onOpenBoard = { sheet = DraftSheet.Board },
                            boardEntity = if (shareMode) null else board,
                        )

                        Spacer(Modifier.height(16.dp))
                        MarkdownEditor(
                            markdown = state.description,
                            editable = true,
                            onChange = viewModel::onDescriptionChange,
                            onUploadImage = { uri -> viewModel.uploadImage(uri) },
                            onUploadMedia = { media -> viewModel.uploadMedia(media) },
                            imageUploadEnabled = true,
                            placeholder = IssueDraftPage.DESCRIPTION_PLACEHOLDER,
                            initialPendingImages = remember { sharePrefill?.pendingImages.orEmpty() },
                            mentionMembers = mentionMembers,
                            onFocusChanged = { focused -> if (!focused) viewModel.flush() },
                            onAttachFile = viewModel::attachFile,
                            modifier = Modifier.testTag("issue-description"),
                        )

                        val heldIds = state.heldFiles.map { it.key }.toSet()
                        FilesSection(
                            files = state.attachments.map { FileItem(it.id, it.filename, it.contentType, it.sizeBytes) } +
                                state.heldFiles.map { FileItem(it.key, it.filename, "", null) },
                            pending = state.pendingUploads,
                            busyIds = busyIds,
                            canDelete = true,
                            onDelete = { id ->
                                if (id in heldIds) viewModel.removeHeldFile(id) else viewModel.deleteAttachment(id)
                            },
                            onOpen = { item ->
                                val held = state.heldFiles.firstOrNull { it.key == item.id }
                                if (held != null) {
                                    runCatching {
                                        context.startActivity(
                                            Intent(Intent.ACTION_VIEW, held.uri)
                                                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
                                        )
                                    }
                                } else {
                                    val file = state.attachments.firstOrNull { it.id == item.id } ?: return@FilesSection
                                    scope.launch {
                                        val local = viewModel.downloadToCache(file) ?: return@launch
                                        openFile(context, local, file.contentType)
                                    }
                                }
                            },
                            onRetry = viewModel::retryFileUpload,
                            onDismissPending = viewModel::dismissFileUpload,
                            modifier = Modifier.padding(top = 20.dp),
                        )

                        Spacer(Modifier.height(BottomBarInset + padding.calculateBottomPadding()))
                    }
                }
            }
        }
    }

    // ── The leave prompt (EXP-1212) ─────────────────────────────────────────
    heldLeave?.let { proceed ->
        // One question: Discard (quiet, destructive) set apart on the leading
        // edge; Save draft (plain pill) · Create issue (primary pill, the default)
        // on the trailing edge. While Create is disabled (no title) Save draft
        // takes the focus. Dismissing (scrim, back) cancels the navigation.
        val canKeep = IssueDraftPage.LeaveChoice.Keep in leaveChoices
        val createAction = GlassAlertAction(
            label = IssueDraftPage.LEAVE_CREATE,
            primary = true,
            enabled = canCreate,
            testTag = "issue-draft-leave-create",
            onClick = {
                heldLeave = null
                if (canCreate) {
                    afterCreate = proceed
                    viewModel.create()
                }
            },
        )
        val keepAction = GlassAlertAction(
            label = IssueDraftPage.LEAVE_KEEP,
            testTag = "issue-draft-leave-keep",
            onClick = {
                heldLeave = null
                afterCreate = null
                keeping = true
                // Awaited: a failed save stays here (the model toasts it)
                // and drops the navigation.
                scope.launch {
                    val kept = try {
                        viewModel.keep()
                    } finally {
                        keeping = false
                    }
                    if (kept) proceed()
                }
            },
        )
        // The trailing row in `leaveChoices` order (Save draft · Create issue).
        val trailingChoices = leaveChoices.filter { it != IssueDraftPage.LeaveChoice.Discard }
        val trailing = trailingChoices.map { choice ->
            if (choice == IssueDraftPage.LeaveChoice.Keep) keepAction else createAction
        }
        GlassAlert(
            title = IssueDraftPage.LEAVE_TITLE,
            onDismiss = {
                heldLeave = null
                afterCreate = null
            },
            leading = if (IssueDraftPage.LeaveChoice.Discard in leaveChoices) {
                GlassAlertAction(
                    label = IssueDraftPage.LEAVE_DISCARD,
                    testTag = "issue-draft-leave-discard",
                    onClick = {
                        heldLeave = null
                        afterCreate = null
                        if (!state.creating) {
                            discard()
                            proceed()
                        }
                    },
                )
            } else {
                null
            },
            trailing = trailing,
            defaultAction = IssueDraftPage.leaveDefault(leaveChoices, canCreate)
                ?.let { choice -> trailingChoices.indexOf(choice).takeIf { it >= 0 } },
        )
    }

    // ── Sheets (each pick saves at once) ────────────────────────────────────
    when (sheet) {
        DraftSheet.Status -> {
            // A new issue cannot be a duplicate (nothing to link yet).
            val creatable = statuses.filter { it.category != IssueStatusCategory.Duplicate }
            StatusPicker(
                statuses = creatable.map { it.toPickerRow() },
                value = setOf(state.status.id),
                onChange = { picked ->
                    picked.firstOrNull()?.let { id -> creatable.firstOrNull { it.id == id } }?.let(viewModel::setStatus)
                },
                open = true,
                onOpenChange = { open -> if (!open) sheet = null },
            )
        }
        DraftSheet.Priority -> PriorityPicker(
            options = issuePriorityPickerOptions(),
            value = setOf(state.priority.wire),
            onChange = { picked -> pickedPriority(picked)?.let { p -> viewModel.onChipChange { it.copy(priority = p) } } },
            open = true,
            onOpenChange = { open -> if (!open) sheet = null },
        )
        DraftSheet.Assignee -> AssigneePicker(
            members = teamUsers.map { it.toPickerMember() },
            value = setOfNotNull(state.assigneeId),
            onChange = { picked -> viewModel.onChipChange { it.copy(assigneeId = picked.firstOrNull()) } },
            open = true,
            onOpenChange = { open -> if (!open) sheet = null },
        )
        DraftSheet.DueDate -> DueDateSheet(
            dueDate = state.dueDate,
            onSetDate = { date -> viewModel.onChipChange { it.copy(dueDate = date) } },
            onDismiss = { sheet = null },
        )
        DraftSheet.Labels -> LabelPickerSheet(
            teamLabels = labels,
            selectedLabelIds = state.labelIds,
            onToggle = { id, selected ->
                viewModel.onChipChange { it.copy(labelIds = if (selected) it.labelIds - id else it.labelIds + id) }
            },
            // A label created here is real at once; pre-select it.
            onCreate = { name, color ->
                scope.launch {
                    viewModel.createLabel(name, color)?.let { created ->
                        viewModel.onChipChange { it.copy(labelIds = it.labelIds + created.id) }
                    }
                }
            },
            onDismiss = { sheet = null },
        )
        DraftSheet.Board -> if (shareMode) {
            ShareBoardPickerSheet(
                groups = shareGroups,
                selectedBoardId = state.boardId,
                onSelect = viewModel::setBoard,
                onDismiss = { sheet = null },
            )
        } else {
            // Picks directly: nothing exists yet, so no move confirm.
            BoardPicker(
                boards = teamBoards.map { it.toPickerBoard() },
                value = state.boardId,
                onChange = viewModel::setBoard,
                open = true,
                onOpenChange = { open -> if (!open) sheet = null },
            )
        }
        null -> Unit
    }
}
