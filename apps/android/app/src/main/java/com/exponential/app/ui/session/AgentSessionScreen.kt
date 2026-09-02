package com.exponential.app.ui.session

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MAX_STEER_IMAGES
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.activeQuestionIds
import com.exponential.app.domain.canOfferFixConflicts
import com.exponential.app.domain.collectSubagents
import com.exponential.app.domain.currentStepperStep
import com.exponential.app.domain.groupFeedRows
import com.exponential.app.domain.localAnswerSummary
import com.exponential.app.domain.locksCard
import com.exponential.app.domain.visibleSubagentTabs
import com.exponential.app.ui.components.BottomBarPillFill
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPillButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.components.TopBarActionButton
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DiffAddColor
import com.exponential.app.ui.issue.DiffDelColor
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.PatchLines
import com.exponential.app.ui.issue.PulsingDot
import com.exponential.app.ui.issue.StartCodingSheet
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.issue.splitUnifiedDiff
import com.exponential.app.ui.issue.unifiedDiffStats
import com.exponential.app.ui.markdown.LocalAttachmentDims
import com.exponential.app.ui.markdown.LocalMarkdownAutolink
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.steer.SteerRunCaptionRow
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
// Package-visible: the Agents list paints a paused (offline-machine) session
// with the same neutral grey (EXP-550).
internal val LostGray = Color(0xFF71717A)
/** Accent for the "Plan ready" card + header cue (EXP-97). */
private val PlanAccent = DesignTokens.Semantic.Blue
/** Hairline around the steer composer card — the comment composer's stroke. */
private val ComposerStroke = Color.White.copy(alpha = 0.12f)

/** EXP-550: the one explanation of a paused (offline-machine) session. */
private const val DEVICE_OFFLINE_DETAIL =
    "The agent is paused on that machine and continues when it comes back online."

/**
 * EXP-550: the phases whose caption is "we are waiting for the publisher" —
 * exactly the ones an offline host explains. Live and a terminal end are not
 * waiting on anything.
 */
