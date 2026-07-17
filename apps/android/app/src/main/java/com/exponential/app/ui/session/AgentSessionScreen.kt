package com.exponential.app.ui.session

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Checklist
import androidx.compose.material.icons.filled.Difference
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Replay
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
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
// narration bubbles + compact tool rows, a pinned "Latest changes" diff chip
// above the input bar, and message-shaped steering (steal-claim + text + \r).
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
    val feed by viewModel.feed.collectAsStateWithLifecycle()
    val latestDiff by viewModel.latestDiff.collectAsStateWithLifecycle()
    val viewers by viewModel.viewers.collectAsStateWithLifecycle()
    val steererId by viewModel.steererId.collectAsStateWithLifecycle()
    val perm by viewModel.perm.collectAsStateWithLifecycle()
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()

    LaunchedEffect(Unit) { viewModel.connectIfIdle() }
    // Auto-release the steer claim when the screen goes away (best-effort;
    // closing the socket releases it relay-side anyway).
    DisposableEffect(Unit) {
        onDispose { viewModel.releaseNow() }
    }

    val steering = steererId != null && steererId == currentUserId
    val otherSteerer = viewers.firstOrNull { it.userId == steererId && it.userId != currentUserId }
    val sessionEnded = session?.status == DomainContract.codingSessionStatusEnded
    // A trailing question/plan means the session is blocked on a human — the
    // header flips to "Needs your input" so it never looks silently stuck.
    val awaitingInput = phase == AgentPhase.Live &&
        remember(feed) { trailingQuestionIds(feed) }.isNotEmpty()

    var diffSheetOpen by remember { mutableStateOf(false) }

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
                        // (EXP-78); the card itself also checks the trailing run.
                        answerEnabled = perm == "steer" &&
                            phase == AgentPhase.Live &&
                            !sessionEnded,
                        onAnswer = viewModel::sendAnswer,
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
                    Text(
                        p.detail ?: "Disconnected",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.weight(1f),
                    )
                    Row(
                        modifier = Modifier
                            .glassButton()
                            .clickable { viewModel.connect() }
                            .padding(horizontal = 12.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(
                            Icons.Filled.Replay,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "Reconnect",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
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
            val inputVisible = perm == "steer" && phase == AgentPhase.Live && !sessionEnded
            if (inputVisible) {
                if (steering) {
                    SteerCaption("You're steering")
                } else if (otherSteerer != null) {
                    SteerCaption(
                        "${otherSteerer.name.ifBlank { "Someone" }} is steering",
                    )
                }
                MessageInputRow(
                    active = steering,
                    onSend = viewModel::sendMessage,
                )
            } else if (perm == "view" && phase == AgentPhase.Live) {
                SteerCaption("Watching — only team owners or the session owner can steer.")
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
    val connecting = phase == AgentPhase.Connecting || phase == AgentPhase.Starting
    val awaiting = phase == AgentPhase.Live && awaitingInput
    val dotColor = when (phase) {
        AgentPhase.Live -> if (awaiting) ConnectingYellow else LiveGreen
        AgentPhase.Connecting, AgentPhase.Starting -> ConnectingYellow
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
                is AgentPhase.Closed -> "Disconnected"
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
    /** (key, submit) — single-select taps submit (digit + Enter); multi-select
     *  taps toggle with the digit alone and [onSubmit] sends the Enter. */
    onAnswer: (String, Boolean) -> Unit,
    onSubmit: () -> Unit,
) {
    val listState = rememberLazyListState()
    var follow by remember { mutableStateOf(true) }
    // Only the trailing consecutive run of questions is still answerable —
    // any later event means the desktop TUI moved on (EXP-78).
    val activeQuestionIds = remember(feed) { trailingQuestionIds(feed) }
    // Consecutive tool calls collapse into "N tool calls" rows (EXP-97) — a
    // render-time projection only, the flat feed stays the state.
    val rows = remember(feed) { groupToolRuns(feed) }

    // Only user drags flip follow-mode; programmatic scrolls keep it.
    LaunchedEffect(listState) {
        snapshotFlow { listState.isScrollInProgress to listState.canScrollForward }
            .distinctUntilChanged()
            .collect { (dragging, canForward) ->
                if (dragging) follow = !canForward
            }
    }
    LaunchedEffect(rows.size, follow) {
        if (follow && rows.isNotEmpty()) listState.scrollToItem(rows.size - 1)
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
                    is AgentFeedRow.Single -> when (val item = row.item) {
                        is AgentFeedItem.Narration -> NarrationBubble(item.text)
                        is AgentFeedItem.Tool -> ToolRow(item.name, item.detail)
                        is AgentFeedItem.UserMessage -> UserMessageBubble(item.text)
                        is AgentFeedItem.Question -> QuestionCard(
                            item = item,
                            trailing = item.id in activeQuestionIds,
                            answerEnabled = answerEnabled,
                            onAnswer = onAnswer,
                            onSubmit = onSubmit,
                        )
                    }
                }
            }
        }
        if (!follow) {
            Text(
                "Jump to latest ↓",
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

// An interactive question (EXP-78): AskUserQuestion / plan approval. Option
// buttons send the option's raw TUI keystroke while the question is still in
// the trailing feed run; stale/view-only cards render options as plain rows.
// planMode cards (EXP-97) get a dedicated "Plan ready" presentation with the
// first option as the primary approve action and the plan rendered as
// markdown on expand — labels/keys always come from the wire options, the
// desktop owns the TUI key mapping. Best-effort by design — the desktop TUI
// remains the source of truth.
@Composable
private fun QuestionCard(
    item: AgentFeedItem.Question,
    /** Still the trailing feed run — the session is blocked on this card. */
    trailing: Boolean,
    /** Live + steer perm — whether this client may answer at all. */
    answerEnabled: Boolean,
    onAnswer: (String, Boolean) -> Unit,
    onSubmit: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    var picked by remember(item.id) { mutableStateOf(emptySet<String>()) }
    val folds = remember(item.text) { clampable(item.text) }
    val answerable = trailing && answerEnabled
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
            if (item.planMode) {
                Text(
                    "Plan ready",
                    style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                    color = PlanAccent,
                )
            }
            if (item.planMode && expanded) {
                // The plan is GFM markdown — render it properly once unfolded.
                MarkdownView(item.text)
            } else {
                SelectionContainer {
                    Text(
                        item.text,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        maxLines = if (folds && !expanded) CLAMP_LINES else Int.MAX_VALUE,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            if (folds) {
                ShowMoreToggle(expanded) { expanded = !expanded }
            }
            item.options.forEachIndexed { index, option ->
                if (answerable) {
                    // The wire's first option of a plan is the primary approve
                    // action ("Approve — auto-accept edits") — promote it.
                    val primary = item.planMode && index == 0
                    Row(
                        modifier = Modifier
                            .glassButton(active = primary || option.key in picked)
                            .clickable {
                                onAnswer(option.key, !item.multiSelect)
                                picked = if (item.multiSelect) {
                                    if (option.key in picked) picked - option.key
                                    else picked + option.key
                                } else {
                                    setOf(option.key)
                                }
                            }
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        QuestionOptionLabel(option, showKey = !item.planMode)
                    }
                } else {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        QuestionOptionLabel(option)
                    }
                }
            }
            if (answerable && item.multiSelect) {
                Text(
                    "Submit selection",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier
                        .glassButton(active = true)
                        .clickable(onClick = onSubmit)
                        .padding(horizontal = 10.dp, vertical = 6.dp),
                )
            }
            if (trailing && !answerable) {
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

@Composable
private fun QuestionOptionLabel(option: QuestionOption, showKey: Boolean = true) {
    if (showKey) {
        Text(
            option.key,
            style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
    Text(
        option.label,
        style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
        color = MaterialTheme.colorScheme.onSurface,
    )
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
private fun SteerCaption(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(start = 4.dp, top = 2.dp, bottom = 2.dp),
    )
}

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
