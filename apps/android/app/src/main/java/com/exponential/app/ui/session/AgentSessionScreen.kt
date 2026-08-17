package com.exponential.app.ui.session

import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.PatchLines
import com.exponential.app.ui.issue.splitUnifiedDiff
import com.exponential.app.ui.issue.unifiedDiffStats
import com.exponential.app.ui.issue.DiffAddColor
import com.exponential.app.ui.issue.DiffDelColor
import com.exponential.app.ui.markdown.LocalAttachmentDims
import com.exponential.app.ui.markdown.LocalMarkdownAutolink
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// The "Agent session" screen (EXP-32) — a chat-style view of a live coding
// session over the relay's scrubbed activity channel. NO terminal rendering:
// narration bubbles, compact tool rows, collapsible subagent groups, question
// steppers, a pinned "Latest changes" diff chip above the input bar, and
// message-shaped steering (text + \r, perm-gated by the relay).
// Identical UX to the iOS AgentSessionView (glass design system).

private val LiveGreen = Color(0xFF34D399)
private val ConnectingYellow = Color(0xFFFBBF24)
private val LostGray = Color(0xFF71717A)
/** Accent for the "Plan ready" card + header cue (EXP-97). */
private val PlanAccent = DesignTokens.Semantic.Blue

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentSessionScreen(
    onBack: () -> Unit,
    viewModel: AgentSessionViewModel = hiltViewModel(),
) {
    val session by viewModel.session.collectAsStateWithLifecycle()
    val phase by viewModel.phase.collectAsStateWithLifecycle()
    val activity by viewModel.activity.collectAsStateWithLifecycle()
    val feed = activity.feed
    val latestDiff = activity.latestDiff
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val killError by viewModel.killError.collectAsStateWithLifecycle()
    val attachmentDims by viewModel.attachmentDims.collectAsStateWithLifecycle()
    val answerStates = activity.answerLocks
    val pendingImages by viewModel.pendingImages.collectAsStateWithLifecycle()
    val steerSending by viewModel.steerSending.collectAsStateWithLifecycle()
    val steerImageError by viewModel.steerImageError.collectAsStateWithLifecycle()

    // Steer image attach (EXP-511) — the system photo picker feeds the VM's
    // pending list; batch and action runs have no issue to upload to.
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val imagePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.PickMultipleVisualMedia(MAX_STEER_IMAGES),
    ) { uris: List<Uri> ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            uris.forEach { uri ->
                // ContentResolver reads can stream from a cloud-backed provider —
                // never on the main thread.
                val picked = withContext(Dispatchers.IO) {
                    val bytes = MarkdownMediaUtils.readBytes(context, uri)
                        ?: return@withContext null
                    PendingSteerImage(
                        uri = uri,
                        bytes = bytes,
                        filename = MarkdownMediaUtils.guessFilename(context, uri),
                        mime = MarkdownMediaUtils.guessMimeType(context, uri),
                    )
                } ?: return@forEach
                viewModel.addPendingImage(picked.uri, picked.bytes, picked.filename, picked.mime)
            }
        }
    }

    LaunchedEffect(Unit) { viewModel.connectIfIdle() }
    // Returning from the background (EXP-243): the socket rarely survives it —
    // reconnect automatically instead of parking behind a manual button.
    LifecycleResumeEffect(Unit) {
        viewModel.reconnectNow()
        onPauseOrDispose { }
    }
    val sessionEnded = session?.status == DomainContract.codingSessionStatusEnded
    // A trailing question/plan means the session is blocked on a human — the
    // header flips to "Needs your input" so it never looks silently stuck.
    val awaitingInput = phase == AgentPhase.Live &&
        remember(feed) { activeQuestionIds(feed) }.isNotEmpty()
    // EXP-529 batch: while a plan-approval card awaits the human, the composer
    // IS the "tell Claude what to change" path (the desktop Escs the picker
    // and types the message) — its placeholder says so instead of offering a
    // dead picker row.
    val planAwaitingApproval = phase == AgentPhase.Live &&
        remember(feed, answerStates) {
            val active = activeQuestionIds(feed)
            feed.any { item ->
                item is AgentFeedItem.Question && item.planMode && !item.resolved &&
                    item.id in active &&
                    answerStates[questionLockKey(item)]?.locksCard() != true
            }
        }

    var diffSheetOpen by remember { mutableStateOf(false) }
    var killDialogOpen by remember { mutableStateOf(false) }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    SessionStatusTitle(
                        phase = phase,
                        deviceLabel = session?.deviceLabel,
                        awaitingInput = awaitingInput,
                    )
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(ExpIcons.uiBack, contentDescription = "Back")
                    }
                },
                actions = {
                    // Kill switch (EXP-268): only while the synced row is
                    // still live, for the session owner — everything about a
                    // live session is owner-only (EXP-312; server enforces
                    // too).
                    val row = session
                    if (row != null && !sessionEnded && row.userId == currentUserId) {
                        IconButton(onClick = { killDialogOpen = true }) {
                            Icon(
                                ExpIcons.codingStop,
                                contentDescription = "Kill session",
                                tint = MaterialTheme.colorScheme.error,
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                // consumeWindowInsets keeps imePadding from re-adding the
                // nav-bar inset already applied by the Scaffold padding —
                // without it the message box floats a nav-bar-height above
                // the keyboard (EXP-336).
                .consumeWindowInsets(padding)
                .fillMaxSize()
                .imePadding()
                .padding(horizontal = 12.dp),
        ) {
            // ── The activity feed (bottom-anchored, follow-scroll) ───────────
            Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
                when {
                    feed.isEmpty() && (phase == AgentPhase.Connecting || phase == AgentPhase.Starting) ->
                        CenteredState {
                            CircularProgressIndicator(
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Text(
                                if (phase == AgentPhase.Starting) {
                                    "The agent is starting. Waiting for the live stream…"
                                } else {
                                    "Connecting…"
                                },
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    feed.isEmpty() && phase == AgentPhase.Live && latestDiff == null ->
                        CenteredState {
                            Text(
                                "Waiting for activity…",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                            Text(
                                "Update the Exponential desktop app to see the live feed.",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                    // EXP-440: the feed renders markdown. Autolink is on — it
                    // is a render-only surface, so bare URLs may become links
                    // without diverging any stored bytes — and embedded images
                    // pre-size from the linked issue's probed attachments.
                    else -> CompositionLocalProvider(
                        LocalMarkdownAutolink provides true,
                        LocalAttachmentDims provides attachmentDims,
                    ) {
                        ActivityFeed(
                            feed = feed,
                            live = phase == AgentPhase.Live,
                            // EXP-389: the agent is actively working — live and
                            // nothing waiting on the user (no active question
                            // card, synced needs_input clear; all three agents
                            // drive the flag).
                            working = phase == AgentPhase.Live && !sessionEnded &&
                                !awaitingInput && session?.needsInput != true,
                            // Question cards are answerable while live (EXP-78;
                            // live implies ownership since EXP-312); the card
                            // itself also checks its own state.
                            answerEnabled = phase == AgentPhase.Live && !sessionEnded,
                            answerStates = answerStates,
                            // One place decides semantic vs legacy: a card with a
                            // wire id answers through the `answer` frame, one
                            // without falls back to raw keystrokes (EXP-249).
                            onAnswer = { question, keys, text ->
                                val wireId = question.wireId
                                if (wireId != null) {
                                    viewModel.sendQuestionAnswer(wireId, question.askId, keys, text)
                                } else {
                                    keys.forEach {
                                        viewModel.sendLegacyAnswer(
                                            questionLockKey(question),
                                            it,
                                            lock = !question.multiSelect,
                                        )
                                    }
                                }
                            },
                            onSubmit = viewModel::sendSubmit,
                        )
                    }
                }
            }

            // ── Status banners (feed retained above) ─────────────────────────
            when (val p = phase) {
                is AgentPhase.Ended -> BannerRow {
                    Text(
                        p.detail ?: "Session ended",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
                is AgentPhase.Closed -> BannerRow {
                    if (p.reconnecting) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(13.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    Text(
                        p.detail
                            ?: if (p.reconnecting) "Connection lost. Reconnecting…" else "Disconnected",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.weight(1f),
                    )
                }
                AgentPhase.Starting -> if (feed.isNotEmpty()) {
                    BannerRow {
                        CircularProgressIndicator(
                            modifier = Modifier.size(13.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "The agent is starting. Waiting for the live stream…",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                }
                else -> Unit
            }

            // A failed kill call (EXP-268) — success needs no banner: the
            // synced row flips to ended and the screen reacts on its own.
            val killFailure = killError
            if (killFailure != null) {
                BannerRow {
                    Text(
                        killFailure,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }

            // ── Pinned "Latest changes" chip (directly above the input bar) ──
            val diff = latestDiff
            if (diff != null) {
                val stats = remember(diff) { unifiedDiffStats(diff) }
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(vertical = 6.dp)
                        .glassRow()
                        .clickable { diffSheetOpen = true }
                        .padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Icon(
                        ExpIcons.codingDiff,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    Text(
                        "Latest changes",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                    )
                    Text(
                        "+${stats.additions}",
                        color = DiffAddColor,
                        fontFamily = FontFamily.Monospace,
                        style = MaterialTheme.typography.labelSmall,
                    )
                    Text(
                        "−${stats.deletions}",
                        color = DiffDelColor,
                        fontFamily = FontFamily.Monospace,
                        style = MaterialTheme.typography.labelSmall,
                    )
                    Icon(
                        ExpIcons.uiChevronUp,
                        contentDescription = "Show diff",
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }

            // A rejected pick or a failed image upload (EXP-511) — the text and
            // the thumbnails survive, so the send is retryable.
            val imageFailure = steerImageError
            if (imageFailure != null) {
                BannerRow {
                    Text(
                        imageFailure,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }

            // ── Steering input — fully seamless (EXP-312): no captions, no
            // operator state; live implies ownership, input just sends.
            val inputVisible = phase == AgentPhase.Live && !sessionEnded
            if (inputVisible) {
                if (pendingImages.isNotEmpty()) {
                    PendingImageStrip(
                        images = pendingImages,
                        enabled = !steerSending,
                        onRemove = viewModel::removePendingImage,
                    )
                }
                MessageInputRow(
                    canAttach = session?.issueId != null,
                    sending = steerSending,
                    hasPendingImages = pendingImages.isNotEmpty(),
                    planPending = planAwaitingApproval,
                    onPickImages = {
                        imagePicker.launch(
                            PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly),
                        )
                    },
                    onSend = viewModel::sendMessageWithImages,
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }

    if (diffSheetOpen && latestDiff != null) {
        UnifiedDiffPanel(
            diff = latestDiff!!,
            onDismiss = { diffSheetOpen = false },
        )
    }

    if (killDialogOpen) {
        AlertDialog(
            onDismissRequest = { killDialogOpen = false },
            title = { Text("Kill this coding session?") },
            text = {
                Text(
                    "This force-terminates the agent's terminal on the desktop " +
                        "and ends the session. It cannot be undone.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    killDialogOpen = false
                    viewModel.killSession()
                }) {
                    Text("Kill session", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { killDialogOpen = false }) { Text("Cancel") }
            },
        )
    }
}

// ── Header: status dot + "Live · <device>" ───────────────────────────────────

@Composable
private fun SessionStatusTitle(
    phase: AgentPhase,
    deviceLabel: String?,
    /** Live but blocked on a trailing question/plan — waiting for a human
     *  answer, not stuck (EXP-97). */
    awaitingInput: Boolean = false,
) {
    // Auto-reconnecting after a drop reads as connecting (EXP-243).
    val connecting = phase == AgentPhase.Connecting || phase == AgentPhase.Starting ||
        (phase is AgentPhase.Closed && phase.reconnecting)
    val awaiting = phase == AgentPhase.Live && awaitingInput
    val dotColor = when {
        phase == AgentPhase.Live -> if (awaiting) ConnectingYellow else LiveGreen
        connecting -> ConnectingYellow
        else -> LostGray
    }
    // The connecting dot pulses; live/ended dots hold steady.
    val pulse by rememberInfiniteTransition(label = "dot").animateFloat(
        initialValue = 1f,
        targetValue = 0.35f,
        animationSpec = infiniteRepeatable(tween(650), RepeatMode.Reverse),
        label = "dotAlpha",
    )
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            modifier = Modifier
                .size(8.dp)
                .alpha(if (connecting) pulse else 1f)
                .background(dotColor, CircleShape),
        )
        Text(
            when (phase) {
                AgentPhase.Live -> {
                    val prefix = if (awaiting) "Needs your input" else "Live"
                    val label = deviceLabel?.takeIf { it.isNotBlank() }
                    if (label != null) "$prefix · $label" else prefix
                }
                AgentPhase.Connecting, AgentPhase.Starting, AgentPhase.Idle -> "Connecting…"
                is AgentPhase.Ended -> "Session ended"
                is AgentPhase.Closed -> if (phase.reconnecting) "Reconnecting…" else "Disconnected"
            },
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// ── The feed ─────────────────────────────────────────────────────────────────

@Composable
private fun ActivityFeed(
    feed: List<AgentFeedItem>,
    live: Boolean,
    /** EXP-389: show the trailing "Working…" indicator — the session is live
     *  and nothing waits on the user. */
    working: Boolean,
    answerEnabled: Boolean,
    answerStates: Map<String, AnswerState>,
    /** (question, keys, text) — the option keys chosen on that card (a
     *  multi-select step sends all of them at once); `text` is the typed
     *  reply for a `freeText` option (EXP-513), else null. */
    onAnswer: (AgentFeedItem.Question, List<String>, String?) -> Unit,
    /** Advances a LEGACY multi-select picker (Tab) — semantic cards submit
     *  through [onAnswer] instead. */
    onSubmit: () -> Unit,
) {
    val listState = rememberLazyListState()
    var follow by remember { mutableStateOf(true) }
    // A card with a wire id stays answerable until it resolves; a legacy card
    // falls back to the trailing-run heuristic (EXP-78/EXP-174).
    val activeQuestionIds = remember(feed) { activeQuestionIds(feed) }
    // Subagent groups, askId steppers and consecutive tool runs are all
    // render-time projections only — the flat feed stays the state.
    val rows = remember(feed) { groupFeedRows(feed) }
    // EXP-356: conversation tabs — null is the main agent; a subagent id
    // focuses that agent's stream. Falls back to Main whenever the id
    // vanishes from the feed (an activity_reset replay). EXP-387: the strip
    // only shows the still-running subagents (plus the focused tab).
    var agentTab by remember { mutableStateOf<String?>(null) }
    val agents = remember(feed) { collectSubagents(feed) }
    val visibleTabs = remember(agents, agentTab) { visibleSubagentTabs(agents, agentTab) }
    val focused = visibleTabs.firstOrNull { it.subagentId == agentTab }
    // A HOLDING lock counts as answered for stepper advance — a Sending lock
    // advances the stepper the moment the tap goes out (claude-TUI-snappy; web
    // parity). A Failed lock (5s no-ack timeout, EXP-334) does NOT: its step
    // re-surfaces with a retry hint.
    val answered = remember(answerStates) {
        answerStates.filterValues { it.locksCard() }.keys
    }

    // Only user drags flip follow-mode; programmatic scrolls keep it.
    // EXP-529 batch: "at the bottom" carries ~96dp of slack — the pixel-exact
    // canScrollForward check dropped follow-mode (and surfaced the "Jump to
    // bottom" chip) while the list sat a few px shy of the true bottom, which
    // visually IS the bottom. Near-bottom counts as bottom, so the chip only
    // appears after a real scroll-up and hides again within the same slack.
    val followSlackPx = with(LocalDensity.current) { 96.dp.toPx() }
    LaunchedEffect(listState, followSlackPx) {
        snapshotFlow { listState.isScrollInProgress to listState.isNearBottom(followSlackPx) }
            .distinctUntilChanged()
            .collect { (dragging, nearBottom) ->
                if (dragging) follow = nearBottom
            }
    }
    // Keyed on feed.size (not rows.size) — a growing trailing tool run adds
    // feed items without adding rows. scrollToItem alone lands on the last
    // item's TOP, which for a long message is nowhere near the end — the
    // scrollBy finishes the scroll to the true bottom (EXP-197). agentTab is a
    // key too: switching conversations re-pins to the newest event (EXP-356),
    // and `working` re-pins when the EXP-389 footer appears/disappears.
    LaunchedEffect(feed.size, follow, agentTab, working) {
        val visible = if (focused != null) {
            focused.tools.size + 1
        } else {
            rows.size + (if (working) 1 else 0)
        }
        if (follow && visible > 0) {
            listState.scrollToItem(visible - 1)
            listState.scrollBy(1_000_000f)
        }
    }

    // Only composed once real activity has arrived (the placeholder states are
    // siblings) — the store-screenshot test waits on this tag so it never
    // captures "Connecting…" / "Waiting for activity…".
    Column(modifier = Modifier.fillMaxSize().testTag("agent-feed")) {
        if (visibleTabs.isNotEmpty()) {
            AgentTabStrip(
                agents = visibleTabs,
                selected = focused?.subagentId,
                onSelect = { id ->
                    agentTab = id
                    follow = true
                },
            )
        }
        Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxSize(),
            // Bottom-anchored: a short feed sits above the input bar, not at
            // the top of the screen.
            verticalArrangement = Arrangement.Bottom,
            contentPadding = PaddingValues(vertical = 8.dp),
        ) {
            if (focused != null) {
                // EXP-356: the focused subagent's conversation — its
                // delegation summary, then every tool call as a full row.
                item(key = "agent-summary") {
                    SubagentGroupRow(run = focused.copy(tools = emptyList()), liveTail = false)
                }
                if (focused.tools.isEmpty()) {
                    item(key = "agent-empty") {
                        Text(
                            "No tool calls yet.",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Tertiary,
                            ),
                            modifier = Modifier.padding(vertical = 4.dp),
                        )
                    }
                } else {
                    items(focused.tools, key = { it.id }) { tool ->
                        ToolRow(tool.name, tool.detail)
                    }
                }
            } else {
            items(rows, key = { it.id }) { row ->
                when (row) {
                    is AgentFeedRow.ToolRun -> ToolGroupRow(
                        items = row.items,
                        liveTail = live && row.id == rows.last().id,
                    )
                    is AgentFeedRow.SubagentRun -> SubagentGroupRow(
                        run = row,
                        liveTail = live && row.id == rows.last().id,
                    )
                    is AgentFeedRow.QuestionStepper -> QuestionStepperCard(
                        steps = row.steps,
                        answered = answered,
                        activeQuestionIds = activeQuestionIds,
                        answerEnabled = answerEnabled,
                        answerStates = answerStates,
                        onAnswer = onAnswer,
                        onSubmit = onSubmit,
                    )
                    is AgentFeedRow.Single -> when (val item = row.item) {
                        is AgentFeedItem.Narration -> NarrationBubble(item.text)
                        is AgentFeedItem.Tool -> ToolRow(item.name, item.detail)
                        is AgentFeedItem.UserMessage -> UserMessageBubble(item.text)
                        is AgentFeedItem.Permission -> PermissionRow(
                            tool = item.tool,
                            detail = item.detail,
                            // The reply-to-continue hint only while the prompt
                            // is plausibly still up: live session, trailing row.
                            showHint = live && row.id == rows.last().id,
                        )
                        // Unreachable in practice — groupFeedRows folds every
                        // subagent marker into a SubagentRun — kept for `when`
                        // exhaustiveness.
                        is AgentFeedItem.Subagent -> SubagentGroupRow(
                            run = AgentFeedRow.SubagentRun(
                                id = item.id,
                                subagentId = item.subagentId,
                                agentType = item.agentType,
                                completed = item.completed,
                                detail = item.detail,
                                tools = emptyList(),
                            ),
                            liveTail = false,
                        )
                        is AgentFeedItem.Question -> QuestionCard(
                            item = item,
                            active = item.id in activeQuestionIds,
                            answerEnabled = answerEnabled,
                            state = answerStates[questionLockKey(item)],
                            stepLabel = null,
                            onAnswer = { keys, text -> onAnswer(item, keys, text) },
                            onSubmit = onSubmit,
                        )
                    }
                }
            }
            // EXP-389: the agent-is-busy footer under the newest event (iOS
            // parity) — main conversation only, subagent chips carry their
            // own spinner.
            if (working) {
                item(key = "working-indicator") { WorkingIndicatorRow() }
            }
            }
        }
        if (!follow) {
            Text(
                "Jump to bottom ↓",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 8.dp)
                    // opaque: the feed scrolls beneath this pill (EXP-165).
                    .glassButton(active = true, opaque = true)
                    .clickable { follow = true }
                    .padding(horizontal = 14.dp, vertical = 8.dp),
            )
        }
        }
    }
}

/** Visually-at-the-bottom, with slack: the last item's bottom edge sits within
 *  [slackPx] of the viewport end (an empty list counts as bottom). A last item
 *  taller than the slack correctly reads as NOT near while its tail is off
 *  screen. */
private fun LazyListState.isNearBottom(slackPx: Float): Boolean {
    val info = layoutInfo
    val last = info.visibleItemsInfo.lastOrNull() ?: return true
    if (last.index < info.totalItemsCount - 1) return false
    return last.offset + last.size - info.viewportEndOffset <= slackPx
}

/** EXP-356: conversation tabs — Main plus one chip per RUNNING subagent
 *  (ended tabs are dropped, EXP-387), labeled with the run's real agent type
 *  and a spinner while it works. */
@Composable
private fun AgentTabStrip(
    agents: List<AgentFeedRow.SubagentRun>,
    selected: String?,
    onSelect: (String?) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 14.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AgentTabChip(label = "Main", running = false, selected = selected == null) {
            onSelect(null)
        }
        agents.forEach { run ->
            AgentTabChip(
                label = run.agentType,
                running = !run.completed,
                selected = selected == run.subagentId,
            ) {
                onSelect(run.subagentId)
            }
        }
    }
}

@Composable
private fun AgentTabChip(
    label: String,
    running: Boolean,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .glassButton(active = selected)
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (selected) TextEmphasis.Primary else TextEmphasis.Secondary,
            ),
            maxLines = 1,
        )
        if (running) {
            CircularProgressIndicator(
                modifier = Modifier.size(10.dp),
                strokeWidth = 1.5.dp,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
    }
}

// Assistant prose — a small glyph + plain full-width selectable text. EXP-274
// dropped the glass speech bubble: agent output is the feed's bulk, and the
// bubble insets cost real width on a phone.
@Composable
private fun NarrationBubble(text: String) {
    // MarkdownView renders nothing for blank input — without this the icon
    // would be left behind as an orphan row.
    if (text.isBlank()) return
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.codingAssistant,
            contentDescription = null,
            modifier = Modifier.size(13.dp).padding(top = 1.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        // EXP-440: narration is GFM — the agents emit lists, code fences,
        // links and embedded images. Never folded: it is the feed's
        // conversation. Bare URLs stay tappable (EXP-430) through the
        // feed's LocalMarkdownAutolink. MarkdownView brings its own
        // SelectionContainer (EXP-534).
        MarkdownView(text, softBreaksAsNewlines = true, modifier = Modifier.weight(1f))
    }
}

/** The trailing "agent is busy" row (EXP-389): a gently pulsing "Working…"
 *  under the newest event whenever the session is live and nothing waits on
 *  the user — without it a feed that ends in tool rows gives no cue whether
 *  the agent is still going. */
@Composable
private fun WorkingIndicatorRow() {
    val pulse by rememberInfiniteTransition(label = "working").animateFloat(
        initialValue = 1f,
        targetValue = 0.4f,
        animationSpec = infiniteRepeatable(tween(900), RepeatMode.Reverse),
        label = "workingAlpha",
    )
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp)
            .alpha(pulse),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.codingAssistant,
            contentDescription = null,
            modifier = Modifier.size(13.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Text(
            "Working…",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/** Fold threshold for user/question text — the initial prompt can be 16 KiB. */
private const val CLAMP_LINES = 6
private const val CLAMP_CHARS = 600

private fun clampable(text: String): Boolean =
    text.length > CLAMP_CHARS || text.count { it == '\n' } >= CLAMP_LINES

@Composable
private fun ShowMoreToggle(expanded: Boolean, onToggle: () -> Unit) {
    Text(
        if (expanded) "Show less" else "Show more",
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier
            .clickable(onClick = onToggle)
            .padding(top = 2.dp),
    )
}

/** Markdown body folded behind Show more (EXP-440): maxLines can't clamp a block
 *  Column, so the fold is a height clamp + clip sized to CLAMP_LINES body lines;
 *  the fold decision stays the source-text heuristic. */
@Composable
private fun FoldableMarkdown(
    text: String,
    expanded: Boolean,
    softBreaks: Boolean = false,
    onToggle: () -> Unit,
) {
    val folds = remember(text) { clampable(text) }
    val clampHeight = with(LocalDensity.current) { (MdStyle.lineHeight * CLAMP_LINES).toDp() }
    // MarkdownView brings its own SelectionContainer (EXP-534).
    MarkdownView(
        text,
        modifier = if (folds && !expanded) {
            Modifier.heightIn(max = clampHeight).clipToBounds()
        } else {
            Modifier
        },
        softBreaksAsNewlines = softBreaks,
    )
    if (folds) {
        ShowMoreToggle(expanded, onToggle)
    }
}

// A human turn (EXP-78): the initial prompt or a steered message — rendered
// end-aligned like the sender's own chat bubble, long text folded.
@Composable
private fun UserMessageBubble(text: String) {
    var expanded by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        horizontalArrangement = Arrangement.End,
    ) {
        Spacer(Modifier.width(32.dp))
        Column(
            modifier = Modifier
                // Slightly brighter than the assistant's glass sections — the
                // sender's own bubble (matches the composer's active tint).
                .background(GlassTokens.RowFillActive, RoundedCornerShape(12.dp))
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            FoldableMarkdown(text, expanded, softBreaks = true) { expanded = !expanded }
        }
    }
}

// One multi-question ask (EXP-249): a claude-style stepper that shows ONE step
// at a time — "Question 2 of 3" — and advances the moment a step's answer is
// sent (web parity; the 5s no-ack timeout rolls it back), ending on the ask's
// final submit step. Once every step is answered the card collapses into the
// answered summary.
@Composable
private fun QuestionStepperCard(
    steps: List<AgentFeedItem.Question>,
    /** Lock keys of the steps whose answer is out (sent or acknowledged). */
    answered: Set<String>,
    activeQuestionIds: Set<Long>,
    answerEnabled: Boolean,
    answerStates: Map<String, AnswerState>,
    onAnswer: (AgentFeedItem.Question, List<String>, String?) -> Unit,
    onSubmit: () -> Unit,
) {
    val current = remember(steps, answered) { currentStepperStep(steps, answered) }
    if (current == null) {
        AnsweredAskCard(steps)
        return
    }
    val total = current.total ?: steps.count { it.index != null }
    QuestionCard(
        item = current,
        active = current.id in activeQuestionIds,
        answerEnabled = answerEnabled,
        state = answerStates[questionLockKey(current)],
        stepLabel = when {
            current.index != null && total > 0 -> "Question ${current.index} of $total"
            // No index: the ask's final review step.
            else -> "Review your answers"
        },
        onAnswer = { keys, text -> onAnswer(current, keys, text) },
        onSubmit = onSubmit,
    )
}

// Every step of a fully answered ask, each with the answer it got.
@Composable
private fun AnsweredAskCard(steps: List<AgentFeedItem.Question>) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.uiHelp,
            contentDescription = null,
            modifier = Modifier.size(13.dp).padding(top = 1.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Column(
            modifier = Modifier
                .weight(1f)
                .glassSection()
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            steps.forEach { step ->
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(
                        step.header?.takeIf { it.isNotBlank() } ?: step.text,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    AnsweredRow(step.answer)
                }
            }
            // Every step is answered but the ask hasn't resolved yet — the
            // agent is still working through it.
            if (steps.none { it.resolved }) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(12.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        "Waiting for the agent…",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
        }
    }
}

// An interactive question (EXP-78): AskUserQuestion step / plan approval. While
// the card is answerable, option rows send their keys — semantically for cards
// carrying a wire id, as raw TUI keystrokes for older desktops; stale/view-only
// cards render options as plain rows. A tapped card locks IMMEDIATELY (EXP-249)
// and stays locked through the desktop's answer_ack, so an answer can never be
// double-sent. planMode cards (EXP-97) get a dedicated "Plan ready"
// presentation with the first option as the primary approve action and the plan
// rendered as markdown — labels/keys always come from the wire options.
@Composable
private fun QuestionCard(
    item: AgentFeedItem.Question,
    /** Still answerable per the feed — the session is blocked on this card. */
    active: Boolean,
    /** Live (and not ended) — whether this client may answer at all. */
    answerEnabled: Boolean,
    /** This client's send state — non-null means the card is locked. */
    state: AnswerState?,
    /** "Question 2 of 3" when the card is one step of a stepper. */
    stepLabel: String?,
    onAnswer: (List<String>, String?) -> Unit,
    onSubmit: () -> Unit,
) {
    // Both keyed on the card id: the stepper reuses ONE card slot across the
    // ask's steps, so an unkeyed `expanded` leaked a "Show more" from a long
    // step onto the next one (EXP-274).
    var expanded by remember(item.id) { mutableStateOf(false) }
    var picked by remember(item.id) { mutableStateOf(emptySet<String>()) }
    // EXP-513: the freeText option whose inline input is open (its key).
    var freeTextKey by remember(item.id) { mutableStateOf<String?>(null) }
    var freeTextValue by remember(item.id) { mutableStateOf("") }
    // A Failed state does NOT lock (EXP-334) — the card is answerable again
    // and renders the retry hint below instead of the sent row.
    val locked = state.locksCard()
    val answerable = active && answerEnabled && !locked
    val semantic = item.wireId != null
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            if (item.planMode) ExpIcons.codingPlan else ExpIcons.uiHelp,
            contentDescription = null,
            modifier = Modifier.size(13.dp).padding(top = 1.dp),
            tint = if (item.planMode) PlanAccent else ConnectingYellow,
        )
        Column(
            modifier = Modifier
                .weight(1f)
                .glassSection()
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (stepLabel != null) {
                Text(
                    stepLabel,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
            if (item.planMode) {
                Text(
                    "Plan ready",
                    style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                    color = PlanAccent,
                )
                // The plan is GFM markdown — always fully rendered, never
                // folded behind a Show more (EXP-197).
                MarkdownView(item.text)
            } else {
                item.header?.takeIf { it.isNotBlank() }?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
                FoldableMarkdown(item.text, expanded) { expanded = !expanded }
            }
            if (item.resolved) {
                // Resolved (EXP-197/EXP-249): the answer replaces the options.
                AnsweredRow(item.answer)
            } else {
                item.options.forEachIndexed { index, option ->
                    // The wire's first option of a plan is the primary approve
                    // action ("Approve — auto-accept edits") — promote it.
                    val primary = item.planMode && index == 0
                    val selected = option.key in picked || freeTextKey == option.key
                    val interactive = answerable || locked
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .then(
                                if (interactive) {
                                    Modifier
                                        .alpha(if (locked) 0.5f else 1f)
                                        // glassRow, not glassButton: the
                                        // capsule's percent radius clipped
                                        // multi-line option descriptions into
                                        // an ellipse (EXP-274).
                                        .glassRow(active = primary || selected)
                                } else {
                                    Modifier
                                },
                            )
                            .then(
                                if (answerable) {
                                    Modifier.clickable {
                                        if (item.multiSelect) {
                                            picked = if (selected) picked - option.key
                                            else picked + option.key
                                            // A legacy picker toggles with the
                                            // raw digit as you tap; a semantic
                                            // one submits every key at once.
                                            if (!semantic) onAnswer(listOf(option.key), null)
                                        } else if (option.freeText && semantic) {
                                            // EXP-513: collect the reply first —
                                            // nothing is sent until it submits.
                                            freeTextKey =
                                                if (freeTextKey == option.key) null else option.key
                                        } else {
                                            picked = setOf(option.key)
                                            onAnswer(listOf(option.key), null)
                                        }
                                    }
                                } else {
                                    Modifier
                                },
                            )
                            .padding(
                                horizontal = if (interactive) 10.dp else 0.dp,
                                vertical = 6.dp,
                            ),
                        verticalAlignment = Alignment.Top,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        QuestionOptionLabel(
                            option = option,
                            showKey = !item.planMode && !semantic,
                            checked = if (item.multiSelect) selected else null,
                        )
                    }
                }
                if (answerable && freeTextKey != null) {
                    // EXP-513: the inline reply for the selected freeText row.
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        TextField(
                            value = freeTextValue,
                            onValueChange = { freeTextValue = it.take(4000) },
                            modifier = Modifier.weight(1f),
                            placeholder = {
                                Text(
                                    "Type your answer…",
                                    color = MaterialTheme.colorScheme.onSurface
                                        .copy(alpha = TextEmphasis.Tertiary),
                                )
                            },
                            maxLines = 3,
                            shape = RoundedCornerShape(12.dp),
                            colors = TextFieldDefaults.colors(
                                focusedContainerColor = GlassTokens.RowFill,
                                unfocusedContainerColor = GlassTokens.RowFill,
                                disabledContainerColor = GlassTokens.RowFill,
                                focusedIndicatorColor = Color.Transparent,
                                unfocusedIndicatorColor = Color.Transparent,
                                disabledIndicatorColor = Color.Transparent,
                            ),
                        )
                        val canSend = freeTextValue.isNotBlank()
                        IconButton(
                            onClick = {
                                val key = freeTextKey
                                val text = freeTextValue.trim()
                                if (key != null && text.isNotEmpty()) {
                                    picked = setOf(key)
                                    onAnswer(listOf(key), text)
                                    freeTextKey = null
                                    freeTextValue = ""
                                }
                            },
                            enabled = canSend,
                        ) {
                            Icon(
                                ExpIcons.uiSend,
                                contentDescription = "Send answer",
                                tint = if (canSend) {
                                    MaterialTheme.colorScheme.onSurface
                                } else {
                                    MaterialTheme.colorScheme.onSurface
                                        .copy(alpha = TextEmphasis.Quaternary)
                                },
                            )
                        }
                    }
                }
                if (item.multiSelect && (answerable || locked)) {
                    // Semantic: one frame carrying every picked key. Legacy:
                    // Tab, which advances the TUI picker to its next step.
                    val enabled = answerable && (!semantic || picked.isNotEmpty())
                    Text(
                        if (semantic) "Submit" else "Continue",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .alpha(if (enabled) 1f else 0.5f)
                            .glassButton(active = true)
                            .clickable(enabled = enabled) {
                                if (semantic) onAnswer(picked.toList(), null) else onSubmit()
                            }
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                    )
                }
            }
            if (locked) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    if (state == AnswerState.Sending) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(12.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    } else {
                        Icon(
                            ExpIcons.uiCheck,
                            contentDescription = null,
                            modifier = Modifier.size(12.dp),
                            tint = LiveGreen,
                        )
                    }
                    Text(
                        if (state == AnswerState.Sending) {
                            "Sending your answer…"
                        } else {
                            "Answer sent. Waiting for the agent."
                        },
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            } else if (state == AnswerState.Failed && answerable) {
                // The optimistic lock expired with no `answer_ack` — say WHY
                // the step re-surfaced instead of silently rolling back
                // (EXP-334, web parity).
                Text(
                    "No confirmation from the desktop. Pick again to retry.",
                    style = MaterialTheme.typography.labelSmall,
                    color = ConnectingYellow,
                )
            } else if (active && !answerEnabled) {
                Text(
                    if (item.planMode) {
                        "Waiting for approval. You're viewing read-only."
                    } else {
                        "Waiting for an answer. You're viewing read-only."
                    },
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

// The chosen answer of a resolved card — a dismissed ask carries none.
@Composable
private fun AnsweredRow(answer: String?) {
    Row(
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            ExpIcons.uiCheck,
            contentDescription = null,
            modifier = Modifier.size(13.dp).padding(top = 1.dp),
            tint = LiveGreen,
        )
        SelectionContainer {
            Text(
                answer?.takeIf { it.isNotBlank() } ?: "Answered",
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
    }
}

@Composable
private fun RowScope.QuestionOptionLabel(
    option: QuestionOption,
    showKey: Boolean = true,
    /** Non-null on a multi-select option — renders its checkbox state. */
    checked: Boolean? = null,
) {
    if (checked != null) {
        Icon(
            if (checked) ExpIcons.uiSelected else ExpIcons.uiUnselected,
            contentDescription = null,
            modifier = Modifier.size(14.dp).padding(top = 1.dp),
            tint = if (checked) {
                MaterialTheme.colorScheme.onSurface
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            },
        )
    } else if (showKey) {
        Text(
            option.key,
            style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
    Column(
        modifier = Modifier.weight(1f),
        verticalArrangement = Arrangement.spacedBy(1.dp),
    ) {
        Text(
            option.label,
            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface,
        )
        option.description?.takeIf { it.isNotBlank() }?.let {
            Text(
                it,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

// A permission prompt the agent hit (EXP-249) — the card itself has nothing to
// press (the desktop TUI owns the decision), but a reply typed below reaches
// the same prompt, so say so instead of dead-ending the viewer (EXP-529).
@Composable
private fun PermissionRow(tool: String, detail: String?, showHint: Boolean) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                ExpIcons.uiPrivate,
                contentDescription = null,
                modifier = Modifier.size(12.dp),
                tint = ConnectingYellow,
            )
            Text(
                "Permission · $tool",
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (!detail.isNullOrBlank()) {
                Text(
                    remember(detail) { middleTruncate(detail) },
                    style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        if (showHint) {
            Text(
                "Approve on the desktop, or reply below to continue.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(start = 20.dp),
            )
        }
    }
}

// A subagent and the tool calls it made (EXP-249), collapsed into one group.
// The header always shows the agent type, a check/spinner, and the delegation
// detail; expansion only reveals the tool calls — a run with none renders as a
// static row with no chevron (EXP-350: the chevron used to expand to nothing).
// While the group is the trailing row of a live session its latest call stays
// visible so the viewer still sees progress.
@Composable
private fun SubagentGroupRow(
    run: AgentFeedRow.SubagentRun,
    liveTail: Boolean,
) {
    var expanded by remember { mutableStateOf(false) }
    val expandable = run.tools.isNotEmpty()
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .let { if (expandable) it.clickable { expanded = !expanded } else it }
                .padding(vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (expandable) {
                Icon(
                    if (expanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
                    contentDescription = if (expanded) "Collapse" else "Expand",
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
            Icon(
                ExpIcons.codingSubagent,
                contentDescription = null,
                modifier = Modifier.size(12.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Text(
                run.agentType,
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (run.completed) {
                Icon(
                    ExpIcons.uiCheck,
                    contentDescription = null,
                    modifier = Modifier.size(12.dp),
                    tint = LiveGreen,
                )
            } else {
                CircularProgressIndicator(
                    modifier = Modifier.size(11.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
            if (run.tools.isNotEmpty()) {
                Text(
                    "${run.tools.size} tool calls",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        if (!run.detail.isNullOrBlank()) {
            Text(
                run.detail,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(start = 22.dp),
            )
        }
        when {
            expanded -> Column(modifier = Modifier.padding(start = 22.dp)) {
                run.tools.forEach { ToolRow(it.name, it.detail) }
            }
            liveTail && run.tools.isNotEmpty() -> Column(modifier = Modifier.padding(start = 22.dp)) {
                val latest = run.tools.last()
                ToolRow(latest.name, latest.detail)
            }
        }
    }
}

// Tool-call headline — compact single line, consecutive rows visually tight.
@Composable
private fun ToolRow(name: String, detail: String?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.codingTool,
            contentDescription = null,
            modifier = Modifier.size(12.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Text(
            name,
            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface,
        )
        if (!detail.isNullOrBlank()) {
            Text(
                remember(detail) { middleTruncate(detail) },
                style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

// A run of ≥2 consecutive tool calls collapsed into one "N tool calls" row
// (EXP-97), expandable to the individual rows. While the run is the trailing
// row of a live session, the latest call stays visible under the count so the
// viewer still sees live progress.
@Composable
private fun ToolGroupRow(items: List<AgentFeedItem.Tool>, liveTail: Boolean) {
    var expanded by remember { mutableStateOf(false) }
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable { expanded = !expanded }
                .padding(vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                if (expanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
                contentDescription = if (expanded) "Collapse" else "Expand",
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Icon(
                ExpIcons.codingTool,
                contentDescription = null,
                modifier = Modifier.size(12.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Text(
                "${items.size} tool calls",
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        when {
            expanded -> Column(modifier = Modifier.padding(start = 22.dp)) {
                items.forEach { ToolRow(it.name, it.detail) }
            }
            liveTail -> Column(modifier = Modifier.padding(start = 22.dp)) {
                val latest = items.last()
                ToolRow(latest.name, latest.detail)
            }
        }
    }
}

// Middle-truncate a tool detail (paths etc.) so head AND tail stay readable.
// (TextOverflow.MiddleEllipsis needs a newer Compose than the pinned BOM.)
private fun middleTruncate(s: String, max: Int = 72): String {
    if (s.length <= max) return s
    val head = max * 2 / 3
    val tail = max - head - 1
    return s.take(head) + "…" + s.takeLast(tail)
}

// ── Steering input ───────────────────────────────────────────────────────────

/** Thumbnails of the images attached to the next message (EXP-511). */
@Composable
private fun PendingImageStrip(
    images: List<PendingSteerImage>,
    enabled: Boolean,
    onRemove: (Int) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(bottom = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        images.forEachIndexed { index, image ->
            val bitmap = remember(image.uri, image.bytes) {
                BitmapFactory.decodeByteArray(image.bytes, 0, image.bytes.size)?.asImageBitmap()
            }
            Box(modifier = Modifier.size(64.dp)) {
                if (bitmap != null) {
                    Image(
                        bitmap = bitmap,
                        contentDescription = null,
                        contentScale = ContentScale.Crop,
                        modifier = Modifier
                            .fillMaxSize()
                            .clip(RoundedCornerShape(8.dp)),
                    )
                } else {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .clip(RoundedCornerShape(8.dp))
                            .background(GlassTokens.RowFill),
                    )
                }
                Box(
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .padding(2.dp)
                        .size(18.dp)
                        .clip(CircleShape)
                        .background(Color.Black.copy(alpha = 0.55f))
                        .clickable(enabled = enabled) { onRemove(index) },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        ExpIcons.uiClose,
                        contentDescription = "Remove image",
                        modifier = Modifier.size(12.dp),
                        tint = Color.White,
                    )
                }
            }
        }
    }
}

@Composable
private fun MessageInputRow(
    canAttach: Boolean,
    sending: Boolean,
    hasPendingImages: Boolean,
    /** A plan-approval card is awaiting the human — the composer doubles as
     *  the "tell Claude what to change" path (EXP-529 batch). */
    planPending: Boolean,
    onPickImages: () -> Unit,
    onSend: (String) -> Unit,
) {
    var field by remember { mutableStateOf("") }
    val canSend = (field.isNotBlank() || hasPendingImages) && !sending
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.Bottom,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (canAttach) {
            IconButton(onClick = onPickImages, enabled = !sending) {
                Icon(
                    ExpIcons.uiAdd,
                    contentDescription = "Attach image",
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
        TextField(
            value = field,
            onValueChange = { field = it },
            modifier = Modifier.weight(1f),
            placeholder = {
                Text(
                    if (planPending) "Tell Claude what to change…" else "Message the agent…",
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            },
            maxLines = 4,
            shape = RoundedCornerShape(12.dp),
            colors = TextFieldDefaults.colors(
                focusedContainerColor = GlassTokens.RowFill,
                unfocusedContainerColor = GlassTokens.RowFill,
                disabledContainerColor = GlassTokens.RowFill,
                focusedIndicatorColor = Color.Transparent,
                unfocusedIndicatorColor = Color.Transparent,
                disabledIndicatorColor = Color.Transparent,
            ),
        )
        IconButton(
            onClick = {
                if (canSend) {
                    onSend(field)
                    field = ""
                }
            },
            enabled = canSend,
        ) {
            if (sending) {
                CircularProgressIndicator(
                    modifier = Modifier.size(16.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            } else {
                Icon(
                    ExpIcons.uiSend,
                    contentDescription = "Send",
                    tint = if (canSend) {
                        MaterialTheme.colorScheme.onSurface
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
                    },
                )
            }
        }
    }
}

// ── Misc rows ────────────────────────────────────────────────────────────────

@Composable
private fun BannerRow(content: @Composable RowScope.() -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}

@Composable
private fun CenteredState(content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
        content = content,
    )
}

// ── Latest-changes diff sheet ────────────────────────────────────────────────

// Renders the latest worktree diff (raw `git diff` output): split on
// `diff --git` into per-file sections with the shared +/−/@@ coloring;
// horizontal scrolling lives inside each file's code block only.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun UnifiedDiffPanel(diff: String, onDismiss: () -> Unit) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val sections = remember(diff) { splitUnifiedDiff(diff) }
    val stats = remember(diff) { unifiedDiffStats(diff) }
    val contextColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = GlassTokens.BackgroundBottom,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.padding(bottom = 10.dp),
            ) {
                Text(
                    "Latest changes",
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    "+${stats.additions}",
                    color = DiffAddColor,
                    fontFamily = FontFamily.Monospace,
                    style = MaterialTheme.typography.labelSmall,
                )
                Text(
                    "−${stats.deletions}",
                    color = DiffDelColor,
                    fontFamily = FontFamily.Monospace,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                contentPadding = PaddingValues(bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                items(sections.size, key = { it }) { index ->
                    val section = sections[index]
                    Column(modifier = Modifier.fillMaxWidth().glassSection()) {
                        if (section.filename.isNotBlank()) {
                            Text(
                                section.filename,
                                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                                color = MaterialTheme.colorScheme.onSurface,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                            )
                        }
                        PatchLines(
                            lines = section.lines,
                            contextColor = contextColor,
                            modifier = Modifier.padding(bottom = 8.dp),
                        )
                    }
                }
            }
        }
    }
}