private val AgentPhase.isWaitingForStream: Boolean
    get() = this == AgentPhase.Idle || this == AgentPhase.Connecting ||
        this == AgentPhase.Starting || (this is AgentPhase.Closed && reconnecting)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentSessionScreen(
    onBack: () -> Unit,
    // EXP-706: the "Fix conflicts" run this screen can start lands in a NEW
    // session, which the caller navigates to (Reviews / Changes pattern).
    onOpenSteer: (String) -> Unit,
    viewModel: AgentSessionViewModel = hiltViewModel(),
) {
    val session by viewModel.session.collectAsStateWithLifecycle()
    val phase by viewModel.phase.collectAsStateWithLifecycle()
    // Whether the socket is actually up (EXP-621). The header and banners read
    // the PHASE — a silent redial must not flicker them — but the composer
    // reads this, so its send button never sits enabled over a dead socket.
    val connected by viewModel.connected.collectAsStateWithLifecycle()
    // EXP-549/550: the host machine via its LIVE devices row — the current
    // label, and "offline" = the run is PAUSED on a machine that went away
    // (lid closed). It is not ended and nothing here stops redialing; only
    // what the screen says changes, and it resumes when the machine returns.
    val hostDevice by viewModel.hostDevice.collectAsStateWithLifecycle()
    val hostOffline by viewModel.hostDeviceOffline.collectAsStateWithLifecycle()
    val activity by viewModel.activity.collectAsStateWithLifecycle()
    val feed = activity.feed
    val latestDiff = activity.latestDiff
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val killError by viewModel.killError.collectAsStateWithLifecycle()
    // EXP-678: the issue whose open PR the Merge pill above the composer
    // merges — null for a run with nothing to merge (see the VM).
    val mergeIssue by viewModel.mergeIssue.collectAsStateWithLifecycle()
    val merging by viewModel.merging.collectAsStateWithLifecycle()
    val mergeError by viewModel.mergeError.collectAsStateWithLifecycle()
    // EXP-706: a conflict-refused merge swaps the bar's pill for the builtin
    // "Fix merge conflicts" run — the launcher, its start feedback, and the
    // jump into the session the desktop reports back.
    val steerLaunchEnabled by viewModel.steerLaunchEnabled.collectAsStateWithLifecycle()
    val steerLaunchDevices by viewModel.steerDevices.collectAsStateWithLifecycle()
    val startCandidates by viewModel.startCandidates.collectAsStateWithLifecycle()
    val launchRunState by viewModel.runState.collectAsStateWithLifecycle()
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    var fixSheetOpen by remember { mutableStateOf(false) }
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
    }
    val attachmentDims by viewModel.attachmentDims.collectAsStateWithLifecycle()
    val answerStates = activity.answerLocks
    // EXP-588: per locked card, what this client picked — joined for display.
    val answerLabels = remember(activity.answerLocks, activity.answerLabels) {
        activity.answerLabels.keys.mapNotNull { key ->
            activity.localAnswerSummary(key)?.let { key to it }
        }.toMap()
    }
    val pendingImages by viewModel.pendingImages.collectAsStateWithLifecycle()
    val steerSending by viewModel.steerSending.collectAsStateWithLifecycle()
    val steerImageError by viewModel.steerImageError.collectAsStateWithLifecycle()
    // EXP-621: the draft lives with the connection, not in a `remember` — so a
    // half-typed message survives a reconnect, a back-tap and a rotation.
    val draft by viewModel.draft.collectAsStateWithLifecycle()
    // EXP-484: the host machine's rate-limit usage for the agent this run
    // launched with — absent (and silent) unless the run is live, recorded its
    // agent, and its machine reported fresh numbers.
    val agentUsage by viewModel.agentUsage.collectAsStateWithLifecycle()
    // EXP-688: who that machine is signed in as for this agent — the Usage
    // sheet's caption. Null whenever the machine never reported an account.
    val agentAccount by viewModel.agentAccount.collectAsStateWithLifecycle()
    // The run's OWN issue (EXP-688) — the header names what is being worked
    // on, exactly like the Agents list row does.
    val issue by viewModel.issue.collectAsStateWithLifecycle()

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
                    PendingAttachment(
                        uri = uri,
                        bytes = bytes,
                        filename = MarkdownMediaUtils.guessFilename(context, uri),
                        contentType = MarkdownMediaUtils.guessMimeType(context, uri),
                        isImage = true,
                    )
                } ?: return@forEach
                viewModel.addPendingImage(
                    picked.uri,
                    picked.bytes,
                    picked.filename,
                    picked.contentType,
                )
            }
        }
    }

    // Attaching revives the connection (EXP-625): since EXP-621 a return visit
    // re-attaches to the running one and its feed is already there, but a dial
    // loop that died while the screen was away is redialed here instead of
    // leaving the screen on a spinner nothing could clear. Foreground and
    // network revivals are the store's job, not this screen's.
    LaunchedEffect(Unit) { viewModel.ensureConnected() }
    val sessionEnded = session?.status == DomainContract.codingSessionStatusEnded
    // EXP-696: leave the screen when the run finishes under the viewer (kill,
    // merge, the agent's own exit — the synced row edge covers every path).
    // Edge-triggered: a screen opened onto an ALREADY-ended run (a finished
    // automation's feed) must stay put, so only the live→ended flip pops.
    // The latch arms only on a REAL live row — the session flow starts as
    // null, and arming on that snapshot would pop a restored screen whose
    // run ended while the process was dead.
    var wasLive by remember { mutableStateOf(false) }
    LaunchedEffect(session?.status) {
        val status = session?.status ?: return@LaunchedEffect
        if (status != DomainContract.codingSessionStatusEnded) {
            wasLive = true
        } else if (wasLive) {
            onBack()
        }
    }
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
                    item.wireId?.let { answerStates[it] }?.locksCard() != true
            }
        }

    var diffSheetOpen by remember { mutableStateOf(false) }
    var killDialogOpen by remember { mutableStateOf(false) }
    var mergeConfirmOpen by remember { mutableStateOf(false) }
    // EXP-688: the top bar's "…" menu, and the Usage sheet it opens.
    var overflowOpen by remember { mutableStateOf(false) }
    var usageSheetOpen by remember { mutableStateOf(false) }
    // The floating Latest-changes bar's measured height — the feed pads its
    // tail by it, so the last message always scrolls clear of the bar.
    var barHeightPx by remember { mutableIntStateOf(0) }
    val barInset = with(LocalDensity.current) { barHeightPx.toDp() }

    // EXP-656: the reader's place in the feed lives at the SCREEN level, not
    // inside ActivityFeed. Held there, a single frame of empty feed flipped the
    // `when` below to a placeholder, which DISPOSED the LazyColumn's scroll
    // state, the follow flag and the focused subagent tab — so a relay replay
    // (every join gets one) came back with follow re-armed and dumped a reader
    // parked mid-plan at the bottom. Rows carry stable keys, so hoisted state
    // keeps the anchor across a whole feed swap.
    val feedListState = rememberLazyListState()
    var follow by rememberSaveable { mutableStateOf(true) }
    var agentTab by rememberSaveable { mutableStateOf<String?>(null) }
    // Belt-and-braces on top of the staged replay (SteerConnection): once the
    // feed has rendered anything, a momentarily empty one holds the last frame
    // instead of remounting the "Waiting for activity…" placeholder.
    var everRendered by remember { mutableStateOf(false) }
    LaunchedEffect(feed.isEmpty()) { if (feed.isNotEmpty()) everRendered = true }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    SessionHeaderTitle(
                        session = session,
                        issue = issue,
                        phase = phase,
                        deviceLabel = hostDevice.displayLabel,
                        awaitingInput = awaitingInput,
                        paused = hostOffline && phase.isWaitingForStream,
                    )
                },
                navigationIcon = {
                    TopBarBackButton(onClick = onBack)
                },
                actions = {
                    // EXP-688: one "…" (the issue-detail pattern) instead of a
                    // bare red kill glyph — Usage joined it when the usage
                    // strip left the header.
                    // Kill switch (EXP-268): only while the synced row is
                    // still live, for the session owner — everything about a
                    // live session is owner-only (EXP-312; server enforces
                    // too).
                    val row = session
                    val canKill = row != null && !sessionEnded && row.userId == currentUserId
                    val usage = agentUsage
                    if (canKill || usage != null) {
                        // The Box stays: it anchors the dropdown to the button.
                        Box {
                            TopBarActionButton(
                                ExpIcons.uiMore,
                                "Session actions",
                                onClick = { overflowOpen = true },
                            )
                            GlassDropdownMenu(
                                expanded = overflowOpen,
                                onDismissRequest = { overflowOpen = false },
                            ) {
                                if (usage != null) {
                                    GlassMenuItem(
                                        leadingIcon = {
                                            Icon(ExpIcons.uiUsage, contentDescription = null)
                                        },
                                        text = { Text("Usage") },
                                        onClick = {
                                            overflowOpen = false
                                            usageSheetOpen = true
                                        },
                                    )
                                }
                                if (canKill) {
                                    GlassMenuItem(
                                        leadingIcon = {
                                            Icon(ExpIcons.codingStop, contentDescription = null)
                                        },
                                        text = { Text("Kill session") },
                                        destructive = true,
                                        onClick = {
                                            overflowOpen = false
                                            killDialogOpen = true
                                        },
                                    )
                                }
                            }
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
            // EXP-688: the usage strip that used to sit here is gone — usage
            // lives in the "…" menu's Usage sheet, and the Latest-changes bar
            // FLOATS over the tail of this feed instead of eating its height.
            Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
                when {
                    // EXP-550: the machine is gone — an endless "waiting for
                    // the live stream" spinner was the bug. The run is parked,
                    // and it picks up when the machine comes back.
                    feed.isEmpty() && hostOffline && phase.isWaitingForStream ->
                        CenteredState {
                            Icon(
                                ExpIcons.uiDeviceOffline,
                                contentDescription = null,
                                tint = LostGray,
                                modifier = Modifier.size(22.dp),
                            )
                            Text(
                                "${hostDevice.displayLabel} is offline",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Text(
                                DEVICE_OFFLINE_DETAIL,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                textAlign = TextAlign.Center,
                            )
                        }
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
                    feed.isEmpty() && !everRendered && phase == AgentPhase.Live &&
                        latestDiff == null ->
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
                            // Hoisted (EXP-656) — see the declarations above.
                            listState = feedListState,
                            follow = follow,
                            onFollowChange = { follow = it },
                            agentTab = agentTab,
                            onAgentTabChange = { agentTab = it },
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
                            answerLabels = answerLabels,
                            // Every answer is one semantic `answer` frame keyed
                            // by the card's wire id (EXP-249); an id-less card
                            // renders read-only and never reaches this (EXP-672).
                            onAnswer = { question, keys, text ->
                                val wireId = question.wireId
                                if (wireId != null) {
                                    // The picked labels (a typed free-text reply
                                    // wins over its row's "Type something"
                                    // label) — what the stepper shows for this
                                    // step until the ask resolves (EXP-588).
                                    val labels = keys.mapNotNull { key ->
                                        val option = question.options.firstOrNull { it.key == key }
                                            ?: return@mapNotNull null
                                        if (option.freeText && !text.isNullOrBlank()) text else option.label
                                    }
                                    viewModel.sendQuestionAnswer(
                                        wireId, question.askId, keys, text, labels,
                                    )
                                }
                            },
                            // The floating bar overlays the tail of the feed —
                            // the list pads past it so the last message (and
                            // every scroll-to-bottom) lands above it.
                            bottomInset = barInset,
                        )
                    }
                }

                // ── The floating "Latest changes" chip + Merge pill (EXP-688:
                // an overlay pinned to the bottom of the feed, not a solid bar
                // that eats feed height). EXP-678: the pill is offered while
                // the run's PR is open and the composer is still there; the
                // merge ends the session server-side (EXP-498), so it retires
                // itself.
                val diff = latestDiff
                val canMerge = mergeIssue?.prState == DomainContract.prStateOpen &&
                    !sessionEnded && phase !is AgentPhase.Ended
                // EXP-706: a REAL conflict on a PR whose branch we recorded
                // (EXP-533's rule) REPLACES the Merge pill with the recovery
                // run — one slot, one thing that can move the PR forward.
                val canFixConflicts = canMerge && canOfferFixConflicts(
                    mergeError,
                    mergeIssue?.branch,
                    steerEnabled = steerLaunchEnabled == true,
                )
                val barVisible = diff != null || canMerge
                // A retired bar owes the feed its height back.
                LaunchedEffect(barVisible) { if (!barVisible) barHeightPx = 0 }
                if (barVisible) {
                    Row(
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .fillMaxWidth()
                            .padding(bottom = 8.dp)
                            .onSizeChanged { barHeightPx = it.height }
                            // Both children measure to the taller one's height,
                            // so the pill lines up with the chip whatever it
                            // says.
                            .height(IntrinsicSize.Min),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        if (diff != null) {
                            val stats = remember(diff) { unifiedDiffStats(diff) }
                            Row(
                                modifier = Modifier
                                    .weight(1f)
                                    .fillMaxHeight()
                                    // opaque: the feed scrolls beneath the bar
                                    // (EXP-165, the Jump-to-bottom pill's rule).
                                    .glassRow(opaque = true)
                                    .clickable { diffSheetOpen = true }
                                    .padding(horizontal = 12.dp, vertical = 10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Icon(
                                    ExpIcons.codingDiff,
                                    contentDescription = null,
                                    modifier = Modifier.size(14.dp),
                                    tint = MaterialTheme.colorScheme.onSurface.copy(
                                        alpha = TextEmphasis.Secondary,
                                    ),
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
                                    tint = MaterialTheme.colorScheme.onSurface.copy(
                                        alpha = TextEmphasis.Tertiary,
                                    ),
                                )
                            }
                        } else {
                            // No diff yet: the pill still sits on the right.
                            Spacer(Modifier.weight(1f))
                        }
                        if (canMerge) {
                            GlassPillButton(
                                if (canFixConflicts) "Fix conflicts" else "Merge",
                                onClick = {
                                    if (canFixConflicts) {
                                        fixSheetOpen = true
                                    } else {
                                        mergeConfirmOpen = true
                                    }
                                },
                                icon = if (canFixConflicts) ExpIcons.uiBranch else ExpIcons.prMerged,
                                enabled = !merging,
                                loading = merging,
                                // Floats over the feed like the chip beside it.
                                opaque = true,
                                // Matches the chip's 10dp — same label style,
                                // same 14dp glyph, so both measure the same
                                // height.
                                verticalPadding = 10.dp,
                                modifier = Modifier.fillMaxHeight(),
                            )
                        }
                    }
                }
            }

            // ── Status banners (feed retained above) ─────────────────────────
            val pausedBanner = hostOffline && phase.isWaitingForStream
            // EXP-550: an offline host outranks the "reconnecting" /
            // "starting" captions — the reason the stream is quiet is the
            // machine, not the connection, and the run resumes when it is
            // back. Only shown with a feed above it; an empty feed already
            // renders the centered offline state.
            if (pausedBanner) {
                if (feed.isNotEmpty()) {
                    BannerRow {
                        Icon(
                            ExpIcons.uiDeviceOffline,
                            contentDescription = null,
                            tint = LostGray,
                            modifier = Modifier.size(13.dp),
                        )
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                "${hostDevice.displayLabel} is offline",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Text(
                                DEVICE_OFFLINE_DETAIL,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    }
                }
            } else {
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

            // A failed merge (EXP-678) — like the kill banner, success needs
            // none: the server ends the session and the flip syncs back.
            // EXP-706: the MESSAGE only; the conflict recovery run took the
            // Merge pill's slot in the bar above.
            val mergeFailure = mergeError
            if (mergeFailure != null) {
                BannerRow {
                    Text(
                        mergeFailure.message,
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }

            // Start feedback for that recovery run (EXP-706) — the same
            // caption Reviews and the Changes bar show.
            SteerRunCaptionRow(
                launchRunState,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp),
            )

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
            // EXP-621: the composer is present for the WHOLE life of the
            // session, not only while the socket happens to be up — a
            // mid-reconnect blip used to yank the keyboard and the typed text
            // away. Only a finished session retires it; until the stream is
            // live, sending is disabled rather than hidden.
            if (!sessionEnded && phase !is AgentPhase.Ended) {
                SteerComposer(
                    value = draft,
                    onValueChange = viewModel::setDraft,
                    pendingImages = pendingImages,
                    canAttach = session?.issueId != null,
                    sending = steerSending,
                    // Phase alone lies here: the silent 4008 redial holds Live
                    // while the socket is briefly gone, and a send over it was
                    // dropped without a word. Gate on the socket too, so the
                    // button dims and the placeholder says "reconnecting…".
                    live = phase == AgentPhase.Live && connected,
                    planPending = planAwaitingApproval,
                    onPickImages = {
                        imagePicker.launch(
                            PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly),
                        )
                    },
                    onRemoveImage = viewModel::removePendingImage,
                    onSend = viewModel::sendDraft,
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

    // "Fix conflicts" (EXP-706, Reviews parity EXP-323): the unified sheet
    // opened on the builtin action with THIS run's pull request already picked.
    if (fixSheetOpen) {
        StartCodingSheet(
            devices = steerLaunchDevices ?: emptyList(),
            issues = startCandidates,
            preselectedIds = emptySet(),
            preselectedActionId = DomainContract.builtinFixConflictsId,
            preselectedPrIssueId = mergeIssue?.id,
            onStart = viewModel::startCoding,
            onRunAction = viewModel::runAction,
            onDismiss = { fixSheetOpen = false },
        )
    }

    // ── The Usage sheet (EXP-688) — where the header's usage strip went.
    // Only reachable while the host machine reports fresh numbers for this
    // run's agent, so the sheet retires itself when they age out.
    val sheetUsage = agentUsage
    LaunchedEffect(sheetUsage) { if (sheetUsage == null) usageSheetOpen = false }
    if (usageSheetOpen && sheetUsage != null) {
        GlassSheet(title = "Usage", onDismiss = { usageSheetOpen = false }) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 20.dp),
            ) {
                // Whose limits these are — the machine's sign-in for this
                // agent, without the agent prefix (the sheet is already about
                // this run's agent).
                agentAccount?.let { account ->
                    Text(
                        AgentUsagePresentation.accountCaption(account),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Secondary,
                        ),
                    )
                    Spacer(Modifier.height(12.dp))
                }
                AgentUsageCards(usage = sheetUsage)
                Spacer(Modifier.height(8.dp))
            }
        }
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

    // EXP-498: merging always closes the session too, so the merge is
    // confirm-gated — same copy as Agents and Reviews.
    if (mergeConfirmOpen) {
        AlertDialog(
            onDismissRequest = { mergeConfirmOpen = false },
            title = { Text("Merge pull request?") },
            text = {
                Text("Merges the pull request, completes every linked issue, and closes the coding session.")
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        mergeConfirmOpen = false
                        viewModel.merge()
                    },
                ) { Text("Merge") }
            },
            dismissButton = {
                TextButton(onClick = { mergeConfirmOpen = false }) { Text("Cancel") }
            },
        )
    }
}

