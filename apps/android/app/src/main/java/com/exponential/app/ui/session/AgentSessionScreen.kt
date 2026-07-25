package com.exponential.app.ui.session

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.HelpOutline
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.AccountTree
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Checklist
import androidx.compose.material.icons.filled.Difference
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.RadioButtonUnchecked
import androidx.compose.material.icons.filled.StopCircle
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.issue.PatchLines
import com.exponential.app.ui.issue.splitUnifiedDiff
import com.exponential.app.ui.issue.unifiedDiffStats
import com.exponential.app.ui.issue.DiffAddColor
import com.exponential.app.ui.issue.DiffDelColor
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.glassSection
import kotlinx.coroutines.flow.distinctUntilChanged

// The "Agent session" screen (EXP-32) — a chat-style view of a live coding
// session over the relay's scrubbed activity channel. NO terminal rendering:
// narration bubbles, compact tool rows, collapsible subagent groups, question
// steppers, a pinned "Latest changes" diff chip above the input bar, and
// message-shaped steering (steal-claim + text + \r).
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
    val steererId by viewModel.steererId.collectAsStateWithLifecycle()
    val perm by viewModel.perm.collectAsStateWithLifecycle()
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val killError by viewModel.killError.collectAsStateWithLifecycle()
    val answerStates = activity.answerLocks

    LaunchedEffect(Unit) { viewModel.connectIfIdle() }
    // Returning from the background (EXP-243): the socket rarely survives it —
    // reconnect automatically instead of parking behind a manual button.
    LifecycleResumeEffect(Unit) {
        viewModel.reconnectNow()
        onPauseOrDispose { }
    }
    // Auto-release the steer claim when the screen goes away (best-effort;
    // closing the socket releases it relay-side anyway).
    DisposableEffect(Unit) {
        onDispose { viewModel.releaseNow() }
    }

    val steering = steererId != null && steererId == currentUserId
    val sessionEnded = session?.status == DomainContract.codingSessionStatusEnded
    // A trailing question/plan means the session is blocked on a human — the
    // header flips to "Needs your input" so it never looks silently stuck.
    val awaitingInput = phase == AgentPhase.Live &&
        remember(feed) { activeQuestionIds(feed) }.isNotEmpty()

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
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    // Kill switch (EXP-268): only while the synced row is still
                    // live, for the session owner or a steer-perm viewer (team
                    // owners — mirrors the server's owner-or-team-owner gate).
                    val row = session
                    if (row != null && !sessionEnded &&
                        (perm == "steer" || row.userId == currentUserId)
                    ) {
                        IconButton(onClick = { killDialogOpen = true }) {
                            Icon(
                                Icons.Filled.StopCircle,
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
                                    "The agent is starting — waiting for the live stream…"
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
                    else -> ActivityFeed(
                        feed = feed,
                        live = phase == AgentPhase.Live,
                        // Question cards are answerable while live + steerable
                        // (EXP-78); the card itself also checks its own state.
                        answerEnabled = perm == "steer" &&
                            phase == AgentPhase.Live &&
                            !sessionEnded,
                        answerStates = answerStates,
                        // One place decides semantic vs legacy: a card with a
                        // wire id answers through the `answer` frame, one
                        // without falls back to raw keystrokes (EXP-249).
                        onAnswer = { question, keys ->
                            val wireId = question.wireId
                            if (wireId != null) {
                                viewModel.sendQuestionAnswer(wireId, question.askId, keys)
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
                            ?: if (p.reconnecting) "Connection lost — reconnecting…" else "Disconnected",
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
                            "The agent is starting — waiting for the live stream…",
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
                        Icons.Filled.Difference,
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
                        Icons.Filled.ExpandLess,
                        contentDescription = "Show diff",
                        modifier = Modifier.size(16.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }

            // ── Steering input (perm-gated; sending steals the claim) ────────
            // No steering captions at all — steering should feel seamless
            // (EXP-197/EXP-268); the composer tint is the only claim signal.
            val inputVisible = perm == "steer" && phase == AgentPhase.Live && !sessionEnded
            if (inputVisible) {
                MessageInputRow(
                    active = steering,
                    onSend = viewModel::sendMessage,
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
    answerEnabled: Boolean,
    answerStates: Map<String, AnswerState>,
    /** (question, keys) — the option keys chosen on that card; a multi-select
     *  step sends all of them at once. */
    onAnswer: (AgentFeedItem.Question, List<String>) -> Unit,
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
    // Any lock counts as answered for stepper advance — a Sending lock advances
    // the stepper the moment the tap goes out (claude-TUI-snappy; web parity),
    // and the 5s no-ack timeout dropping the lock rolls the step back.
    val answered = remember(answerStates) { answerStates.keys }

    // Only user drags flip follow-mode; programmatic scrolls keep it.
    LaunchedEffect(listState) {
        snapshotFlow { listState.isScrollInProgress to listState.canScrollForward }
            .distinctUntilChanged()
            .collect { (dragging, canForward) ->
                if (dragging) follow = !canForward
            }
    }
    // Keyed on feed.size (not rows.size) — a growing trailing tool run adds
    // feed items without adding rows. scrollToItem alone lands on the last
    // item's TOP, which for a long message is nowhere near the end — the
    // scrollBy finishes the scroll to the true bottom (EXP-197).
    LaunchedEffect(feed.size, follow) {
        if (follow && rows.isNotEmpty()) {
            listState.scrollToItem(rows.size - 1)
            listState.scrollBy(1_000_000f)
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxSize(),
            // Bottom-anchored: a short feed sits above the input bar, not at
            // the top of the screen.
            verticalArrangement = Arrangement.Bottom,
            contentPadding = PaddingValues(vertical = 8.dp),
        ) {
            items(rows, key = { it.id }) { row ->
                when (row) {
                    is AgentFeedRow.ToolRun -> ToolGroupRow(
                        items = row.items,
                        liveTail = live && row.id == rows.last().id,
                    )
                    is AgentFeedRow.SubagentRun -> SubagentGroupRow(
                        subagent = row.subagent,
                        tools = row.tools,
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
                        is AgentFeedItem.Permission -> PermissionRow(item.tool, item.detail)
                        is AgentFeedItem.Subagent -> SubagentGroupRow(item, emptyList(), false)
                        is AgentFeedItem.Question -> QuestionCard(
                            item = item,
                            active = item.id in activeQuestionIds,
                            answerEnabled = answerEnabled,
                            state = answerStates[questionLockKey(item)],
                            stepLabel = null,
                            onAnswer = { keys -> onAnswer(item, keys) },
                            onSubmit = onSubmit,
                        )
                    }
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

// Assistant prose — a chat bubble with a small glyph, selectable text.
@Composable
private fun NarrationBubble(text: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            Icons.Filled.AutoAwesome,
            contentDescription = null,
            modifier = Modifier.size(13.dp).padding(top = 1.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        SelectionContainer(modifier = Modifier.weight(1f)) {
            Text(
                text,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier
                    .fillMaxWidth()
                    .glassSection()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            )
        }
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

// A human turn (EXP-78): the initial prompt or a steered message — rendered
// end-aligned like the sender's own chat bubble, long text folded.
@Composable
private fun UserMessageBubble(text: String) {
    var expanded by remember { mutableStateOf(false) }
    val folds = remember(text) { clampable(text) }
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
            SelectionContainer {
                Text(
                    text,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    maxLines = if (folds && !expanded) CLAMP_LINES else Int.MAX_VALUE,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (folds) {
                ShowMoreToggle(expanded) { expanded = !expanded }
            }
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
    onAnswer: (AgentFeedItem.Question, List<String>) -> Unit,
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
        onAnswer = { keys -> onAnswer(current, keys) },
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
            Icons.AutoMirrored.Filled.HelpOutline,
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
    /** Live + steer perm — whether this client may answer at all. */
    answerEnabled: Boolean,
    /** This client's send state — non-null means the card is locked. */
    state: AnswerState?,
    /** "Question 2 of 3" when the card is one step of a stepper. */
    stepLabel: String?,
    onAnswer: (List<String>) -> Unit,
    onSubmit: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    var picked by remember(item.id) { mutableStateOf(emptySet<String>()) }
    val folds = remember(item.text) { clampable(item.text) }
    val locked = state != null
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
            if (item.planMode) Icons.Filled.Checklist else Icons.AutoMirrored.Filled.HelpOutline,
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
                SelectionContainer {
                    Text(
                        item.text,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        maxLines = if (folds && !expanded) CLAMP_LINES else Int.MAX_VALUE,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (folds) {
                    ShowMoreToggle(expanded) { expanded = !expanded }
                }
            }
            if (item.resolved) {
                // Resolved (EXP-197/EXP-249): the answer replaces the options.
                AnsweredRow(item.answer)
            } else {
                item.options.forEachIndexed { index, option ->
                    // The wire's first option of a plan is the primary approve
                    // action ("Approve — auto-accept edits") — promote it.
                    val primary = item.planMode && index == 0
                    val selected = option.key in picked
                    val interactive = answerable || locked
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .then(
                                if (interactive) {
                                    Modifier
                                        .alpha(if (locked) 0.5f else 1f)
                                        .glassButton(active = primary || selected)
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
                                            if (!semantic) onAnswer(listOf(option.key))
                                        } else {
                                            picked = setOf(option.key)
                                            onAnswer(listOf(option.key))
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
                                if (semantic) onAnswer(picked.toList()) else onSubmit()
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
                            Icons.Filled.Check,
                            contentDescription = null,
                            modifier = Modifier.size(12.dp),
                            tint = LiveGreen,
                        )
                    }
                    Text(
                        if (state == AnswerState.Sending) {
                            "Sending your answer…"
                        } else {
                            "Answer sent — waiting for the agent."
                        },
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            } else if (active && !answerEnabled) {
                Text(
                    if (item.planMode) {
                        "Waiting for approval — you're viewing read-only."
                    } else {
                        "Waiting for an answer — you're viewing read-only."
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
            Icons.Filled.Check,
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
            if (checked) Icons.Filled.CheckCircle else Icons.Filled.RadioButtonUnchecked,
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

// A permission prompt the agent hit (EXP-249) — informational: it is answered
// on the desktop, never from here.
@Composable
private fun PermissionRow(tool: String, detail: String?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            Icons.Filled.Lock,
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
}

// A subagent and the tool calls it made (EXP-249), collapsed into one
// expandable group. While the group is the trailing row of a live session its
// latest call stays visible so the viewer still sees progress.
@Composable
private fun SubagentGroupRow(
    subagent: AgentFeedItem.Subagent,
    tools: List<AgentFeedItem.Tool>,
    liveTail: Boolean,
) {
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
                if (expanded) Icons.Filled.KeyboardArrowDown else Icons.AutoMirrored.Filled.KeyboardArrowRight,
                contentDescription = if (expanded) "Collapse" else "Expand",
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Icon(
                Icons.Filled.AccountTree,
                contentDescription = null,
                modifier = Modifier.size(12.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Text(
                "${subagent.agentType} subagent",
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (subagent.completed) {
                Icon(
                    Icons.Filled.Check,
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
            if (tools.isNotEmpty()) {
                Text(
                    "${tools.size} tool calls",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        when {
            expanded -> Column(modifier = Modifier.padding(start = 22.dp)) {
                if (!subagent.detail.isNullOrBlank()) {
                    Text(
                        subagent.detail,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
                tools.forEach { ToolRow(it.name, it.detail) }
            }
            liveTail && tools.isNotEmpty() -> Column(modifier = Modifier.padding(start = 22.dp)) {
                val latest = tools.last()
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
            Icons.Filled.Build,
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
                if (expanded) Icons.Filled.KeyboardArrowDown else Icons.AutoMirrored.Filled.KeyboardArrowRight,
                contentDescription = if (expanded) "Collapse" else "Expand",
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Icon(
                Icons.Filled.Build,
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

@Composable
private fun MessageInputRow(active: Boolean, onSend: (String) -> Unit) {
    var field by remember { mutableStateOf("") }
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.Bottom,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        TextField(
            value = field,
            onValueChange = { field = it },
            modifier = Modifier.weight(1f),
            placeholder = {
                Text(
                    "Message the agent…",
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            },
            maxLines = 4,
            shape = RoundedCornerShape(12.dp),
            colors = TextFieldDefaults.colors(
                // Subtle active tint while we hold the steer claim.
                focusedContainerColor = if (active) GlassTokens.RowFillActive else GlassTokens.RowFill,
                unfocusedContainerColor = if (active) GlassTokens.RowFillActive else GlassTokens.RowFill,
                disabledContainerColor = GlassTokens.RowFill,
                focusedIndicatorColor = Color.Transparent,
                unfocusedIndicatorColor = Color.Transparent,
                disabledIndicatorColor = Color.Transparent,
            ),
        )
        IconButton(
            onClick = {
                if (field.isNotBlank()) {
                    onSend(field)
                    field = ""
                }
            },
            enabled = field.isNotBlank(),
        ) {
            Icon(
                Icons.AutoMirrored.Filled.Send,
                contentDescription = "Send",
                tint = if (field.isNotBlank()) {
                    MaterialTheme.colorScheme.onSurface
                } else {
                    MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
                },
            )
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
