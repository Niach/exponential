package com.exponential.app.ui.issue

import android.content.Intent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.codingSessionDisplayState
import com.exponential.app.domain.issuePriorityOrder
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.EditorModel
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.LocalAttachmentDims
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.LocalMarkdownToolbarController
import com.exponential.app.ui.markdown.LocalMentions
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.MentionResolver
import com.exponential.app.ui.markdown.ProvideMarkdownToolbar
import com.exponential.app.ui.markdown.appendPickedImage
import com.exponential.app.ui.markdown.extractDescriptionMarkdown
import com.exponential.app.ui.markdown.stripDraftImages
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassRow
import kotlinx.coroutines.launch

// The per-property/combined sheets the detail screen can present (EXP-240).
// One nullable slot: children opened from the Properties sheet stack over it
// (propertiesOpen stays true beneath).
private enum class IssueSheet { Status, Priority, Assignee, Labels, DueDate, Duplicate, MoveBoard, StartCoding }

// Linear-mobile-style issue detail (EXP-240): centered "Issue" nav title,
// identifier chip + overflow header row, large editable title, the property
// chip box, the description editor, the agent/PR card, and the activity
// timeline — with a floating three-element bottom bar (properties circle,
// expanding comment pill, start-coding circle).
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IssueDetailScreen(
    issueId: String,
    onBack: () -> Unit,
    onOpenIssue: (String) -> Unit = {},
    onOpenSteer: (String) -> Unit = {},
    onOpenChanges: () -> Unit = {},
    viewModel: IssueDetailViewModel = hiltViewModel(),
    commentViewModel: CommentThreadViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val permissions by viewModel.permissions.collectAsStateWithLifecycle()
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val isSubscribed by viewModel.isSubscribed.collectAsStateWithLifecycle()
    val runningSession by viewModel.runningSession.collectAsStateWithLifecycle()
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val widgetSubmission by viewModel.widgetSubmission.collectAsStateWithLifecycle()
    val steerDevices by viewModel.steerDevices.collectAsStateWithLifecycle()
    val startState by viewModel.startState.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    val missing by viewModel.missing.collectAsStateWithLifecycle()
    val duplicateOf by viewModel.duplicateOf.collectAsStateWithLifecycle()
    val duplicateCandidates by viewModel.duplicateCandidates.collectAsStateWithLifecycle()
    val shareUrl by viewModel.shareUrl.collectAsStateWithLifecycle()
    val syncBanner by viewModel.syncBanner.collectAsStateWithLifecycle()
    // The board team's status rows (EXP-314) — picker vocabulary + chip label.
    val teamStatuses by viewModel.teamStatuses.collectAsStateWithLifecycle()
    val isModerator = permissions.isModerator
    // EXP-50: solo teams (one human member) hide the assignee chip/row.
    val soloMemberId by viewModel.soloMemberId.collectAsStateWithLifecycle()
    // EXP-487: the issue team's members — the assignee-picker + @-mention
    // vocabulary. state.users stays account-wide for display lookups.
    val teamUsers by viewModel.teamUsers.collectAsStateWithLifecycle()
    // EXP-57: same-team boards the issue can move to.
    val moveTargets by viewModel.moveTargets.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val issue = state.issue
    // Remote-reconciled title/description: seed on first load, live-apply a remote
    // edit while clean, stash + banner while dirty (field-level last-write-wins).
    // remember(issue?.id) gives the per-issue reset the old seed effect provided.
    val titleSync = remember(issue?.id) { RemoteSyncedText(normalizeForEcho = { it.trim() }) }
    val descriptionSync = remember(issue?.id) { RemoteSyncedText(normalizeForEcho = ::stripDraftImages) }
    var propertiesOpen by remember { mutableStateOf(false) }
    var activeSheet by remember { mutableStateOf<IssueSheet?>(null) }
    var confirmDelete by remember { mutableStateOf(false) }
    var overflowOpen by remember { mutableStateOf(false) }
    // The picked target board, pending the move confirmation (EXP-57).
    var moveTarget by remember { mutableStateOf<com.exponential.app.data.db.BoardEntity?>(null) }
    // The docked comment composer (bottom bar) expansion.
    var composerExpanded by remember { mutableStateOf(false) }

    // Hoisted so an image that arrived through the FILE path can still be
    // appended to the description instead of erroring (EXP-327).
    val descriptionModel = remember(issue?.id) { EditorModel() }
    LaunchedEffect(descriptionModel, viewModel) {
        viewModel.onInlineImagePicked = { uri, contentType ->
            scope.launch {
                appendPickedImage(context, descriptionModel, uri, contentType) { picked ->
                    viewModel.uploadImage(picked)
                }
            }
        }
    }
    DisposableEffect(viewModel) {
        onDispose { viewModel.onInlineImagePicked = null }
    }

    // The bar's comment half shares the thread's screen-scoped VM (hoisted
    // draft) — bind before either consumer renders.
    LaunchedEffect(issueId) { commentViewModel.bind(issueId) }
    val commentDraft by commentViewModel.draft.collectAsStateWithLifecycle()
    val commentSending by commentViewModel.sending.collectAsStateWithLifecycle()
    // Files queued for the next comment (EXP-554) — they upload on send.
    val commentAttachments by commentViewModel.pendingAttachments.collectAsStateWithLifecycle()

    // Own-save echo recognition (EXP-689): declared BEFORE the remote sync
    // effects so, within one composition, the save is on record by the time
    // its echo is reconciled.
    val lastSavedTitle by viewModel.lastSavedTitle.collectAsStateWithLifecycle()
    LaunchedEffect(titleSync, lastSavedTitle) {
        lastSavedTitle?.let { titleSync.markSaved(it) }
    }
    val lastSavedDescription by viewModel.lastSavedDescription.collectAsStateWithLifecycle()
    LaunchedEffect(descriptionSync, lastSavedDescription) {
        lastSavedDescription?.let { descriptionSync.markSaved(it) }
    }
    LaunchedEffect(titleSync, issue?.title) {
        issue?.title?.let { titleSync.syncRemote(it) }
    }
    val remoteDescription = issue?.let { extractDescriptionMarkdown(it.description) }
    LaunchedEffect(descriptionSync, remoteDescription) {
        remoteDescription?.let { descriptionSync.syncRemote(it) }
        // A clean live-apply supersedes any not-yet-flushed local input: without
        // this, the dispose-time flush would re-save text the user no longer sees.
        if (remoteDescription != null && !descriptionSync.isDirty) viewModel.discardPendingDescription()
    }

    // Surface failed description saves (retries exhausted) — the draft is
    // retained in the ViewModel, so the user knows to stay/retry instead of
    // believing the edit persisted.
    val descriptionSaveError by viewModel.descriptionSaveError.collectAsStateWithLifecycle()
    val snackbarHostState = remember { SnackbarHostState() }
    LaunchedEffect(descriptionSaveError) {
        descriptionSaveError?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeDescriptionSaveError()
        }
    }

    // Surface a failed move (EXP-57) — otherwise the issue silently stays put.
    val moveError by viewModel.moveError.collectAsStateWithLifecycle()
    LaunchedEffect(moveError) {
        moveError?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeMoveError()
        }
    }

    // Surface a failed inline label create (EXP-254 duplicate-name CONFLICT) —
    // otherwise the label sheet closes and nothing happens.
    val labelError by viewModel.labelError.collectAsStateWithLifecycle()
    LaunchedEffect(labelError) {
        labelError?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeLabelError()
        }
    }

    // Surface a refused property mutation — status/priority/due date/assignee/
    // labels/title, subscribe, duplicate, delete (REV2-50). The screen renders
    // synced state, so without this the tap read as "nothing happened".
    val mutationError by viewModel.mutationError.collectAsStateWithLifecycle()
    LaunchedEffect(mutationError) {
        mutationError?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.consumeMutationError()
        }
    }

    // Same for the comment thread (send / edit / delete).
    val commentError by commentViewModel.commentError.collectAsStateWithLifecycle()
    LaunchedEffect(commentError) {
        commentError?.let {
            snackbarHostState.showSnackbar(it)
            commentViewModel.consumeCommentError()
        }
    }

    // Remote-start feedback (EXP-240 — the inline captions left with the
    // card's start strip): failures surface as a snackbar. EXP-536: a
    // successful send says nothing — single and batch alike keep spinning in
    // the start circle until the session row syncs in, and the screen then
    // opens the live session below.
    LaunchedEffect(startState) {
        (startState as? SteerStartState.Failed)?.let {
            snackbarHostState.showSnackbar(it.message)
        }
    }

    // The desktop picked the start up — open the live session ONCE (EXP-536).
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }

    // Inline `#IDENTIFIER` pills + editor #-autocomplete (masterplan §5e):
    // resolve against this team's synced issues; a tap navigates to the
    // referenced issue. The CompositionLocal reaches every MarkdownView below
    // (description read view + comment thread) and every embedded editor
    // (description, comment composer, comment edit).
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    val currentOnOpenIssue by rememberUpdatedState(onOpenIssue)
    val issueRefHandler = remember(issueRefCandidates) {
        IssueRefHandler(issueRefCandidates) { target -> currentOnOpenIssue(target.issueId) }
    }

    // Inline `@email` mention pills (REV2-42): the same synced team members the
    // @-autocomplete offers, so a written mention renders as the member's name
    // in every read view below (description + comment thread) instead of a raw
    // address. Display-only — the stored markdown keeps the `@email` token.
    val mentionMembers = remember(teamUsers) {
        teamUsers.map { MentionMember(it.name ?: it.email, it.email) }
    }
    val mentionResolver = remember(mentionMembers) { MentionResolver(mentionMembers) }

    // Probed image sizes for the description + comment images (REV2-79).
    val attachmentDims by viewModel.attachmentDims.collectAsStateWithLifecycle()

    CompositionLocalProvider(
        LocalIssueRefs provides issueRefHandler,
        LocalMentions provides mentionResolver,
        LocalAttachmentDims provides attachmentDims,
    ) {
    ProvideMarkdownToolbar {
    Scaffold(
        topBar = {
            CenterAlignedTopAppBar(
                // EXP-568: the identifier titles the bar (it used to be a chip
                // in the content, under a generic "Issue" title).
                title = { Text(issue?.identifier ?: "") },
                navigationIcon = {
                    CircleIconButton(
                        ExpIcons.uiBack,
                        "Back",
                        onClick = onBack,
                        modifier = Modifier.padding(start = 8.dp),
                    )
                },
                actions = {
                    if (issue != null) {
                        // EXP-327: one `⋮` and nothing else — share and the
                        // subscribe toggle moved inside it (with words, so the
                        // bell's state is readable instead of guessed), next to
                        // Move to board. The MENU is available to everyone;
                        // only the mutating items are moderator-gated.
                        val url = shareUrl
                        // The Box stays: it anchors the dropdown to the button.
                        Box {
                            CircleIconButton(
                                ExpIcons.uiMore,
                                "Issue actions",
                                onClick = { overflowOpen = true },
                                modifier = Modifier.padding(end = 8.dp),
                            )
                            GlassDropdownMenu(expanded = overflowOpen, onDismissRequest = { overflowOpen = false }) {
                                if (url != null) {
                                    GlassMenuItem(
                                        leadingIcon = { Icon(ExpIcons.uiShare, contentDescription = null) },
                                        text = { Text("Share") },
                                        onClick = {
                                            overflowOpen = false
                                            val send = Intent(Intent.ACTION_SEND).apply {
                                                type = "text/plain"
                                                putExtra(
                                                    Intent.EXTRA_TEXT,
                                                    "${issue.identifier}: ${issue.title}\n$url",
                                                )
                                            }
                                            runCatching {
                                                context.startActivity(
                                                    Intent.createChooser(send, "Share issue"),
                                                )
                                            }
                                        },
                                    )
                                }
                                GlassMenuItem(
                                    leadingIcon = {
                                        // Menu row: the icon depicts the ACTION, like the
                                        // label beside it — bell-off next to "Unsubscribe",
                                        // not the current state (iOS parity).
                                        Icon(
                                            if (isSubscribed) ExpIcons.uiUnsubscribe else ExpIcons.uiSubscribe,
                                            contentDescription = null,
                                        )
                                    },
                                    text = { Text(if (isSubscribed) "Unsubscribe" else "Subscribe") },
                                    onClick = {
                                        overflowOpen = false
                                        viewModel.toggleSubscribe()
                                    },
                                )
                                // Duplicate = status interception (L27): marking a
                                // duplicate happens by picking the `duplicate` status,
                                // which opens the canonical-issue picker. Only the
                                // unmark action lives here.
                                if (isModerator && issue.duplicateOfId != null) {
                                    GlassMenuItem(
                                        leadingIcon = { Icon(ExpIcons.uiCopy, contentDescription = null) },
                                        text = { Text("Unmark duplicate") },
                                        onClick = {
                                            overflowOpen = false
                                            viewModel.unmarkDuplicate()
                                        },
                                    )
                                }
                                // Move to another board in the same team
                                // (EXP-57) — hidden when this is the team's
                                // only board (web parity: 2+ boards).
                                if (isModerator && moveTargets.isNotEmpty()) {
                                    GlassMenuItem(
                                        leadingIcon = { Icon(ExpIcons.navBoards, contentDescription = null) },
                                        text = { Text("Move to board") },
                                        onClick = {
                                            overflowOpen = false
                                            activeSheet = IssueSheet.MoveBoard
                                        },
                                    )
                                }
                                if (isModerator) {
                                    GlassMenuItem(
                                        leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                                        text = { Text("Delete issue") },
                                        destructive = true,
                                        onClick = {
                                            overflowOpen = false
                                            confirmDelete = true
                                        },
                                    )
                                }
                            }
                        }
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(
                    containerColor = Color.Transparent,
                ),
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
        containerColor = Color.Transparent,
    ) { padding ->
        if (issue == null) {
            if (missing == MissingIssueState.Unavailable) {
                Column(
                    modifier = Modifier.padding(padding).fillMaxSize().padding(horizontal = 24.dp),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("Issue not available", style = MaterialTheme.typography.titleMedium)
                    Spacer(Modifier.height(6.dp))
                    Text(
                        "It may have been deleted, or you may not have access.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TextButton(onClick = viewModel::retryFetch) { Text("Retry") }
                        TextButton(onClick = onBack) { Text("Go back") }
                    }
                }
            } else {
                // Still resolving: sync was kicked and the direct fetch may be
                // in flight — a spinner, not a dead end.
                LoadingState(modifier = Modifier.padding(padding))
            }
            return@Scaffold
        }

        val status = IssueStatusResolver.resolve(issue, teamStatuses)
        val priority = IssuePriority.fromWire(issue.priority)

        // Start-circle gating + content (EXP-240): hidden without steer /
        // membership / a repo-backed board, and until the device list has
        // loaded (null = the device lookup is in flight — no premature dimmed circle); a
        // live session shows its state dot; an in-flight send spins; otherwise
        // the play glyph (dimmed while no desktop is online — tapping then
        // explains via snackbar).
        val session = runningSession
        // EXP-312: the start circle deep-links into the live viewer, which is
        // owner-only — only the caller's OWN session flips it to the state
        // dot; a teammate's run shows in the Coding-now card and the circle
        // falls through to Start coding.
        val ownSession = session?.takeIf { it.userId == currentUserId }
        val devices = steerDevices
        val startAllowed = steerEnabled == true && permissions.isMember && state.board?.repositoryId != null
        val startUi: StartButtonUi? = when {
            !startAllowed -> null
            ownSession != null -> StartButtonUi.Session(codingSessionDisplayState(ownSession, issue.prState))
            startState is SteerStartState.Sending || startState is SteerStartState.Sent -> StartButtonUi.Sending
            devices == null -> null
            else -> StartButtonUi.Start(enabled = devices.isNotEmpty())
        }

        // The bar yields to the title/description keyboard (the markdown
        // toolbar owns that space); its own composer keeps it visible.
        val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
        // EXP-568: another markdown editor (the description, or a comment being
        // edited) holding the keyboard. The composer opts out of the floating
        // toolbar, so it never registers as the controller's activeModel — a
        // registered focused model is therefore always some OTHER editor. Two
        // editors used to stack on screen at once because the expanded composer
        // stayed up while the description took focus.
        val toolbarController = LocalMarkdownToolbarController.current
        // EXP-573: the back gesture dismisses the IME WITHOUT blurring the
        // editor, so focus alone must not keep the bar hidden — only an editor
        // that actually holds the keyboard does.
        val otherEditorFocused = toolbarController?.activeModel?.focusedRowId != null
        val barVisible = (composerExpanded || !imeVisible) && !(otherEditorFocused && imeVisible)

        // Tap-outside keyboard dismissal (EXP-246): a tap on dead space clears
        // focus and drops the IME. Children (chips, editors, the bar) consume
        // their own taps, so this only fires on genuinely empty areas; an
        // empty-draft composer then collapses via its own blur logic.
        val focusManager = LocalFocusManager.current
        val keyboard = LocalSoftwareKeyboardController.current

        Box(
            modifier = Modifier
                .padding(padding)
                // Shrink the scrollport above the keyboard: with edge-to-edge,
                // adjustResize alone never resizes the window, so without this
                // the focused editor line stays hidden behind the IME (EXP-135).
                // consumeWindowInsets keeps imePadding from re-adding the
                // nav-bar inset already applied by the Scaffold padding.
                .consumeWindowInsets(padding)
                .fillMaxSize()
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
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 20.dp, vertical = 8.dp)
                    .fillMaxWidth(),
            ) {
            SyncBannerRow(syncBanner)
            if (syncBanner != SyncBanner.None) Spacer(Modifier.height(8.dp))
            // Header: the origin chip only — the identifier moved to the nav
            // bar title (EXP-568). Origin chip: issues filed via the feedback
            // widget (source == "widget") or by a coding agent over MCP
            // (source == "agent", EXP-496) carry no user creator. Read-only
            // indicator. The repo chip renders once, above the agent/PR card
            // (EXP-170).
            if (issue.source == DomainContract.issueSourceWidget ||
                issue.source == DomainContract.issueSourceAgent
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    OriginChip(isAgent = issue.source == DomainContract.issueSourceAgent)
                }
            }

            // Conflict affordance: a remote edit to the title or description
            // arrived while that field was dirty/focused, so it was stashed rather
            // than clobbering the local edit. Tapping discards local text for the
            // remote value (until then it's last-write-wins — the local save
            // still overwrites the remote, matching iOS).
            if (titleSync.pendingRemote != null || descriptionSync.pendingRemote != null) {
                Spacer(Modifier.height(8.dp))
                RemoteEditBanner(onReload = {
                    titleSync.reloadPending()
                    if (descriptionSync.reloadPending()) viewModel.discardPendingDescription()
                })
            }

            // Canonical-issue banner (masterplan §5e): "Duplicate of {IDENTIFIER}"
            // with a clickable pill through to the canonical issue + Unmark.
            if (issue.duplicateOfId != null) {
                Spacer(Modifier.height(8.dp))
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassCard()
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        ExpIcons.statusDuplicate,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        "Duplicate of",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Spacer(Modifier.width(6.dp))
                    val canonical = duplicateOf
                    if (canonical != null) {
                        GlassPill(
                            canonical.identifier,
                            size = PillSize.Sm,
                            fontFamily = FontFamily.Monospace,
                            onClick = { onOpenIssue(canonical.id) },
                        )
                    } else {
                        Text(
                            "another issue",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    Spacer(Modifier.weight(1f))
                    if (isModerator) {
                        GlassPill(
                            "Unmark",
                            size = PillSize.Sm,
                            onClick = { viewModel.unmarkDuplicate() },
                        )
                    }
                }
            }

            Spacer(Modifier.height(8.dp))
            // Large title (borderless, save on focus-loss)
            BasicTextField(
                value = titleSync.text,
                onValueChange = { titleSync.onUserEdit(it) },
                readOnly = !isModerator,
                textStyle = MaterialTheme.typography.headlineSmall.copy(
                    color = MaterialTheme.colorScheme.onSurface,
                ),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.onSurface),
                modifier = Modifier
                    .fillMaxWidth()
                    .onFocusChanged { focus ->
                        titleSync.setFocused(focus.isFocused)
                        // Dirty is measured against the seed BASELINE, not the live
                        // row: a remote rename the user never touched leaves the
                        // field clean, so blur fires no save and the rename stands.
                        if (isModerator && !focus.isFocused && titleSync.text.isNotBlank() && titleSync.isDirty) {
                            viewModel.updateTitle(titleSync.text)
                        }
                    },
                decorationBox = { inner ->
                    if (titleSync.text.isEmpty()) {
                        Text(
                            "Title",
                            style = MaterialTheme.typography.headlineSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    inner()
                },
            )

            Spacer(Modifier.height(12.dp))
            // The top property chip box (EXP-240) — replaces the stacked
            // property/times cards + labels section.
            IssuePropertyChips(
                issue = issue,
                status = status,
                priority = priority,
                assignee = state.assignee,
                issueLabels = state.issueLabels,
                isModerator = isModerator,
                hideAssignee = soloMemberId != null,
                onOpenStatus = { activeSheet = IssueSheet.Status },
                onOpenPriority = { activeSheet = IssueSheet.Priority },
                onOpenAssignee = { activeSheet = IssueSheet.Assignee },
                onOpenDueDate = { activeSheet = IssueSheet.DueDate },
                onOpenLabels = { activeSheet = IssueSheet.Labels },
                onOpenProperties = { propertiesOpen = true },
            )

            // EXP-698 r4: the live run sits in its OWN box directly under the
            // property chips — same chrome, same width — instead of below the
            // description where it read as an afterthought. The PR/branch rows
            // stay down there, next to the code they link to.
            if (session != null) {
                Spacer(Modifier.height(16.dp))
                CodingNowCard(
                    session = session,
                    prState = issue.prState,
                    sessionOwner = state.users.firstOrNull { it.id == session.userId },
                    steerEnabled = steerEnabled,
                    currentUserId = currentUserId,
                    onWatch = onOpenSteer,
                )
            }

            Spacer(Modifier.height(16.dp))
            MarkdownEditor(
                model = descriptionModel,
                markdown = descriptionSync.text,
                editable = isModerator,
                onChange = {
                    descriptionSync.onUserEdit(it)
                    viewModel.updateDescription(it)
                },
                onUploadImage = if (isModerator) { uri -> viewModel.uploadImage(uri) } else null,
                imageUploadEnabled = isModerator,
                mentionMembers = mentionMembers,
                onFocusChanged = { descriptionSync.setFocused(it) },
                // EXP-327: the description editor is the ONE attach affordance;
                // non-image picks land in the Files section below.
                onAttachFile = if (isModerator) { uri -> viewModel.uploadFile(uri) } else null,
                // EXP-627: the store slide's pop-out rect is measured off this
                // block (`PopRects`), iOS parity.
                modifier = Modifier.testTag("issue-description"),
            )
            DisposableEffect(Unit) {
                onDispose { viewModel.flushDescription() }
            }

            // The PR/branch rows (EXP-156) linking to the dedicated Changes
            // page. Start moved to the bottom bar (EXP-240) and the live
            // session to its own card above (EXP-698 r4), so this renders only
            // with a PR or a pushed branch.
            val cardVisible = !issue.prUrl.isNullOrBlank() || !issue.branch.isNullOrBlank()
            if (cardVisible) {
                Spacer(Modifier.height(20.dp))
                // EXP-327: no repo chip here — the PR row itself is the link to
                // the code, and the chip only repeated what the board already
                // says (Linear parity).
                AgentPrCard(
                    issue = issue,
                    onOpenChanges = onOpenChanges,
                )
            }

            // Widget/agent submission metadata (EXP-496): expandable card,
            // default collapsed; renders nothing without a submission row.
            widgetSubmission?.let { submission ->
                WidgetSubmissionCard(
                    submission = submission,
                    isAgent = issue.source == DomainContract.issueSourceAgent,
                    modifier = Modifier.padding(top = 20.dp),
                )
            }

            // Non-image attachments (EXP-297) — they never appear in the
            // markdown, so this is the only surface they exist on. EXP-327:
            // it renders (and pads) nothing at all when there are no files;
            // attaching happens from the description editor's attach menu.
            IssueFilesSection(
                viewModel = viewModel,
                canDelete = permissions.isMember,
                modifier = Modifier.padding(top = 20.dp),
            )

            Spacer(Modifier.height(20.dp))
            CommentThread(
                issueId = issue.id,
                viewModel = commentViewModel,
            )

            // Clearance so the last timeline row scrolls out from under the
            // floating bar (kept in sync with the nav pill inset, EXP-36).
            Spacer(Modifier.height(BottomBarInset))
            }

            // The floating bottom bar / docked composer. Lives INSIDE the
            // ProvideMarkdownToolbar content (which bottom-insets by the
            // toolbar height), with a single imePadding — so the stack is
            // IME → markdown toolbar → composer.
            Box(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .imePadding(),
            ) {
                AnimatedVisibility(
                    visible = barVisible,
                    // EXP-523: the shared `standard` token (180ms, unchanged).
                    enter = fadeIn(Motion.standard()),
                    exit = fadeOut(Motion.standard()),
                ) {
                    IssueDetailBottomBar(
                        expanded = composerExpanded,
                        onExpandedChange = { composerExpanded = it },
                        showProperties = isModerator,
                        onOpenProperties = { propertiesOpen = true },
                        startButton = startUi,
                        onStartClick = {
                            when {
                                ownSession != null -> onOpenSteer(ownSession.id)
                                startState is SteerStartState.Sending || startState is SteerStartState.Sent -> Unit
                                steerDevices.isNullOrEmpty() -> scope.launch {
                                    snackbarHostState.showSnackbar(
                                        "No desktop online. Open the Exponential desktop app to run here.",
                                    )
                                }
                                else -> activeSheet = IssueSheet.StartCoding
                            }
                        },
                        draft = commentDraft,
                        onDraftChange = commentViewModel::updateDraft,
                        sending = commentSending,
                        onSend = { commentViewModel.send { composerExpanded = false } },
                        pendingAttachments = commentAttachments,
                        onAddAttachment = commentViewModel::addPendingAttachment,
                        onRemoveAttachment = commentViewModel::removePendingAttachment,
                        mentionMembers = mentionMembers,
                        showMentionButton = soloMemberId == null,
                        otherEditorFocused = otherEditorFocused,
                    )
                }
            }
        }
    }
    }
    }

    // ── Sheets ────────────────────────────────────────────────────────────────

    if (propertiesOpen && issue != null && isModerator) {
        PropertiesSheet(
            issue = issue,
            status = IssueStatusResolver.resolve(issue, teamStatuses),
            priority = IssuePriority.fromWire(issue.priority),
            assignee = state.assignee,
            hideAssignee = soloMemberId != null,
            issueLabels = state.issueLabels,
            currentBoard = state.board,
            hasMoveTargets = moveTargets.isNotEmpty(),
            onOpenStatus = { activeSheet = IssueSheet.Status },
            onOpenPriority = { activeSheet = IssueSheet.Priority },
            onOpenAssignee = { activeSheet = IssueSheet.Assignee },
            onOpenDueDate = { activeSheet = IssueSheet.DueDate },
            onOpenLabels = { activeSheet = IssueSheet.Labels },
            onOpenMoveBoard = { activeSheet = IssueSheet.MoveBoard },
            onToggleLabel = { id, assigned -> viewModel.toggleLabel(id, assigned) },
            onDismiss = { propertiesOpen = false },
        )
    }

    if (activeSheet == IssueSheet.Status && issue != null && isModerator) {
        val currentStatus = IssueStatusResolver.resolve(issue, teamStatuses)
        IssuePickerSheet(
            title = "Status",
            items = teamStatuses,
            selected = currentStatus,
            keyOf = { it.id },
            labelOf = { it.name },
            leadingContent = { StatusIcon(it, size = 16.dp) },
            onSelect = {
                // Duplicate = status interception (L27): picking a
                // duplicate-CATEGORY status opens the canonical-issue picker
                // instead of writing the status directly; markDuplicate sets
                // duplicateOfId + status='duplicate' atomically (still the enum
                // path — EXP-314). Cancelling leaves the status untouched.
                if (it.category == IssueStatusCategory.Duplicate) {
                    activeSheet = IssueSheet.Duplicate
                } else {
                    viewModel.updateStatus(it)
                }
            },
            onDismiss = { if (activeSheet == IssueSheet.Status) activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Priority && issue != null && isModerator) {
        val currentPriority = IssuePriority.fromWire(issue.priority)
        IssuePickerSheet(
            title = "Priority",
            items = issuePriorityOrder,
            selected = currentPriority,
            labelOf = { it.label },
            leadingContent = { PriorityIcon(it, size = 16.dp) },
            onSelect = { viewModel.updatePriority(it) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Assignee && issue != null && isModerator) {
        AssigneePickerSheet(
            users = teamUsers,
            selectedUserId = issue.assigneeId,
            onSelect = { viewModel.updateAssignee(it) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.DueDate && issue != null && isModerator) {
        DueDateSheet(
            dueDate = issue.dueDate,
            onSetDate = { viewModel.updateDueDate(it) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Labels && issue != null && isModerator) {
        LabelPickerSheet(
            teamLabels = state.teamLabels,
            selectedLabelIds = state.issueLabels.map { it.id }.toSet(),
            onToggle = { id, assigned -> viewModel.toggleLabel(id, assigned) },
            onCreate = { name, color -> viewModel.createAndAssignLabel(name, color) },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.Duplicate && issue != null && isModerator) {
        DuplicatePickerSheet(
            candidates = duplicateCandidates,
            onPick = { viewModel.markDuplicate(it.id) },
            onDismiss = { activeSheet = null },
        )
    }

    // Move to board (EXP-57): pick a same-team target, then confirm —
    // the move renumbers the issue (new identifier), so it's consequential.
    if (activeSheet == IssueSheet.MoveBoard && issue != null && isModerator) {
        IssuePickerSheet(
            title = "Move to board",
            items = moveTargets,
            selected = null,
            keyOf = { it.id },
            labelOf = { it.name },
            leadingContent = { BoardIcon(it, size = 18.dp) },
            onSelect = { moveTarget = it },
            onDismiss = { activeSheet = null },
        )
    }

    if (activeSheet == IssueSheet.StartCoding && issue != null) {
        StartCodingSheet(
            devices = steerDevices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = setOf(issue.id),
            onStart = viewModel::startOnDesktop,
            onRunAction = viewModel::runAction,
            onDismiss = { activeSheet = null },
        )
    }

    val pendingMoveTarget = moveTarget
    if (pendingMoveTarget != null && issue != null) {
        AlertDialog(
            onDismissRequest = { moveTarget = null },
            title = { Text("Move issue") },
            text = {
                Text(
                    "Move ${issue.identifier} to \"${pendingMoveTarget.name}\"? " +
                        "The issue will get a new identifier in that board.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    moveTarget = null
                    viewModel.moveToBoard(pendingMoveTarget.id)
                }) {
                    Text("Move")
                }
            },
            dismissButton = {
                TextButton(onClick = { moveTarget = null }) { Text("Cancel") }
            },
        )
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete issue") },
            text = { Text("This action cannot be undone.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    viewModel.delete(onBack)
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDelete = false }) { Text("Cancel") }
            },
        )
    }
}

// Non-blocking conflict banner: a teammate changed the title or description
// while this field was being edited, so the remote value was stashed. Tapping
// discards the local edit and loads the remote value. Matches the SyncBannerRow
// glass-row idiom.
@Composable
private fun RemoteEditBanner(onReload: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .clickable { onReload() }
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            ExpIcons.uiRefresh,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Text(
            "Updated by someone else. Tap to reload",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

// Origin pill for issues without a user creator: "Feedback widget"
// (source == "widget") or "Agent" (source == "agent", EXP-496) — a muted,
// read-only indicator built on the shared glass chip idiom.
@Composable
private fun OriginChip(isAgent: Boolean) {
    GlassPill(
        if (isAgent) "Agent" else "Feedback widget",
        size = PillSize.Sm,
        mode = PillMode.Readonly,
        icon = if (isAgent) ExpIcons.uiAgentSource else ExpIcons.uiWidget,
    )
}