// ── Header: the session's identity, with its live status under it ────────────

/**
 * EXP-688: what the steering screen is steering. Line 1 is the Agents list
 * row's identity line ([SessionRowTitle] — the shared composable, so the two
 * can't drift): status dot, mono identifier, issue title. Line 2 is the
 * status caption the header used to be all by itself ("Live · macbook"),
 * which never said which issue was being worked on.
 */
@Composable
private fun SessionHeaderTitle(
    session: CodingSessionEntity?,
    issue: IssueEntity?,
    phase: AgentPhase,
    deviceLabel: String?,
    /** Live but blocked on a trailing question/plan — waiting for a human
     *  answer, not stuck (EXP-97). */
    awaitingInput: Boolean = false,
    /** EXP-550: the host machine is offline while we wait for its stream —
     *  the run is parked on it, so nothing here reads as connecting. */
    paused: Boolean = false,
) {
    // Auto-reconnecting after a drop reads as connecting (EXP-243) — unless
    // the machine itself is offline, which is a paused run, not a connection
    // problem (EXP-550).
    val connecting = !paused && (
        phase == AgentPhase.Connecting || phase == AgentPhase.Starting ||
            (phase is AgentPhase.Closed && phase.reconnecting)
        )
    val awaiting = phase == AgentPhase.Live && awaitingInput
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        SessionRowTitle(
            identifier = sessionRowIdentifier(issue),
            // The row hasn't synced yet: name the surface rather than nothing.
            title = session?.let { sessionRowTitle(it, issue) } ?: "Coding session",
            dot = {
                // The list row's dot rule: a working run pulses, every parked
                // state is a static tone (EXP-194/EXP-214/EXP-550).
                when {
                    paused -> StaticDot(LostGray)
                    awaiting -> StaticDot(NeedsInputAmber)
                    phase == AgentPhase.Live -> PulsingDot()
                    connecting -> StaticDot(ConnectingYellow)
                    else -> StaticDot(LostGray)
                }
            },
        )
        Text(
            sessionStatusLine(phase, deviceLabel, awaiting, paused),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** `Live · macbook` / `Needs your input · macbook` / `Paused · …` / `Session
 *  ended` — the caption under the identity line. */
private fun sessionStatusLine(
    phase: AgentPhase,
    deviceLabel: String?,
    awaiting: Boolean,
    paused: Boolean,
): String {
    val label = deviceLabel?.takeIf { it.isNotBlank() }
    return when {
        paused -> if (label != null) "Paused · $label" else "Paused"
        else -> when (phase) {
            AgentPhase.Live -> {
                val prefix = if (awaiting) "Needs your input" else "Live"
                if (label != null) "$prefix · $label" else prefix
            }
            AgentPhase.Connecting, AgentPhase.Starting, AgentPhase.Idle -> "Connecting…"
            is AgentPhase.Ended -> "Session ended"
            is AgentPhase.Closed -> if (phase.reconnecting) "Reconnecting…" else "Disconnected"
        }
    }
}

// ── The feed ─────────────────────────────────────────────────────────────────

@Composable
private fun ActivityFeed(
    feed: List<AgentFeedItem>,
    live: Boolean,
    /** Hoisted to the screen (EXP-656): the reader's scroll anchor, whether
     *  the feed auto-follows its tail, and the focused subagent tab. They
     *  outlive this composable so no placeholder flip can reset them. */
    listState: LazyListState,
    follow: Boolean,
    onFollowChange: (Boolean) -> Unit,
    agentTab: String?,
    onAgentTabChange: (String?) -> Unit,
    /** EXP-389: show the trailing "Working…" indicator — the session is live
     *  and nothing waits on the user. */
    working: Boolean,
    answerEnabled: Boolean,
    answerStates: Map<String, AnswerState>,
    /** EXP-588: lock key → the locally picked answer summary. */
    answerLabels: Map<String, String>,
    /** (question, keys, text) — the option keys chosen on that card (a
     *  multi-select step sends all of them at once); `text` is the typed
     *  reply for a `freeText` option (EXP-513), else null. */
    onAnswer: (AgentFeedItem.Question, List<String>, String?) -> Unit,
    /** EXP-688: how much of the feed's tail the floating Latest-changes bar
     *  covers. The list pads past it (and so does the Jump-to-bottom pill), so
     *  the last message is never parked underneath it. */
    bottomInset: Dp = 0.dp,
) {
    // A card with a wire id stays answerable until it resolves; an id-less one
    // (a pre-EXP-249 desktop) is read-only (EXP-672).
    val activeQuestionIds = remember(feed) { activeQuestionIds(feed) }
    // Subagent groups, askId steppers and consecutive tool runs are all
    // render-time projections only — the flat feed stays the state.
    val rows = remember(feed) { groupFeedRows(feed) }
    // EXP-356: conversation tabs — null is the main agent; a subagent id
    // focuses that agent's stream. Falls back to Main whenever the id
    // vanishes from the feed (an activity_reset replay). EXP-387: the strip
    // only shows the still-running subagents (plus the focused tab).
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
                if (dragging) onFollowChange(nearBottom)
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
                    onAgentTabChange(id)
                    onFollowChange(true)
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
            contentPadding = PaddingValues(top = 8.dp, bottom = 8.dp + bottomInset),
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
                        answerLabels = answerLabels,
                        onAnswer = onAnswer,
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
                            state = item.wireId?.let { answerStates[it] },
                            stepLabel = null,
                            onAnswer = { keys, text -> onAnswer(item, keys, text) },
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
                    .padding(bottom = 8.dp + bottomInset)
                    // opaque: the feed scrolls beneath this pill (EXP-165).
                    .glassButton(active = true, opaque = true)
                    .clickable { onFollowChange(true) }
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
            Modifier.foldedTo(clampHeight)
        } else {
            Modifier
        },
        softBreaksAsNewlines = softBreaks,
    )
    if (folds) {
        ShowMoreToggle(expanded, onToggle)
    }
}

/** The fold CLIPS naturally-laid-out content instead of constraining its
 *  measurement — iOS `.frame(maxHeight:).clipped()` / web `max-h-40
 *  overflow-hidden` parity. A max-height CONSTRAINT squeezes every block's
 *  measure pass, and an `aspectRatio` image whose width is pinned by
 *  `fillMaxWidth` degenerates under a too-small max height (EXP-605: prompt
 *  images collapsing onto the surrounding text). */
private fun Modifier.foldedTo(clampHeight: Dp): Modifier =
    clipToBounds().layout { measurable, constraints ->
        val placeable = measurable.measure(
            constraints.copy(minHeight = 0, maxHeight = Constraints.Infinity),
        )
        val height = minOf(placeable.height, clampHeight.roundToPx())
        layout(placeable.width, height) { placeable.placeRelative(0, 0) }
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
    /** EXP-588: lock key → the locally picked answer summary. */
    answerLabels: Map<String, String>,
    onAnswer: (AgentFeedItem.Question, List<String>, String?) -> Unit,
) {
    val current = remember(steps, answered) { currentStepperStep(steps, answered) }
    if (current == null) {
        AnsweredAskCard(steps, answerLabels)
        return
    }
    val total = current.total ?: steps.count { it.index != null }
    // EXP-588 (web/iOS parity): the steps already answered stay visible above
    // the current one — the question folded to one line next to its answer.
    val prior = remember(steps, current) { steps.takeWhile { it.id != current.id } }
    QuestionCard(
        item = current,
        active = current.id in activeQuestionIds,
        answerEnabled = answerEnabled,
        state = current.wireId?.let { answerStates[it] },
        stepLabel = when {
            current.index != null && total > 0 -> "Question ${current.index} of $total"
            // No index: the ask's final review step.
            else -> "Review your answers"
        },
        priorSteps = prior,
        priorAnswers = prior.map { stepAnswer(it, answerLabels) },
        localAnswer = current.wireId?.let { answerLabels[it] },
        onAnswer = { keys, text -> onAnswer(current, keys, text) },
    )
}

/** A step's answer for display: the desktop-resolved text, else what this
 *  client picked (EXP-588); null = answered elsewhere, unknown here. */
private fun stepAnswer(step: AgentFeedItem.Question, answerLabels: Map<String, String>): String? =
    step.answer?.takeIf { it.isNotBlank() } ?: step.wireId?.let { answerLabels[it] }

/** One already-answered step of a stepper: the question on the left, folded
 *  to one line, the answer on the right (web `AnsweredStepRow` parity). */
@Composable
private fun AnsweredStepRow(step: AgentFeedItem.Question, answer: String?) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            ExpIcons.uiCheck,
            contentDescription = null,
            modifier = Modifier.size(13.dp).padding(top = 1.dp),
            tint = LiveGreen,
        )
        Text(
            step.header?.takeIf { it.isNotBlank() } ?: step.text,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            answer?.takeIf { it.isNotBlank() } ?: "Answered",
            modifier = Modifier.widthIn(max = 180.dp),
            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.End,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// Every step of a fully answered ask, each with the answer it got.
@Composable
private fun AnsweredAskCard(
    steps: List<AgentFeedItem.Question>,
    answerLabels: Map<String, String>,
) {
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
                    AnsweredRow(stepAnswer(step, answerLabels))
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
// the card is answerable, option rows send their keys in ONE semantic `answer`
// frame keyed by the card's wire id; stale, view-only and id-less cards render
// options as plain rows. A tapped card locks IMMEDIATELY (EXP-249) and stays
// locked through the desktop's answer_ack, so an answer can never be
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
    /** The ask's already-answered steps, summarized above this one (EXP-588). */
    priorSteps: List<AgentFeedItem.Question> = emptyList(),
    /** Per prior step: its answer text, or null when unknown here. */
    priorAnswers: List<String?> = emptyList(),
    /** What this client picked for THIS card — the resolved row's fallback
     *  when the desktop's resolution carried no answer text (EXP-588). */
    localAnswer: String? = null,
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
    Row(
        modifier = Modifier
            .fillMaxWidth()
            // EXP-627: the store slide's pop-out rect is measured off the
            // question card (`PopRects`), iOS parity.
            .testTag("agent-feed-question")
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
            if (priorSteps.isNotEmpty()) {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    priorSteps.forEachIndexed { position, step ->
                        AnsweredStepRow(step, priorAnswers.getOrNull(position))
                    }
                }
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
                AnsweredRow(item.answer?.takeIf { it.isNotBlank() } ?: localAnswer)
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
                                            // Every picked key goes out at once
                                            // when the card submits.
                                            picked = if (selected) picked - option.key
                                            else picked + option.key
                                        } else if (option.freeText) {
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
                        GlassTextField(
                            value = freeTextValue,
                            onValueChange = { freeTextValue = it.take(4000) },
                            modifier = Modifier.weight(1f),
                            placeholder = "Type your answer…",
                            maxLines = 3,
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
                    // One frame carrying every picked key.
                    val enabled = answerable && picked.isNotEmpty()
                    Text(
                        "Submit",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .alpha(if (enabled) 1f else 0.5f)
                            .glassButton(active = true)
                            .clickable(enabled = enabled) { onAnswer(picked.toList(), null) }
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
            } else if (item.wireId == null && !item.resolved) {
                // A card from a pre-EXP-249 desktop: it carries no wire id, so
                // no `answer` frame can address it and the raw-keystroke
                // fallback is gone (EXP-672). Read-only, and say why.
                Text(
                    "Update the desktop app to answer this here.",
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

/**
 * The steering composer (EXP-511) restyled to the comment composer's chrome
 * (EXP-554): ONE rounded card — the near-opaque bottom-bar pill fill under a
 * hairline stroke — holding the pending-image strip, a transparent text field,
 * and the `[+] · spacer · send` row, with the send glyph tinted indigo the
 * moment there is something to send.
 *
 * Chrome only: the image cap, the upload-on-send path and the frozen steer
 * message wire format are untouched.
 *
 * EXP-621: the text is NOT owned here — it belongs to the connection, so the
 * composer is a pure function of a draft that outlives the screen. It also
 * renders while the stream is down ([live] false), with sending disabled: a
 * reconnect must never eat what the user was typing.
 */
@Composable
private fun SteerComposer(
    value: String,
    onValueChange: (String) -> Unit,
    pendingImages: List<PendingAttachment>,
    canAttach: Boolean,
    sending: Boolean,
    /** The relay stream is up — only then can a message actually go out. */
    live: Boolean,
    /** A plan-approval card is awaiting the human — the composer doubles as
     *  the "tell Claude what to change" path (EXP-529 batch). */
    planPending: Boolean,
    onPickImages: () -> Unit,
    onRemoveImage: (Int) -> Unit,
    onSend: () -> Unit,
) {
    val canSend = (value.isNotBlank() || pendingImages.isNotEmpty()) && !sending && live
    val shape = RoundedCornerShape(24.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(BottomBarPillFill)
            .border(GlassTokens.Hairline, ComposerStroke, shape)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    ) {
        PendingAttachmentStrip(
            items = pendingImages,
            enabled = !sending,
            onRemove = onRemoveImage,
        )
        TextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier.fillMaxWidth(),
            placeholder = {
                Text(
                    when {
                        planPending -> "Tell Claude what to change…"
                        // Typing is always allowed; the message just waits for
                        // the stream to come back (EXP-621).
                        !live -> "Message the agent (reconnecting…)"
                        else -> "Message the agent…"
                    },
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            },
            maxLines = 4,
            colors = TextFieldDefaults.colors(
                focusedContainerColor = Color.Transparent,
                unfocusedContainerColor = Color.Transparent,
                disabledContainerColor = Color.Transparent,
                focusedIndicatorColor = Color.Transparent,
                unfocusedIndicatorColor = Color.Transparent,
                disabledIndicatorColor = Color.Transparent,
            ),
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (canAttach) {
                IconButton(onClick = onPickImages, enabled = !sending) {
                    Icon(
                        ExpIcons.uiAdd,
                        contentDescription = "Attach image",
                        modifier = Modifier.size(20.dp),
                        tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                    )
                }
            }
            Spacer(Modifier.weight(1f))
            IconButton(
                // The draft is cleared by the send itself, and only once the
                // message is out (EXP-621) — a failed image upload leaves the
                // whole composition intact to retry.
                onClick = { if (canSend) onSend() },
                enabled = canSend,
            ) {
                if (sending) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = Color.White,
                    )
                } else {
                    Icon(
                        ExpIcons.uiSend,
                        contentDescription = "Send",
                        modifier = Modifier.size(24.dp),
                        tint = if (canSend) Color.White else Color.White.copy(alpha = 0.3f),
                    )
                }
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
@Composable
private fun UnifiedDiffPanel(diff: String, onDismiss: () -> Unit) {
    val sections = remember(diff) { splitUnifiedDiff(diff) }
    val stats = remember(diff) { unifiedDiffStats(diff) }
    val contextColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

    GlassSheet(
        title = "Latest changes",
        onDismiss = onDismiss,
        height = SheetHeight.Full,
        headerAction = {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
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
        },
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
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
