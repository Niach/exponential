package com.exponential.app.ui.session

import com.exponential.app.data.api.AnswerInput
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.domain.EmptyStateCopy
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.FlowRowScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import com.exponential.app.domain.DetailChrome
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.detailHazeSource
import com.exponential.app.ui.components.FadeEdge
import com.exponential.app.ui.components.EdgeFade
import com.exponential.app.ui.components.InlineSpinner
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.FloatingBarEdge
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.PaddingValues
import com.exponential.app.domain.PLAN_MODE_LABEL
import com.exponential.app.domain.STEER_COMPOSER_PLACEHOLDER
import com.exponential.app.domain.OPEN_RESULTS_LABEL
import com.exponential.app.domain.SESSION_INLINE_TILE_HEIGHT
import com.exponential.app.domain.sessionResultPicture
import com.exponential.app.domain.SessionResultEntry
import com.exponential.app.domain.ToastAction
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.TURN_STATE_STARTED
import com.exponential.app.domain.SessionTurn
import com.exponential.app.domain.SessionTurns
import com.exponential.app.domain.TurnFeedRow
import com.exponential.app.domain.WireTimestamps
import com.exponential.app.domain.recordFeedMessages
import com.exponential.app.domain.recordTurnSlot
import com.exponential.app.domain.sessionTurns
import com.exponential.app.domain.turnEventsOf
import com.exponential.app.domain.firstTurnEndKnown
import com.exponential.app.domain.turnLogFor
import com.exponential.app.domain.userMessageCaption
import com.exponential.app.domain.sessionResultTileCaption
import com.exponential.app.domain.sessionResultTileHeightFitting
import com.exponential.app.ui.work.ResultPreviewDialog
import com.exponential.app.ui.work.ResultTile
import com.exponential.app.domain.composerModel
import com.exponential.app.ui.components.BarCapsule
import com.exponential.app.ui.components.ContextRing
import com.exponential.app.ui.components.FloatingBottomBar
import com.exponential.app.ui.components.FloatingComposer
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelOptionsFor
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.PlanCard
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.BACKGROUND_TASK_KIND_AGENT
import com.exponential.app.domain.TaskQueueBackgroundInput
import com.exponential.app.domain.TaskQueueInput
import com.exponential.app.domain.TaskQueueTaskInput
import com.exponential.app.domain.TaskQueues
import com.exponential.app.domain.Trace as TraceModels
import com.exponential.app.domain.TraceStatus
import com.exponential.app.domain.TraceSpec
import com.exponential.app.domain.SUBAGENT_FALLBACK_TYPE
import com.exponential.app.domain.durationMs
import com.exponential.app.domain.openWaits
import com.exponential.app.ui.components.CodeBlock
import com.exponential.app.ui.components.CodeBlockLanguage
import com.exponential.app.ui.components.TaskQueueView
import com.exponential.app.ui.components.Trace
import com.exponential.app.ui.components.TraceCollapsible
import com.exponential.app.ui.components.TraceLine
import com.exponential.app.ui.components.TraceSize
import com.exponential.app.ui.components.rememberTickingNow
import com.exponential.app.ui.components.traceTitleWithSubject
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.AgentHealthRules
import com.exponential.app.domain.AgentPhase
import com.exponential.app.domain.AgentRowClass
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.ContextLayoutPresentation
import com.exponential.app.domain.ToolCallSummary
import com.exponential.app.domain.ExpToolDisplay
import com.exponential.app.domain.ExpToolGroup
import com.exponential.app.domain.ExpToolRow
import com.exponential.app.domain.ToolResultPreview
import com.exponential.app.domain.ToolGroupSummary
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.BackgroundTask
import com.exponential.app.domain.QUEUE_REMOVE_LABEL
import com.exponential.app.domain.QUEUE_STRIP_TITLE
import com.exponential.app.domain.QueuedMessage
import com.exponential.app.domain.TaskListEntry
import com.exponential.app.domain.WorkflowAgent
import com.exponential.app.domain.WorkflowState
import com.exponential.app.domain.caption
import com.exponential.app.domain.runningWorkflow
import com.exponential.app.domain.ScriptCards
import com.exponential.app.domain.foldedPhases
import com.exponential.app.domain.isScriptCall
import com.exponential.app.domain.laneAgent
import com.exponential.app.domain.scriptCardInput
import com.exponential.app.domain.workCard
import com.exponential.app.domain.workflowDuplicates
import com.exponential.app.domain.splitWorkflowToolRows
import com.exponential.app.domain.WORKFLOW_STATUS_COMPLETED
import com.exponential.app.domain.WORKFLOW_STATUS_FAILED
import com.exponential.app.domain.WORKFLOW_STATUS_STOPPED
import com.exponential.app.domain.workingCaption
import com.exponential.app.domain.WORKFLOW_AGENT_STATE_DONE
import com.exponential.app.domain.WORKFLOW_AGENT_STATE_ERROR
import com.exponential.app.domain.WORKFLOW_AGENT_STATE_RUNNING
import com.exponential.app.domain.COMPACTED_LABEL
import com.exponential.app.domain.COMPACTING_LABEL
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.HistoryState
import com.exponential.app.domain.rateLimitBannerShows
import com.exponential.app.domain.insertImageMarker
import com.exponential.app.domain.SteerMessageSegment
import com.exponential.app.domain.imageMarker
import com.exponential.app.domain.parseSteerMessage
import com.exponential.app.domain.escapeSteerFileName
import com.exponential.app.ui.agent.ComposerPick
import com.exponential.app.ui.agent.ComposerPlusMenuSheet
import com.exponential.app.ui.agent.AgentIssuePickerPage
import com.exponential.app.ui.agent.IssueOption
import com.exponential.app.domain.ComposerMenuConditions
import com.exponential.app.domain.ComposerMenuEntry
import com.exponential.app.domain.ComposerMenuRowId
import com.exponential.app.domain.McpServerOption
import com.exponential.app.domain.mcpPickValue
import com.exponential.app.domain.MAX_STEER_FILES
import com.exponential.app.domain.MAX_STEER_IMAGES
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GlassSwitchSize
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.components.picker.DevicePicker
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.domain.SessionDeviceMove
import com.exponential.app.ui.components.picker.McpServerPicker
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.agent.readComposerPick
import com.exponential.app.domain.steerMessageSegments
import com.exponential.app.domain.renumberImageMarkers
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.imageNumberAt
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.SessionAccountOption
import com.exponential.app.domain.SessionAccountSwitch
import com.exponential.app.domain.SlashCommand
import com.exponential.app.domain.SlashCommands
import com.exponential.app.domain.TranscriptGap
import com.exponential.app.domain.activeQuestionIds
import com.exponential.app.domain.agentWorking
import com.exponential.app.domain.askComplete
import com.exponential.app.domain.BACK_TO_CURRENT_STEP_LABEL
import com.exponential.app.domain.FREE_TEXT_ANSWER_PLACEHOLDER
import com.exponential.app.domain.withFreeTextRow
import com.exponential.app.domain.PLAN_FEEDBACK_PLACEHOLDER
import com.exponential.app.domain.collectSubagents
import com.exponential.app.domain.projectLaneRows
import com.exponential.app.domain.currentStepperStep
import com.exponential.app.domain.FEED_WINDOW
import com.exponential.app.domain.FEED_WINDOW_STEP
import com.exponential.app.domain.groupFeedRows
import com.exponential.app.domain.awaitsAnswer
import com.exponential.app.domain.label
import com.exponential.app.domain.liveToolRowId
import com.exponential.app.domain.localAnswerSummary
import com.exponential.app.domain.planModeBadge
import com.exponential.app.domain.rowClass
import com.exponential.app.domain.transcriptGap
import com.exponential.app.domain.locksCard
import com.exponential.app.domain.visibleSubagentTabs
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SegmentedTrack
import com.exponential.app.ui.components.contextToneColor
import com.exponential.app.ui.components.AgentWorkingMark
import com.exponential.app.ui.components.agentIconPainter
import com.exponential.app.ui.components.agentIconTint
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.ComposerToolButton
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassMenuSurface
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.JumpToBottomButton
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.domain.AskDock as AskDockModel
import com.exponential.app.ui.emoji.rememberEmojiData
import com.exponential.app.ui.emoji.rememberEmojiPrefs
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.markdown.AutocompleteRows
import com.exponential.app.ui.markdown.EMOJI_TYPEAHEAD_LIMIT
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.autocompleteCandidateCount
import com.exponential.app.ui.markdown.autocompleteTriggersAt
import com.exponential.app.ui.markdown.mentionCandidatesFor
import com.exponential.app.ui.markdown.pickAutocompleteAt
import com.exponential.app.ui.markdown.IssueTypeaheadSheetHost
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.withEmoji
import com.exponential.app.ui.markdown.withIssueRef
import com.exponential.app.ui.markdown.withMention
import com.exponential.app.ui.markdown.LocalAppLinks
import com.exponential.app.ui.markdown.LocalAttachmentDims
import com.exponential.app.ui.markdown.rememberAppLinkHandler
import com.exponential.app.ui.markdown.LocalIssueRefBare
import com.exponential.app.ui.markdown.LocalIssueRefs
import com.exponential.app.ui.markdown.LocalMentions
import com.exponential.app.ui.markdown.MentionResolver
import com.exponential.app.ui.markdown.annotate
import com.exponential.app.ui.markdown.LocalInlineCodeStyle
import com.exponential.app.ui.markdown.LocalMarkdownAutolink
import com.exponential.app.ui.markdown.LocalMarkdownBodyStyle
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerRunCaptionRow
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassCard
import com.exponential.app.ui.theme.glassRow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

// The RUN FACE of the phone Work screen (EXP-32 → EXP-893) — a chat-style
// view of a coding session over the relay's scrubbed activity channel. NO
// terminal rendering: narration bubbles, compact tool rows, collapsible
// subagent groups, question steppers, and message-shaped steering (text +
// \r, perm-gated by the relay). The diff is the Changes face; Stop / Resume,
// the title dot and the face tabs belong to the host (`WorkScreen`).
// Identical UX to the iOS RunFace (glass design system).

private val LiveGreen = Color(0xFF34D399)
private val ConnectingYellow = Color(0xFFFBBF24)
// Package-visible: the Agents list paints a paused (offline-machine) session
// with the same neutral grey (EXP-550).
internal val LostGray = Color(0xFF71717A)
/** EXP-550: the one explanation of a paused (offline-machine) session. */
private const val DEVICE_OFFLINE_DETAIL =
    "The agent is paused on that machine and continues when it comes back online."

/** EXP-1051: how long the Usage sheet's transient notice (a refused account
 *  switch) stays up — long enough to read one sentence, short enough that it
 *  never becomes the standing caption it replaced. */

/**
 * EXP-550: the phases whose caption is "we are waiting for the publisher" —
 * exactly the ones an offline host explains. Live and a terminal end are not
 * waiting on anything.
 */
private val AgentPhase.isWaitingForStream: Boolean
    get() = this == AgentPhase.Idle || this == AgentPhase.Connecting ||
        this == AgentPhase.Starting || (this is AgentPhase.Closed && reconnecting)

/**
 * EXP-893: the Work screen's RUN FACE — the transcript directly under the
 * host's top bar, the strips above the composer, and the floating bottom bar
 * (usage ring · `Type / for commands` capsule · the host's trailing circle,
 * EXP-1150: Start coding once the run ended for good) while the run is live;
 * the bare bar with only the trailing circle — or no bar without one — once it
 * ended or a card holds the input. The host (`WorkScreen`) owns the Scaffold,
 * the title dot, Stop / Resume, the kill and resume confirms, the merge
 * (EXP-1191: the glyph-only circle right of this bar's capsule, [mergeBarSlot]) and the
 * ended-edge navigation; this renders INSIDE its
 * content slot, one instance per shown run (`key(shownSessionId)`).
 */
@Composable
fun RunFace(
    viewModel: AgentSessionViewModel,
    padding: PaddingValues,
    // EXP-760: a chipped identifier in the feed opens that issue.
    onOpenIssue: (String) -> Unit,
    /** The bar's right circle (EXP-1150: the Start circle once the run
     *  ended for good); null = none. */
    trailingBarSlot: (@Composable () -> Unit)?,
    /** EXP-1191: the bar's Merge circle, directly right of the capsule and
     *  before [trailingBarSlot]; hidden with the circles while the composer
     *  is expanded. */
    mergeBarSlot: (@Composable () -> Unit)? = null,
    /** EXP-933: switches the host to its Guide face — the inline
     *  `sessions_guide` card's `Open Guide` button. Null hides it. */
    onOpenResults: (() -> Unit)? = null,
) {
    // EXP-1172: the run's synced `coding_sessions.results`, read by a settled
    // `exponential_sessions_show` row to draw its picture; live, so the tile
    // appears the moment the upload syncs.
    val results = viewModel.session.collectAsStateWithLifecycle().value?.guide
    CompositionLocalProvider(
        LocalOpenResults provides onOpenResults,
        LocalSessionResults provides results,
    ) {
        RunFaceContent(viewModel, padding, onOpenIssue, trailingBarSlot, mergeBarSlot)
    }
}

/**
 * EXP-1291: the RAW SESSION VIEW (`session/{sessionId}`) — ONE session's live
 * transcript off the steer relay (lane SESSION owns it), opened from its card
 * in the thread. Header ([SessionHeaderBar]): back · a back row named after
 * the THREAD (→ [onOpenThread], its root) over the session's own name with
 * its state mark · Stop while it is live and the viewer may stop it (shared
 * or own). No face toggle, no Show work, no device caption. The bottom is THE
 * session composer: a message = `sessions.send({sessionId})` — it resumes an
 * idle session and continues an ended one. Keyed by a `sessions` row id.
 */
@Composable
fun AgentSessionScreen(
    sessionId: String,
    onBack: () -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    /** The session's thread (its ROOT), `thread/{rootId}`. */
    onOpenThread: (rootSessionId: String) -> Unit,
) {
    val viewModel = androidx.hilt.navigation.compose.hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
        key = "session:$sessionId",
    ) { factory -> factory.create(sessionId) }
    val session by viewModel.session.collectAsStateWithLifecycle()
    val header by viewModel.header.collectAsStateWithLifecycle()
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()
    var stopConfirmOpen by remember { mutableStateOf(false) }
    val row = session
    val live = row != null && !row.isEnded
    // A shared session lets every member Stop; a private one only its starter
    // (the server re-checks).
    val canStop = live && (row?.shared == true || row?.userId == currentUserId)
    val rootId = row?.rootId?.takeIf { it.isNotEmpty() } ?: row?.id
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            SessionHeaderBar(
                threadTitle = header?.threadTitle,
                sessionName = header?.sessionName ?: row?.title?.takeIf { it.isNotBlank() } ?: "",
                agent = row?.cli,
                state = row?.let { SessionTree.sessionState(SessionTree.session(it)) },
                canStop = canStop,
                onBack = onBack,
                onOpenThread = rootId?.let { id -> { onOpenThread(id) } },
                onStop = { stopConfirmOpen = true },
            )
        },
    ) { padding ->
        RunFace(
            viewModel = viewModel,
            padding = padding,
            onOpenIssue = onOpenIssue,
            trailingBarSlot = null,
        )
    }
    if (stopConfirmOpen) {
        com.exponential.app.ui.components.PromptAlert(
            prompt = com.exponential.app.domain.Prompts.StopRun.prompt(),
            onDismiss = { stopConfirmOpen = false },
            handlers = mapOf(
                "stop" to {
                    stopConfirmOpen = false
                    viewModel.stopSession()
                },
            ),
        )
    }
}

/**
 * EXP-1343: a session chip's page in its THREAD — the raw session view's body
 * ([RunFace]: the session's raw work + THE session composer) without its own
 * [SessionHeaderBar]: the thread's ONE header (the strip + Stop) stays on top.
 * The view model lives in the page's own store (the thread pager's
 * `PageViewModelStore`), so only the shown page holds the live session.
 * [onOpenIssue] = the THREAD's handler (EXP-1347: a linked issue opens its tab).
 */
@Composable
fun SessionPage(
    sessionId: String,
    padding: PaddingValues,
    onOpenIssue: (issueId: String) -> Unit,
) {
    val viewModel = androidx.hilt.navigation.compose.hiltViewModel<AgentSessionViewModel, AgentSessionViewModel.Factory>(
        key = "session:$sessionId",
    ) { factory -> factory.create(sessionId) }
    RunFace(
        viewModel = viewModel,
        padding = padding,
        onOpenIssue = onOpenIssue,
        trailingBarSlot = null,
    )
}

/**
 * EXP-1291: the raw session view's header, stateless. The shared [com.exponential.app.ui.work.WorkTopBar]
 * (back · title · Stop) whose title is two lines: the back row naming the
 * thread (`session-open-thread`, muted, a chevron) over the session's name
 * with its [AgentRunMark] (the ×4 state on the agent's mark).
 */
@Composable
fun SessionHeaderBar(
    threadTitle: String?,
    sessionName: String,
    agent: String?,
    state: SessionTree.SessionState?,
    canStop: Boolean,
    onBack: () -> Unit,
    /** Null hides the back row (no thread resolved yet). */
    onOpenThread: (() -> Unit)?,
    onStop: () -> Unit,
) {
    com.exponential.app.ui.work.WorkTopBar(
        title = sessionName,
        titleContent = {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                if (onOpenThread != null && !threadTitle.isNullOrBlank()) {
                    Row(
                        modifier = Modifier
                            .clip(RoundedCornerShape(6.dp))
                            .clickable(onClick = onOpenThread)
                            .padding(horizontal = 4.dp, vertical = 1.dp)
                            .testTag("session-open-thread"),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(2.dp),
                    ) {
                        Icon(
                            ExpIcons.uiChevronLeft,
                            contentDescription = null,
                            modifier = Modifier.size(12.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                        Text(
                            threadTitle,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    AgentRunMark(
                        agent = agent?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT,
                        state = state,
                        size = 14.dp,
                        badgeSize = 6.dp,
                    )
                    Text(
                        sessionName,
                        style = MaterialTheme.typography.titleSmall,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.testTag("work-title"),
                    )
                }
            }
        },
        onBack = onBack,
        verb = if (canStop) com.exponential.app.ui.work.WorkBarVerb.Stop else null,
        verbEnabled = true,
        onVerb = onStop,
        menu = null,
    )
}

/** EXP-1172: the shown run's raw results blob, for the transcript's inline
 *  pictures. Dynamic: only the rows that read it recompose on a sync. */
private val LocalSessionResults = compositionLocalOf<String?> { null }

/** EXP-933: the host's "show the Guide face" hop, read by the transcript's
 *  `sessions_guide` card deep inside the feed. */
private val LocalOpenResults = staticCompositionLocalOf<(() -> Unit)?> { null }

@Composable
private fun RunFaceContent(
    viewModel: AgentSessionViewModel,
    padding: PaddingValues,
    onOpenIssue: (String) -> Unit,
    trailingBarSlot: (@Composable () -> Unit)?,
    mergeBarSlot: (@Composable () -> Unit)?,
) {
    val session by viewModel.session.collectAsStateWithLifecycle()
    val phase by viewModel.phase.collectAsStateWithLifecycle()
    // Whether the socket is actually up (EXP-621). The banners read the
    // PHASE — a silent redial must not flicker them — but the composer
    // reads this, so its send button never sits enabled over a dead socket.
    val connected by viewModel.connected.collectAsStateWithLifecycle()
    // EXP-549/550: the host machine via its LIVE devices row — the current
    // label, and "offline" = the run is PAUSED on a machine that went away
    // (lid closed). It is not ended and nothing here stops redialing; only
    // what the screen says changes, and it resumes when the machine returns.
    val hostDevice by viewModel.hostDevice.collectAsStateWithLifecycle()
    val hostOffline by viewModel.hostDeviceOffline.collectAsStateWithLifecycle()
    val activity by viewModel.activity.collectAsStateWithLifecycle()
    // EXP-773: an ENDED run's transcript is republished by the machine that
    // ran it — this is that fetch's status.
    val history by viewModel.history.collectAsStateWithLifecycle()
    val feed = activity.feed
    val latestDiff = activity.latestDiff
    val killError by viewModel.killError.collectAsStateWithLifecycle()
    // FEED-79 (EXP-1259 D10): a failed kill call (EXP-268) is the error toast
    // with Retry — success needs nothing: the synced row flips to ended.
    val toaster = LocalToaster.current
    LaunchedEffect(killError) {
        val failure = killError ?: return@LaunchedEffect
        val title = FeedbackStates.errorTitle("stop", "the session")
        toaster.error(
            title,
            failure.takeIf { it != title },
            ToastAction(FeedbackStates.RETRY) { viewModel.stopSession() },
        )
        viewModel.clearKillError()
    }
    val attachmentDims by viewModel.attachmentDims.collectAsStateWithLifecycle()
    // EXP-760: the run's team issues, newest-first — resolves the identifiers
    // the agent names in the feed (IssueFace's handler, same shape).
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    val currentOnOpenIssue by rememberUpdatedState(onOpenIssue)
    // EXP-920: the transcript's entity-preview sheets read the run's team
    // rows off the account DB (the viewer id scopes notifications).
    val currentUserId by viewModel.currentUserId.collectAsStateWithLifecycle()
    val entityRefResolver = remember(viewModel, session?.teamId, currentUserId) {
        EntityRefResolver(viewModel.accountDb, session?.teamId, currentUserId)
    }
    val issueRefHandler = remember(issueRefCandidates) {
        IssueRefHandler(
            issueRefCandidates,
            searchServer = viewModel::searchIssueRefs,
        ) { target -> currentOnOpenIssue(target.issueId) }
    }
    // EXP-1188: links the agent writes to this instance open in the app.
    val appLinkHandler = rememberAppLinkHandler()
    val answerStates = activity.answerLocks
    // EXP-588: per locked card, what this client picked — joined for display.
    val answerLabels = remember(activity.answerLocks, activity.answerLabels) {
        activity.answerLabels.keys.mapNotNull { key ->
            activity.localAnswerSummary(key)?.let { key to it }
        }.toMap()
    }
    val pendingImages by viewModel.pendingImages.collectAsStateWithLifecycle()
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
    // EXP-849 phase 3: the logins this run could continue under, and everything
    // the switch gates on but the turn slot (which is right here in `activity`).
    val accountSwitch by viewModel.accountSwitch.collectAsStateWithLifecycle()
    val switchingAccount by viewModel.switchingAccount.collectAsStateWithLifecycle()
    // EXP-849: a refused switch = `Couldn't switch the account`.
    val switchError by viewModel.switchError.collectAsStateWithLifecycle()
    com.exponential.app.ui.components.FailedActionToast(
        switchError?.let { com.exponential.app.ui.components.FailedAction("switch", "the account", it.ifBlank { null }) },
        onShown = viewModel::clearSwitchError,
    )
    // EXP-746: the agent's live configuration (the composer footer + the run's
    // own `/` commands) and its context/spend meter. Both null on a PTY run,
    // which publishes neither.
    val sessionConfig by viewModel.sessionConfig.collectAsStateWithLifecycle()
    val sessionUsage by viewModel.sessionUsage.collectAsStateWithLifecycle()
    // EXP-1051: …and how that window is laid out — the device's attribution of
    // what it put in the context before the first turn. Null until a device
    // publishes one; the bar then draws the conversation alone.
    val sessionContextLayout by viewModel.sessionContextLayout.collectAsStateWithLifecycle()
    // EXP-746: the `/` hint counts the MERGED catalog — an agent that
    // advertises commands has a menu even if the contract had none for it.
    // An agent-less run that publishes a `config_state` is an EXTERNAL agent
    // (`SlashCommands.agentId`), which has no contract rows at all.
    val catalogAgent = SlashCommands.agentId(session?.cli, sessionConfig != null)

    // Attaching revives the connection (EXP-625): since EXP-621 a return visit
    // re-attaches to the running one and its feed is already there, but a dial
    // loop that died while the screen was away is redialed here instead of
    // leaving the screen on a spinner nothing could clear. Foreground and
    // network revivals are the store's job, not this screen's.
    LaunchedEffect(Unit) { viewModel.ensureConnected() }
    // EXP-1323: opening a session view reads it (bold rows + the nav dot);
    // news landing while it is open is read too, like the thread screen.
    val newestAgentMessage by viewModel.newestAgentMessageAt.collectAsStateWithLifecycle()
    LaunchedEffect(viewModel.codingSessionId, newestAgentMessage) { viewModel.markRead() }
    // EXP-888: a sweep end is not an end — the composer stays live.
    val sessionEnded = session?.isEnded == true
    // A trailing question/plan means the session is blocked on a human — the
    // host's title dot flips amber so it never looks silently stuck.
    val awaitingInput = phase == AgentPhase.Live &&
        remember(feed) { activeQuestionIds(feed) }.isNotEmpty()
    // EXP-389/848: the agent is actively working. ONE rule, shared ×4
    // (`agentWorking`): a live, unended run MID-TURN with nothing parked on it —
    // no active question card, synced needs_input clear, no usage wall, not
    // compacting. Drives the busy footer AND the composer's Stop glyph
    // (EXP-790).
    val agentWorking = agentWorking(
        live = phase == AgentPhase.Live,
        sessionEnded = sessionEnded,
        turnState = activity.turnState,
        awaitingInput = awaitingInput,
        needsInput = session?.needsInput == true,
        blocked = AgentUsagePresentation.parseBlocked(session?.blocked) != null,
        compacting = activity.compacting != null,
    )
    // ── EXP-1291: the status row — the ×4 session state + where it runs ────
    val cardState = session?.let { SessionTree.sessionState(SessionTree.session(it)) }
    val rowPaused = !sessionEnded && cardState != null && hostDevice.isPaused(cardState)
    val startingRun = session != null && session?.userId == currentUserId &&
        !sessionEnded && !rowPaused && feed.isEmpty() &&
        activity.turnState != TURN_STATE_STARTED && history == null &&
        phase !is AgentPhase.Closed && phase !is AgentPhase.Ended
    val statusRow: @Composable () -> Unit = {
        ReadingColumn {
            Row(
                modifier = Modifier.padding(vertical = 8.dp).testTag("run-status-row"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                AgentRunMark(
                    agent = session?.cli?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT,
                    state = cardState,
                    size = 14.dp,
                    badgeSize = 6.dp,
                )
                // EXP-1291: the state word alone — no device caption.
                Text(
                    cardState?.let { SessionTree.CardCopy.STATES[it] }.orEmpty(),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
    }
    // EXP-850 (S5/S7): the NEWEST running workflow — its caption is what the
    // working row says while it runs. Only while the run is LIVE: a replayed
    // transcript can end with a workflow frozen mid-flight, and an ended run
    // must never claim to be working on one.
    val workflowCaption = remember(activity.workflows, phase, sessionEnded) {
        if (phase == AgentPhase.Live && !sessionEnded) activity.runningWorkflow()?.caption() else null
    }
    // EXP-850 (S5): whose brand mark pulses beside the working caption. A run
    // that named no agent is claude (the device default, EXP-484).
    val workingAgent = session?.cli?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT
    // EXP-850 (S1/S2): the strip above the composer — one line per background
    // task the machine is running, plus one per still-open `wait` tool row.
    // EXP-927: a task of kind `agent` is a subagent, and a subagent is a
    // conversation TAB — the strip never lists it.
    // EXP-1321/1324: all three — the agent's own task list, the background
    // tasks and the open waits — are THE Queue now (`TaskQueues.of`, fixture
    // task-queue.json ×4); its folds stick for the session.
    var queueTodoOpen by rememberSaveable { mutableStateOf(true) }
    var queueCompletedOpen by rememberSaveable { mutableStateOf(false) }
    // EXP-790: the composer folds to the bar's capsule while it is unfocused
    // and empty (the issue's comment bar rule); a restored draft opens it.
    var composerExpanded by rememberSaveable {
        mutableStateOf(draft.isNotBlank() || pendingImages.isNotEmpty())
    }

    // EXP-784/831: the 30s clock the rate-limit banner ages on.
    val live = phase == AgentPhase.Live
    var nowMs by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(live) {
        if (!live) return@LaunchedEffect
        while (true) {
            nowMs = System.currentTimeMillis()
            delay(STALE_ACTIVITY_TICK_MS)
        }
    }

    // EXP-688/893: the Usage sheet, opened by the bar's ring (and the wall's
    // "Switch account" pill).
    var usageSheetOpen by remember { mutableStateOf(false) }
    // EXP-1051/EXP-1031: a refused account switch says why when its disabled
    // control is tapped, as an error toast — the toast window draws above the
    // Usage sheet, so it is no longer an inline line inside it. (`toaster`
    // is held above, beside the kill toast.)

    // EXP-656: the reader's place in the feed lives at the FACE level, not
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


    // EXP-1162: the transcript runs UNDER the header band and the bottom
    // band (banners, strips, menus, the composer), which FLOATS over its tail
    // like iOS's `safeAreaInset(edge: .bottom)`: the list's bottom content
    // padding tracks the band's measured height, so the last row clears it
    // and follow-mode still lands on the true end.
    val layoutDirection = LocalLayoutDirection.current
    val topInset = padding.calculateTopPadding()
    val bottomInset = padding.calculateBottomPadding()
    var bandHeightPx by remember { mutableIntStateOf(0) }
    val bandClearance = with(LocalDensity.current) { bandHeightPx.toDp() } + bottomInset
    // The placeholder states (no feed yet) centre in the space between the
    // bands; the feed itself scrolls under both.
    val feedPlaceholder = feed.isEmpty() && (
        history != null ||
            (hostOffline && phase.isWaitingForStream) ||
            phase == AgentPhase.Connecting || phase == AgentPhase.Starting ||
            (!everRendered && phase == AgentPhase.Live && latestDiff == null)
        )
    // Every answer is one semantic `answer` frame keyed by the card's wire
    // id (EXP-249) — shared by the transcript and the thread (EXP-1175).
    val onAnswer: (AgentFeedItem.Question, List<String>, String?) -> Unit = { question, keys, text ->
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
            question.wireId, question.askId, keys, text, labels,
        )
    }
    // EXP-820: a plan rejected with feedback — the reject key answers the
    // card, the text follows as the next message (iOS/web parity).
    val onPlanFollowUp: (AgentFeedItem.Question, String, String) -> Unit = { question, key, text ->
        val label = question.options.firstOrNull { it.key == key }?.label ?: key
        viewModel.answerThenSend(
            question.wireId, question.askId, listOf(key), listOf(label), text,
        )
    }
    // THE ask dock (ask-dock.json): the card waiting on this viewer leaves
    // the transcript and takes the composer's slot (the session view feeds
    // the dock its ONE answerable card, so it is always `single`).
    val dockable = phase == AgentPhase.Live && !sessionEnded
    val toolAsks by viewModel.toolAsks.collectAsStateWithLifecycle()
    val dockActive = remember(feed) { activeQuestionIds(feed) }
    val dockedRow = remember(feed, dockActive, dockable) {
        if (!dockable || dockActive.isEmpty()) null else dockedAskRow(groupFeedRows(feed), dockActive)
    }
    val dockCards = AnswerCards(
        answered = remember(answerStates) { answerStates.filterValues { it.locksCard() }.keys },
        activeQuestionIds = dockActive,
        answerEnabled = dockable,
        answerStates = answerStates,
        answerLabels = answerLabels,
        onAnswer = onAnswer,
        onPlanFollowUp = onPlanFollowUp,
    )
    val dock: (@Composable () -> Unit)? = dockedRow?.let { row ->
        {
            val pending = AskDockModel.Pending(
                id = row.id.toString(),
                kind = if ((row as? AgentFeedRow.Single)?.item.let { it as? AgentFeedItem.Question }?.planMode == true) {
                    AskDockModel.Kind.Plan
                } else {
                    AskDockModel.Kind.Question
                },
                text = "",
                sessionName = null,
                createdAt = 0L,
            )
            AskDock(
                view = AskDockModel.askDock(listOf(pending), null),
                onFocus = {},
                onBack = {},
            ) { AnswerCardRow(row, dockCards) }
        }
    } ?: toolAsks.takeIf { it.isNotEmpty() && !sessionEnded }?.let { asks ->
        // EXP-1280: a card a TOOL posted (plan_propose / sessions_ask) has no
        // relay ask: THE rich card answers it through `sessions.answer`.
        {
            var focusedAsk by remember { mutableStateOf<String?>(null) }
            val byId = remember(asks) { asks.associateBy { it.id } }
            AskDock(
                view = AskDockModel.askDock(AskDockModel.threadPendingAsks(asks) { null }, focusedAsk),
                onFocus = { focusedAsk = it },
                onBack = { focusedAsk = null },
            ) { id ->
                byId[id]?.let { row ->
                    com.exponential.app.ui.thread.ThreadAskCard(
                        row = row,
                        onAnswer = { messageId, a, settled ->
                            viewModel.answerCard(
                                AnswerInput(
                                    messageId = messageId,
                                    answer = a.answer,
                                    answers = a.answers?.let(AnswerInput::questionAnswers),
                                    surfaceData = a.surfaceData,
                                ),
                                "question",
                                settled,
                            )
                        },
                        onDecidePlan = { messageId, d, settled ->
                            viewModel.answerCard(
                                AnswerInput(
                                    messageId = messageId,
                                    decision = if (d.approved) "approved" else "rejected",
                                    fresh = if (d.approved && d.fresh) true else null,
                                    feedback = d.feedback,
                                    answers = d.answers?.let(AnswerInput::planAnswers),
                                    surfaceData = d.surfaceData,
                                ),
                                "plan",
                                settled,
                            )
                        },
                    )
                }
            }
        }
    }
    // The placeholder states (no feed yet): connecting, a paused host, an
    // ended run's history fetch, an old desktop.
    val placeholderBody: @Composable () -> Unit = {
        when {
            // EXP-773: a finished run's transcript lives on the
            // machine that ran it, and the relay is asking that
            // machine for it. Say which machine, and say plainly when
            // it cannot answer — this is not a connection problem the
            // viewer can wait out.
            // EXP-1259 D8.3: the transcript's first load = placeholder rows
            // in its own column, the phase as a muted caption.
            feed.isEmpty() && history == HistoryState.Pending ->
                TranscriptPlaceholder(historyStatus(history, hostDevice.displayLabel))
            feed.isEmpty() && history != null -> CenteredState {
                Text(
                    historyStatus(history, hostDevice.displayLabel),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    textAlign = TextAlign.Center,
                )
            }
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
            // EXP-1271: the status row above says "Starting on …".
            startingRun -> Unit
            feed.isEmpty() && (phase == AgentPhase.Connecting || phase == AgentPhase.Starting) ->
                TranscriptPlaceholder(
                    if (phase == AgentPhase.Starting) {
                        "The agent is starting. Waiting for the live stream…"
                    } else {
                        "Connecting…"
                    },
                )
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
            else -> Unit
        }
    }
    val bodyPlaceholder = feedPlaceholder
    Box(
        modifier = Modifier
            .padding(
                start = padding.calculateStartPadding(layoutDirection),
                end = padding.calculateEndPadding(layoutDirection),
            )
            // The bottom band sits on the nav bar (its own padding below);
            // consuming it keeps imePadding from re-adding it, so the band
            // lands exactly on the keyboard (EXP-336).
            .consumeWindowInsets(PaddingValues(bottom = bottomInset))
            .fillMaxSize()
            .imePadding(),
    ) {
        // EXP-440: the feed renders markdown. Autolink is on — it
        // is a render-only surface, so bare URLs may become links
        // without diverging any stored bytes — and embedded images
        // pre-size from the linked issue's probed attachments.
        CompositionLocalProvider(
            LocalMarkdownAutolink provides true,
            LocalAttachmentDims provides attachmentDims,
            // EXP-760: an identifier the agent names is a chip that
            // opens the issue. Resolution is scoped to the RUN's
            // team (a batch / action / chat run has no issue), and
            // BARE `EXP-758` chips here too — this is the one
            // surface agents write, and they omit the `#`.
            // Display-only: the composer below is outside this
            // provider, so what the user types stays the stored
            // `#IDENTIFIER` contract.
            LocalIssueRefs provides issueRefHandler,
            LocalIssueRefBare provides true,
            // EXP-920: what a tool row's entity chips resolve
            // their preview sheets against — the run's team.
            LocalEntityRefResolver provides entityRefResolver,
            // EXP-698: inline `code` is TINTED in a chat feed —
            // narration, the user's own bubbles, plan and ask
            // cards, everything under this provider. Issue
            // descriptions and comments keep the flat wash.
            LocalInlineCodeStyle provides MdStyle.Chat,
            // EXP-787: every markdown surface in the transcript —
            // narration, the user's own bubbles, a plan card —
            // reads at the shared body step (14 sp on a 22 sp
            // line), not the 17 sp document measure an issue
            // description uses.
            LocalMarkdownBodyStyle provides TranscriptBodyStyle,
            // EXP-1188: a link to an issue or run on this instance opens
            // in the app; anything else stays a browser link.
            LocalAppLinks provides appLinkHandler,
            // EXP-1241: a comment call's card names the run's agent and its
            // owner (the author until the comment row syncs).
            LocalRunCards provides RunCards(
                agent = session?.cli?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT,
                ownerId = session?.userId,
            ),
        ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 12.dp)
                // The store-screenshot test waits on this tag so it never
                // captures "Connecting…" / "Waiting for activity…" — the
                // Run face body in BOTH modes, once it has something to show.
                .then(if (bodyPlaceholder) Modifier else Modifier.testTag("agent-feed")),
        ) {
            // EXP-893: the diff is the Changes face and Merge lives in the
            // header. An ended run's byline heads the feed as its first row
            // (so it scrolls with it), or sits above a placeholder.
            // EXP-1175: the status row sits on top in both modes.
            if (bodyPlaceholder) {
                Spacer(Modifier.height(topInset))
                statusRow()
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .weight(1f)
                        .padding(bottom = bandClearance),
                ) { placeholderBody() }
            } else {
                ActivityFeed(
                    feed = feed,
                    live = phase == AgentPhase.Live,
                    // Hoisted (EXP-656) — see the declarations above.
                    listState = feedListState,
                    follow = follow,
                    onFollowChange = { follow = it },
                    agentTab = agentTab,
                    onAgentTabChange = { agentTab = it },
                    // EXP-783: past the feed's own first row, the next
                    // page comes off the device's journal.
                    onCanLoadEarlier = { viewModel.canLoadEarlier() },
                    onLoadEarlier = { viewModel.loadEarlier() },
                    working = agentWorking,
                    // EXP-850 (S5): the working caption's inputs.
                    turnStartedAt = activity.turnStartedAt,
                    turnTokens = activity.turnTokens,
                    workflowCaption = workflowCaption,
                    workingAgent = workingAgent,
                    // EXP-850 (S3): the cards, looked up by the id of
                    // the `Workflow` tool row they replace.
                    workflows = activity.workflows,
                    // Question cards are answerable while live (EXP-78;
                    // live implies ownership since EXP-312); the card
                    // itself also checks its own state.
                    answerEnabled = phase == AgentPhase.Live && !sessionEnded,
                    answerStates = answerStates,
                    answerLabels = answerLabels,
                    // EXP-724: filters the command pill's catalog.
                    agent = catalogAgent,
                    // Every answer is one semantic `answer` frame keyed
                    // by the card's wire id (EXP-249).
                    onAnswer = onAnswer,
                    onPlanFollowUp = onPlanFollowUp,
                    // EXP-1162: the floating bottom band's height
                    // (+ the nav bar) clears the tail; the header
                    // band's height clears the head.
                    bottomInset = bandClearance,
                    topInset = topInset,
                    statusRow = statusRow,
                    header = {},
                    docked = dockedRow,
                )
            }
        }
        }

        // ── The bottom band: floats over the transcript's tail
        FloatingBarEdge(
            bottomInset = bottomInset,
            // An expanded composer card is its own surface.
            visible = !composerExpanded,
            // EXP-1256: the thread's bottom edge in the page's own ground.
            ground = true,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .padding(bottom = bottomInset),
        ) {
        // THE session composer (shared with the thread + the agent channel);
        // the face hands it its banners and the strips around the menus.
        SessionComposer(
            viewModel = viewModel,
            expanded = composerExpanded,
            onExpandedChange = { composerExpanded = it },
            onOpenUsage = { usageSheetOpen = true },
            trailing = trailingBarSlot,
            merge = mergeBarSlot,
            dock = dock,
            modifier = Modifier.onSizeChanged { if (it.height != bandHeightPx) bandHeightPx = it.height },
            banners = {
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
                                // EXP-1259 D8.1: the band's inline spinner.
                                InlineSpinner()
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
                                // EXP-1259 D8.1: the band's inline spinner.
                                InlineSpinner()
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


                // EXP-804 / NEW-1: the PERSISTED block off the session row
                // (web `session-blocked-strip`): a usage wall reads `Rate
                // limited · resets in …` with Switch account; a watchdog flag
                // (device offline, not picked up, stuck) its card word, no
                // countdown and no switch.
                val persistedBlock = AgentUsagePresentation.parseBlocked(session?.blocked)
                val blockedLabel = AgentUsagePresentation.blockedBadgeLabel(persistedBlock, nowMs)
                if (blockedLabel != null) {
                    ReadingColumn {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(vertical = 6.dp)
                                .testTag("session-blocked-strip"),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Text(
                                blockedLabel,
                                style = MaterialTheme.typography.labelMedium,
                                color = DesignTokens.Semantic.Yellow,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.weight(1f),
                            )
                            if (session?.isEnded != true &&
                                AgentUsagePresentation.isUsageWall(persistedBlock) &&
                                accountSwitch.canSwitchAny(activity.turnState)
                            ) {
                                GlassPill(
                                    SessionAccountSwitch.WALL_SWITCH_LABEL,
                                    onClick = { usageSheetOpen = true },
                                    size = PillSize.Sm,
                                    mode = PillMode.Action,
                                    icon = ExpIcons.uiSwap,
                                    modifier = Modifier.testTag("blocked-switch-account"),
                                )
                            }
                        }
                    }
                }

                // EXP-784: the agent's rate-limit window — ONE banner from the
                // latest-wins slot (never a run of identical feed rows), with how
                // long until the reset when the agent named one. Gone the
                // moment an empty/`ok` status clears the slot; never up for a
                // bare warning (EXP-818) and, on the 30s clock, dropped a minute
                // past the reset it named (EXP-831).
                val rateLimit = activity.rateLimit
                if (rateLimit != null && phase !is AgentPhase.Ended &&
                    rateLimitBannerShows(rateLimit, nowMs)
                ) {
                    val caption = rateLimitCaption(rateLimit.message, rateLimit.resetsAt, nowMs)
                    // Web `RateLimitBanner` is the reference: ONE row, 6dp gaps,
                    // centered — the usage glyph + the message in the semantic
                    // yellow (one truncating line), the relative countdown as a
                    // separate muted caption, then the Switch account pill.
                    ReadingColumn {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .glassRow()
                                .padding(horizontal = 12.dp, vertical = 10.dp)
                                .testTag("agent-rate-limit")
                                .semantics(mergeDescendants = true) {
                                    contentDescription = caption.accessibilityLabel
                                },
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            Icon(
                                ExpIcons.uiUsage,
                                contentDescription = null,
                                modifier = Modifier.size(12.dp),
                                tint = DesignTokens.Semantic.Yellow,
                            )
                            Text(
                                caption.message,
                                style = MaterialTheme.typography.labelMedium,
                                color = DesignTokens.Semantic.Yellow,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.weight(1f, fill = false),
                            )
                            Text(
                                caption.detail,
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                softWrap = false,
                            )
                            Spacer(Modifier.weight(1f))
                            // EXP-849: the PRIMARY move on a usage wall is another
                            // account, not waiting for the reset — the button opens the
                            // account rows (with their own numbers) and the switch
                            // clears this notice by continuing the run over there.
                            // Absent unless a switch is actually possible, so the wall
                            // never offers a dead end.
                            if (accountSwitch.canSwitchAny(activity.turnState)) {
                                GlassPill(
                                    SessionAccountSwitch.WALL_SWITCH_LABEL,
                                    onClick = { usageSheetOpen = true },
                                    size = PillSize.Sm,
                                    mode = PillMode.Action,
                                    icon = ExpIcons.uiSwap,
                                    primary = true,
                                    modifier = Modifier.testTag("rate-limit-switch-account"),
                                )
                            }
                        }
                    }
                    Spacer(Modifier.height(8.dp))
                }

                // EXP-724: the agent is compacting its context — an indeterminate
                // strip, never a feed row, so the 10-170s of silence has a visible
                // reason. It closes on `compaction ended` (which leaves the quiet
                // marker in the feed), on the connection's 180s backstop, and with
                // the session itself.
                if (activity.compacting != null && phase !is AgentPhase.Ended) {
                    ReadingColumn {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .glassRow()
                                .padding(horizontal = 12.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            Icon(
                                ExpIcons.codingCompact,
                                contentDescription = null,
                                modifier = Modifier.size(14.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(
                                    alpha = TextEmphasis.Secondary,
                                ),
                            )
                            Column(
                                modifier = Modifier.weight(1f),
                                verticalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                Text(
                                    COMPACTING_LABEL,
                                    style = MaterialTheme.typography.labelMedium,
                                    color = MaterialTheme.colorScheme.onSurface,
                                )
                                LinearProgressIndicator(
                                    modifier = Modifier.fillMaxWidth().height(3.dp),
                                    color = Color.White,
                                    trackColor = Color.White.copy(alpha = 0.12f),
                                )
                            }
                        }
                    }
                    Spacer(Modifier.height(8.dp))
                }

                // EXP-1321/1324: THE Queue — the agent's tasks, its background
                // work and its open waits as two Trace folds, the last block
                // before the queued messages. Same live gate the strips had: a
                // finished run has nothing left to do.
                if (phase == AgentPhase.Live && !sessionEnded) {
                    SessionTaskQueue(
                        feed = feed,
                        taskList = activity.taskList,
                        backgroundTasks = activity.backgroundTasks,
                        todoOpen = queueTodoOpen,
                        onTodoOpenChange = { queueTodoOpen = it },
                        completedOpen = queueCompletedOpen,
                        onCompletedOpenChange = { queueCompletedOpen = it },
                    )
                }
            },
        )
        }
    }

    // ── The Usage overlay — THE sheet, shared with the thread's composer ring.
    SessionUsageSheet(
        viewModel = viewModel,
        open = usageSheetOpen,
        onDismiss = { usageSheetOpen = false },
    )
}

/**
 * THE Usage sheet (EXP-688/EXP-909) behind a session composer's ring and the
 * rate-limit wall's "Switch account": the run's account, its windows, the
 * context window and the other logins to switch to. The session view and the
 * thread / agent channel (the ROOT's view model) open the same one.
 */
@Composable
internal fun SessionUsageSheet(
    viewModel: AgentSessionViewModel,
    open: Boolean,
    onDismiss: () -> Unit,
) {
    val activity by viewModel.activity.collectAsStateWithLifecycle()
    val agentUsage by viewModel.agentUsage.collectAsStateWithLifecycle()
    val agentAccount by viewModel.agentAccount.collectAsStateWithLifecycle()
    val accountSwitch by viewModel.accountSwitch.collectAsStateWithLifecycle()
    val switchingAccount by viewModel.switchingAccount.collectAsStateWithLifecycle()
    val sessionUsage by viewModel.sessionUsage.collectAsStateWithLifecycle()
    val sessionContextLayout by viewModel.sessionContextLayout.collectAsStateWithLifecycle()
    val toaster = LocalToaster.current
    // EXP-1051: the context-window legend is folded by default — the bar and
    // its headline are the glance, the per-layer numbers the follow-up.
    var contextLayoutExpanded by rememberSaveable { mutableStateOf(false) }
    // The header's brand mark: the run's agent (claude when it named none).
    val session by viewModel.session.collectAsStateWithLifecycle()
    val workingAgent = session?.cli?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT
    // ── The Usage overlay (EXP-688/EXP-909) — behind the bar's ring
    // (EXP-893). It retires with the run: an ended row reports no windows, so
    // nothing here has anything to show. Stale numbers are NOT a reason to
    // hide it — they dim and date themselves instead.
    val sheetUsage = agentUsage
    // EXP-746: this run's own context/spend meter — a second, independent
    // source, so the sheet stays reachable while EITHER has something.
    val contextUsage = sessionUsage
    val switchOptions = accountSwitch.options
    LaunchedEffect(sheetUsage, contextUsage, switchOptions) {
        if (sheetUsage == null && contextUsage == null && switchOptions.isEmpty()) {
            onDismiss()
        }
    }
    // EXP-909: one on-demand refresh of the run's login when the overlay
    // opens — the machine answers by re-reporting, which arrives through sync.
    // Once per open; the VM refuses it when there is nothing to ask.
    LaunchedEffect(open) {
        if (open) viewModel.refreshRunUsage()
    }
    if (open && (sheetUsage != null || contextUsage != null || switchOptions.isNotEmpty())) {
        // EXP-909: THE overlay, identical ×4 — the run's account as the
        // header, its windows, the Context line, then only the OTHER accounts.
        val nowMs = rememberUsageClock()
        val runAccount = switchOptions.firstOrNull { it.current }
        val otherAccounts = switchOptions.filter { it != runAccount }
        // Whose limits the header names — read one block early, so the context
        // window above it knows whether anything follows it (EXP-1051).
        val headerCaption = runAccount?.caption
            ?: agentAccount?.let(AgentUsagePresentation::accountCaption)
        GlassSheet(title = "Usage", onDismiss = onDismiss) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    // EXP-698: 16dp — the group inset every other sheet uses.
                    .padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // ── EXP-1051: where this run's context window actually GOES.
                // FIRST, above the machine's plan limits: the window is the
                // thing the person can act on (compact, start fresh), and the
                // layers say whether acting would even help. Folded to the bar
                // + headline; the legend is the tap.
                val contextWindow = ContextLayoutPresentation.contextWindowView(
                    contextUsage,
                    sessionContextLayout,
                )
                if (contextWindow != null) {
                    val fg = MaterialTheme.colorScheme.onSurface
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("session-context-window"),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { contextLayoutExpanded = !contextLayoutExpanded },
                        ) {
                            Text(
                                ContextLayoutPresentation.TITLE.uppercase(),
                                style = MaterialTheme.typography.labelSmall,
                                color = fg.copy(alpha = TextEmphasis.Tertiary),
                            )
                            Spacer(Modifier.weight(1f))
                            // The spend rides beside the numbers, where the
                            // old one-line Context block carried it.
                            AgentUsagePresentation.formatUsageCost(contextUsage)?.let { cost ->
                                Text(
                                    cost,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = fg.copy(alpha = TextEmphasis.Tertiary),
                                    maxLines = 1,
                                )
                                Spacer(Modifier.width(8.dp))
                            }
                            Text(
                                contextWindow.headline,
                                style = MaterialTheme.typography.bodyMedium,
                                color = fg,
                                maxLines = 1,
                            )
                            Spacer(Modifier.width(6.dp))
                            Icon(
                                if (contextLayoutExpanded) {
                                    ExpIcons.uiChevronDown
                                } else {
                                    ExpIcons.uiChevronRight
                                },
                                contentDescription = null,
                                modifier = Modifier.size(12.dp),
                                tint = fg.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                        SegmentedTrack(
                            slices = contextWindow.bar.map { slice ->
                                contextToneColor(slice.tone, fg) to slice.percent
                            },
                            ticks = contextWindow.ticks,
                        )
                        if (contextLayoutExpanded) {
                            contextWindow.legend.forEach { row ->
                                Column(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalArrangement = Arrangement.spacedBy(2.dp),
                                ) {
                                    Row(
                                        verticalAlignment = Alignment.CenterVertically,
                                        modifier = Modifier.fillMaxWidth(),
                                    ) {
                                        Box(
                                            modifier = Modifier
                                                .size(10.dp)
                                                .clip(RoundedCornerShape(3.dp))
                                                .background(contextToneColor(row.tone, fg)),
                                        )
                                        Spacer(Modifier.width(8.dp))
                                        Text(
                                            row.label,
                                            style = MaterialTheme.typography.bodySmall,
                                            color = fg.copy(alpha = TextEmphasis.Secondary),
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                            modifier = Modifier.weight(1f),
                                        )
                                        Text(
                                            // `≈` on every layer the device
                                            // guessed rather than counted.
                                            if (row.estimated) "≈${row.tokens}" else row.tokens,
                                            style = MaterialTheme.typography.labelSmall,
                                            color = fg.copy(alpha = TextEmphasis.Tertiary),
                                            maxLines = 1,
                                        )
                                        Spacer(Modifier.width(10.dp))
                                        Text(
                                            row.percent,
                                            style = MaterialTheme.typography.labelSmall,
                                            color = fg.copy(alpha = TextEmphasis.Secondary),
                                            maxLines = 1,
                                        )
                                    }
                                    // What the layer is made of, when the
                                    // device named it (`project`'s CLAUDE.md
                                    // list) — under the label, not beside it.
                                    row.detail?.let { detail ->
                                        Text(
                                            detail,
                                            style = MaterialTheme.typography.labelSmall,
                                            color = fg.copy(alpha = TextEmphasis.Quaternary),
                                            maxLines = 2,
                                            overflow = TextOverflow.Ellipsis,
                                            modifier = Modifier.padding(start = 18.dp),
                                        )
                                    }
                                }
                            }
                        }
                    }
                    if (headerCaption != null || sheetUsage != null) GroupDivider()
                }
                // ── Header: whose limits these are. The brand mark says the
                // agent, the caption the login, and the trailing slot carries
                // the health badge when there is one — else the plan, which is
                // then the only place the plan string appears.
                if (headerCaption != null) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(
                            agentIconPainter(workingAgent),
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = agentIconTint(workingAgent),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            headerCaption,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                        val headerBadge = runAccount
                            ?.let { AgentHealthRules.badgeLabel(it.health) }
                        val headerPlan = runAccount?.plan
                            ?.takeIf { it != headerCaption }
                            ?: agentAccount?.plan?.takeIf { it != headerCaption }
                        when {
                            headerBadge != null -> Text(
                                headerBadge,
                                style = MaterialTheme.typography.labelSmall,
                                color = NeedsInputAmber,
                                maxLines = 1,
                            )
                            headerPlan != null -> Text(
                                headerPlan,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(
                                    alpha = TextEmphasis.Tertiary,
                                ),
                                maxLines = 1,
                            )
                        }
                    }
                }
                // ── The run's own windows. "Checking…" while its login has
                // reported none yet: a machine still working must not read as
                // a machine with nothing to say.
                if (sheetUsage != null && sheetUsage.windows.isNotEmpty()) {
                    UsageWindows(usage = sheetUsage)
                } else if (headerCaption != null) {
                    Text(
                        "Checking…",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
                // EXP-1051: the old one-line "Context" block (title · numbers ·
                // cost + a single-fill meter) is GONE — the block at the top of
                // this sheet is the same reading, broken down.
                // ── EXP-849 phase 3 / EXP-909: the OTHER logins this machine
                // holds for the agent — the readout IS the switch control.
                // Claude only, idle only, owner only; a refused row keeps its
                // button and says why rather than hiding it mid-run.
                if (otherAccounts.isNotEmpty()) {
                    GroupDivider()
                    Text(
                        SessionAccountSwitch.SECTION_TITLE.uppercase(),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                    otherAccounts.forEach { option ->
                        SessionAccountRow(
                            option = option,
                            refusal = accountSwitch.refusal(option, activity.turnState),
                            switching = switchingAccount == option.profileId,
                            nowMs = nowMs,
                            // EXP-849: `sessions.send({sessionId, account})`;
                            // the host resumes the same row on that login.
                            onSwitch = {
                                viewModel.switchAccount(option)
                                onDismiss()
                            },
                            // EXP-1051: a refused row no longer carries its
                            // reason as a permanent caption — tapping the
                            // disabled control says it, once.
                            onRefused = { toaster.error(it) },
                        )
                    }
                }
                Spacer(Modifier.height(8.dp))
            }
        }
    }
}

// ── Status captions (FEED-26; the host's title dot reads `sessionDotTone`) ──

/**
 * FEED-26: a live run whose feed has been silent this long stops reading as a
 * healthy "Live" — the caption counts the quiet minutes and the dot goes the
 * steady amber of "Needs your input". Byte-identical across the four clients.
 */
internal const val STALE_ACTIVITY_AFTER_MS = 10L * 60L * 1000L

/** FEED-26: how often the quiet clock ticks, so the minute count moves on its
 *  own without a feed event. */
private const val STALE_ACTIVITY_TICK_MS = 30_000L

/**
 * FEED-26: whole minutes of silence once a live run crosses
 * [STALE_ACTIVITY_AFTER_MS] — null while it still reads as healthy (and
 * whenever nothing has dated the feed yet). [lastActivityMs] is OUR
 * observation of the last feed change, never a wire `at` (they are optional).
 */
internal fun staleActivityMinutes(lastActivityMs: Long?, nowMs: Long): Int? {
    val quiet = nowMs - (lastActivityMs ?: return null)
    if (quiet < STALE_ACTIVITY_AFTER_MS) return null
    return (quiet / 60_000L).toInt()
}

/** `Live · macbook` / `Needs your input · macbook` / `No activity for 27 min ·
 *  macbook` / `Paused · …` / `Session ended` — the caption under the identity
 *  line. */
internal fun sessionStatusLine(
    phase: AgentPhase,
    deviceLabel: String?,
    awaiting: Boolean,
    paused: Boolean,
    /** FEED-26: minutes of silence, already thresholded (see
     *  [staleActivityMinutes]); null = the run reads as healthy. Paused,
     *  awaiting and compacting runs never get one. */
    staleMinutes: Int? = null,
): String {
    val label = deviceLabel?.takeIf { it.isNotBlank() }
    return when {
        paused -> if (label != null) "Paused · $label" else "Paused"
        else -> when (phase) {
            AgentPhase.Live -> {
                val prefix = when {
                    awaiting -> "Needs your input"
                    staleMinutes != null -> "No activity for $staleMinutes min"
                    else -> "Live"
                }
                if (label != null) "$prefix · $label" else prefix
            }
            AgentPhase.Connecting, AgentPhase.Starting, AgentPhase.Idle -> "Connecting…"
            is AgentPhase.Ended -> "Session ended"
            is AgentPhase.Closed -> if (phase.reconnecting) "Reconnecting…" else "Disconnected"
        }
    }
}

// ── The feed ─────────────────────────────────────────────────────────────────

/** EXP-795 — the window anchor that means "the feed's first row, whatever it
 *  is": set when the reader asks for a page the feed does not hold yet, so a
 *  prepended page is inside the window the moment it lands. Below every real
 *  id (ids only ever count down at the front by a finite amount). */
private const val WINDOW_FROM_START = Long.MIN_VALUE

// ── EXP-787: the transcript's measure, rhythm and type steps ────────────────
//
// The ladder itself is `transcriptGap` in domain/AgentFeed.kt — ONE derivation,
// mirrored ×4. These are its Compose end: the dp per step, the reading measure
// a row is capped at, and the two type steps the transcript reads at. All five
// numbers come from the shared `DesignTokens.Transcript` group; none of them is
// spelled out at a call site.

/** The space a ladder step is worth. */
private val TranscriptGap.dp: Dp
    get() = when (this) {
        TranscriptGap.Turn -> DesignTokens.Transcript.GapTurn.dp
        TranscriptGap.Block -> DesignTokens.Transcript.GapBlock.dp
        TranscriptGap.Tool -> DesignTokens.Transcript.GapTool.dp
        TranscriptGap.Default -> DesignTokens.Transcript.GapDefault.dp
        TranscriptGap.None -> 0.dp
    }

/** The transcript's reading measure — prose stops widening here however wide
 *  the window gets (landscape, a tablet, an unfolded foldable), instead of
 *  running a paragraph across the whole screen. */
private val TranscriptMaxWidth = DesignTokens.Transcript.MaxWidth.dp

/**
 * One row of the transcript: the gap ABOVE it, and its content held to
 * [TranscriptMaxWidth] and centred in whatever width is left. The screen's own
 * horizontal inset is outside this — the cap only bounds how wide a row may
 * get, never how narrow.
 */
@Composable
private fun TranscriptRow(gap: TranscriptGap, content: @Composable () -> Unit) {
    ReadingColumn(modifier = Modifier.padding(top = gap.dp), content = content)
}

/**
 * EXP-927: the transcript's reading column, on its own — [TranscriptMaxWidth]
 * centred in whatever width is left, inside the face's own horizontal inset.
 * The conversation tabs and every strip block above the composer (task list,
 * background tasks, waits, queue) take it too, so on a tablet they sit in the
 * SAME column as the rows they describe instead of running the panel's width.
 * On a phone it is a no-op: nothing is wider than the measure anyway, and the
 * gutter is the face's.
 */
@Composable
private fun ReadingColumn(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
        Box(modifier = Modifier.widthIn(max = TranscriptMaxWidth).fillMaxWidth()) { content() }
    }
}

/** Prose in the transcript — narration, a sent message, a card's markdown.
 *  DERIVED from the markdown body style, so the family, the colour and every
 *  other document affordance stay exactly as they are and only the measure
 *  changes: this is a wall of agent prose, not an issue description. */
internal val TranscriptBodyStyle: TextStyle = MdStyle.body.copy(
    fontSize = DesignTokens.Transcript.BodySize.sp,
    lineHeight = DesignTokens.Transcript.BodyLineHeight.sp,
)

/** Everything the transcript says ABOUT the run — tool rows, group captions,
 *  the permission headline, "Working…". One step down from prose. */
@Composable
private fun transcriptToolStyle(): TextStyle = MaterialTheme.typography.labelMedium.copy(
    fontSize = DesignTokens.Transcript.ToolSize.sp,
    lineHeight = DesignTokens.Transcript.ToolLineHeight.sp,
)

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
    /** EXP-783: whether the DEVICE still holds transcript below this feed's
     *  first row, and the ask that fetches it. */
    onCanLoadEarlier: () -> Boolean = { false },
    onLoadEarlier: () -> Unit = {},
    /** EXP-389: show the trailing "Working…" indicator — the session is live
     *  and nothing waits on the user. */
    working: Boolean,
    /** EXP-850 (S5): the working row's caption inputs — the turn's start (the
     *  clock AND the verb's seed), its output tokens so far, and the caption
     *  of the running workflow, which REPLACES the verb while one runs. */
    turnStartedAt: Long? = null,
    turnTokens: Long? = null,
    workflowCaption: String? = null,
    /** EXP-850 (S5): whose brand mark pulses beside that caption. */
    workingAgent: String? = null,
    /** EXP-850 (S3): the workflow cards, latest-wins per id. Each one is drawn
     *  IN PLACE OF the `tool` row carrying the same id, never as a second
     *  row; its own settle folds into the card. */
    workflows: List<WorkflowState> = emptyList(),
    answerEnabled: Boolean,
    answerStates: Map<String, AnswerState>,
    /** EXP-588: lock key → the locally picked answer summary. */
    answerLabels: Map<String, String>,
    /** (question, keys, text) — the option keys chosen on that card (a
     *  multi-select step sends all of them at once); `text` is the typed
     *  reply for a `freeText` option (EXP-513), else null. */
    onAnswer: (AgentFeedItem.Question, List<String>, String?) -> Unit,
    /** EXP-820: (plan, reject key, feedback) — the plan's reject row sent
     *  WITH text: the key answers the card, the text is the next message. */
    onPlanFollowUp: (AgentFeedItem.Question, String, String) -> Unit,
    /** EXP-724: this run's agent (null = the default one) — a steered message
     *  whose first token is one of ITS catalog commands renders as a command
     *  pill instead of prose. */
    agent: String? = null,
    /** EXP-688: how much of the feed's tail the floating Latest-changes bar
     *  covers. The list pads past it (and so does the Jump-to-bottom pill), so
     *  the last message is never parked underneath it. */
    bottomInset: Dp = 0.dp,
    /** EXP-1162: the header band the list's head scrolls under. */
    topInset: Dp = 0.dp,
    /** EXP-1162: the feed's leading row (an ended run's byline). */
    header: @Composable () -> Unit = {},
    /** EXP-1175: the Run face's status row, pinned under the header band
     *  above the tabs and the list. */
    statusRow: @Composable () -> Unit = {},
    /** THE ask dock (ask-dock.json): the card row waiting on the viewer,
     *  drawn in the composer's slot instead of here; null = none. */
    docked: AgentFeedRow? = null,
) {
    // A card with a wire id stays answerable until it resolves; an id-less one
    // (a pre-EXP-249 desktop) is read-only (EXP-672).
    val activeQuestionIds = remember(feed) { activeQuestionIds(feed) }
    // Subagent groups, askId steppers and consecutive tool runs are all
    // render-time projections only — the flat feed stays the state.
    // EXP-783: the transcript keeps the WHOLE run, and this screen paints a
    // WINDOW over it. `windowFrom` is the id of the oldest rendered row — null
    // means the newest FEED_WINDOW rows, WINDOW_FROM_START the feed's own
    // first row whatever it is. An id rather than an index, so a byte-budget
    // eviction or a replay swap cannot slide the window under the reader.
    var windowFrom by remember { mutableStateOf<Long?>(null) }
    val windowStart = remember(feed, windowFrom) {
        val tail = maxOf(0, feed.size - FEED_WINDOW)
        val from = windowFrom ?: return@remember tail
        val at = feed.indexOfFirst { it.id >= from }
        minOf(if (at == -1) tail else at, tail)
    }
    // EXP-850 (S3): a `Workflow` tool row never collapses into a tool group —
    // it IS the card.
    val workflowIds = remember(workflows) { workflows.mapTo(mutableSetOf()) { it.id } }
    // EXP-895: the ONE row the transcript keeps EXPANDED — the last feed item
    // while it is an unsettled tool call, and only in a LIVE run (an ended run's
    // trailing unsettled row is history, not a tail). The rule is shared ×4.
    val liveRowId = remember(feed, live) { if (live) liveToolRowId(feed) else null }
    val rows = remember(feed, windowStart, workflowIds, docked) {
        splitWorkflowToolRows(groupFeedRows(feed, windowStart, workflowIds), workflowIds)
            .filterNot { docked != null && isSameAskRow(it, docked) }
    }
    // Cards whose tool row this window does not hold (see the tail items).
    val orphanWorkflows = remember(rows, workflows) {
        val shown = rows.flatMapTo(mutableSetOf()) { row ->
            when (row) {
                is AgentFeedRow.Single -> listOfNotNull((row.item as? AgentFeedItem.Tool)?.callId)
                is AgentFeedRow.ToolRun -> row.items.mapNotNull { it.callId }
                else -> emptyList()
            }
        }
        workflows.filter { it.id !in shown }
    }
    // While the reader follows the tail the window may SLIDE with the stream
    // (invisible, and it keeps the projection bounded). The moment they scroll
    // up it stays pinned: rows vanishing above a reader is exactly the jump
    // this change exists to avoid.
    LaunchedEffect(follow) { if (follow) windowFrom = null }
    val canLoadEarlier = windowStart > 0 || onCanLoadEarlier()
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
    // EXP-1175: the card inputs, shared with the thread's pending cards.
    val cards = AnswerCards(
        answered = answered,
        activeQuestionIds = activeQuestionIds,
        answerEnabled = answerEnabled,
        answerStates = answerStates,
        answerLabels = answerLabels,
        onAnswer = onAnswer,
        onPlanFollowUp = onPlanFollowUp,
    )

    // Only user drags flip follow-mode; programmatic scrolls keep it.
    // EXP-529 batch: "at the bottom" carries ~96dp of slack — the pixel-exact
    // canScrollForward check dropped follow-mode (and surfaced the "Jump to
    // bottom" chip) while the list sat a few px shy of the true bottom, which
    // visually IS the bottom. Near-bottom counts as bottom, so the chip only
    // appears after a real scroll-up and hides again within the same slack.
    val followSlackPx = with(LocalDensity.current) { 96.dp.toPx() }
    val jumpScope = rememberCoroutineScope()
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
    LaunchedEffect(feed.size, follow, agentTab, working, orphanWorkflows.size) {
        val visible = if (focused != null) {
            focused.items.size + 1
        } else {
            rows.size + orphanWorkflows.size + (if (working) 1 else 0)
        }
        if (follow && visible > 0) {
            listState.scrollToItem(visible - 1)
            listState.scrollBy(1_000_000f)
        }
    }

    // EXP-1162: the floating bottom band grew or shrank (a banner, the
    // queue, the keyboard's composer) — a follower stays on the true end.
    LaunchedEffect(bottomInset) {
        if (follow) listState.scrollBy(1_000_000f)
    }
    // EXP-1316: a follower sits on the TRUE end whatever moved it off: the
    // newest row growing after its first layout (a card's markdown, a late
    // option row) or the pinned band (the Tasks panel, the composer)
    // changing height. Whenever the list could still scroll forward while
    // following and nobody drags, it re-pins, so the newest row always
    // clears what is pinned at the bottom.
    val followState = rememberUpdatedState(follow)
    LaunchedEffect(listState) {
        snapshotFlow {
            followState.value && !listState.isScrollInProgress && listState.canScrollForward
        }.collect { behind -> if (behind) listState.scrollBy(1_000_000f) }
    }

    // Only composed once real activity has arrived (the placeholder states are
    // siblings). EXP-1175: the `agent-feed` tag moved to the Run face body.
    Column(modifier = Modifier.fillMaxSize()) {
        // EXP-1175: the status row, then the subagent tabs, stay put above the
        // list (under the header band, not beneath it); the list starts below.
        Spacer(Modifier.height(topInset))
        statusRow()
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
            modifier = Modifier.fillMaxSize().detailHazeSource(),
            // Bottom-anchored: a short feed sits above the input bar, not at
            // the top of the screen.
            verticalArrangement = Arrangement.Bottom,
            contentPadding = PaddingValues(
                top = 8.dp,
                bottom = 8.dp + bottomInset,
            ),
        ) {
            item(key = "feed-header") { header() }
            if (focused != null) {
                // EXP-356: the focused subagent's conversation — its
                // delegation summary, then (EXP-773) its prose, the turns
                // addressed to it and its tool calls, in publish order.
                item(key = "agent-summary") {
                    TranscriptRow(TranscriptGap.None) {
                        SubagentWorkCard(run = focused, onOpen = null)
                    }
                }
                if (focused.items.isEmpty()) {
                    item(key = "agent-empty") {
                        EmptyState(EmptyStateCopy.transcript)
                    }
                } else {
                    // EXP-916: the lane's own projection — its consecutive
                    // edits fold into ONE edited-files card, exactly as they
                    // do in the main transcript.
                    val laneRows = projectLaneRows(focused.items, workflowIds)
                    itemsIndexed(laneRows, key = { _, laneRow -> laneRow.id }) { index, laneRow ->
                        // The same ladder as the main feed, over the rows a
                        // subagent's conversation is made of.
                        TranscriptRow(
                            transcriptGap(laneRows.getOrNull(index - 1)?.rowClass, laneRow.rowClass),
                        ) {
                            LaneRow(laneRow, liveItemId = liveRowId, live = live)
                        }
                    }
                }
            } else {
            if (canLoadEarlier) {
                item(key = "load-earlier") {
                    TextButton(
                        onClick = {
                            if (windowStart > 0) {
                                windowFrom = feed[maxOf(0, windowStart - FEED_WINDOW_STEP)].id
                            } else {
                                // Past the feed's own first row: pin the
                                // window to the FRONT before asking, so the
                                // page the device sends is inside it the
                                // moment it lands (EXP-795).
                                windowFrom = WINDOW_FROM_START
                                onLoadEarlier()
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text("Load earlier") }
                }
            }
            // EXP-787: the row's space comes from the row BEFORE it — the one
            // gap ladder, shared with web, iOS and the desktop. Keys stay the
            // row ids: the follow/scroll logic is anchored on them.
            itemsIndexed(rows, key = { _, row -> row.id }) { index, row ->
                TranscriptRow(
                    transcriptGap(rows.getOrNull(index - 1)?.rowClass, row.rowClass),
                ) {
                    when (row) {
                        is AgentFeedRow.ToolRun -> ToolGroupRow(
                            items = row.items,
                            liveTail = liveRowId != null && row.items.last().id == liveRowId,
                        )
                        // EXP-916: a run of edits is ONE card, one row per
                        // path, the live one open with its patch inline.
                        is AgentFeedRow.Edits -> EditedFilesCard(
                            items = row.items,
                            liveItemId = liveRowId,
                        )
                        // EXP-948: a run of consecutive calls to ONE of our own
                        // MCP tools is ONE captioned row ("Read 3 issues") —
                        // our tools never hide inside "N other tools".
                        is AgentFeedRow.ExpRun -> ExpToolGroupRow(items = row.items)
                        // EXP-1324: a subagent is its Trace line (the duration
                        // as the outline badge); its rows are its own tab's,
                        // which the line opens.
                        is AgentFeedRow.SubagentRun -> SubagentLineRow(
                            run = row,
                            live = live,
                            onOpen = {
                                onAgentTabChange(row.subagentId)
                                onFollowChange(true)
                            },
                        )
                        is AgentFeedRow.QuestionStepper -> AnswerCardRow(row, cards)
                        is AgentFeedRow.Single -> when (val item = row.item) {
                            is AgentFeedItem.Narration -> NarrationBubble(item.text)
                            is AgentFeedItem.ApiError -> ApiErrorRow(item.message, item.errorType)
                            // EXP-850 (S3): the `Workflow` call renders as its
                            // card — name, phases, agents, summary — and never
                            // as a tool row beside it.
                            is AgentFeedItem.Tool ->
                                when (val workflow = workflows.firstOrNull { it.id == item.callId }) {
                                    // EXP-916: such a call may still CARRY a
                                    // patch (an edit tagged with a workflow).
                                    // The card is the only place a patch
                                    // renders now, so a one-member card draws
                                    // it rather than a row that drops it.
                                    null -> if (!item.diff.isNullOrEmpty()) {
                                        EditedFilesCard(
                                            items = listOf(item),
                                            liveItemId = liveRowId,
                                        )
                                    } else {
                                        ToolRow(item, live = item.id == liveRowId)
                                    }
                                    else -> WorkflowWorkCard(
                                        workflow = workflow,
                                        duplicates = workflowDuplicates(feed, workflow.id),
                                        onOpenAgent = { id ->
                                            onAgentTabChange(id)
                                            onFollowChange(true)
                                        },
                                    )
                                }
                            // EXP-856: a second copy of a live agent, inline —
                            // a workflow's own duplicates render under its card.
                            is AgentFeedItem.DuplicateAgent -> DuplicateAgentRow(item)
                            is AgentFeedItem.UserMessage -> {
                                // EXP-724: a steered catalog command reads as one.
                                val command =
                                    SlashCommands.commandFor(item.text, agent)
                                if (command != null) {
                                    CommandRow(command, item.text)
                                } else {
                                    UserMessageBubble(item.text)
                                }
                            }
                            is AgentFeedItem.Compaction -> CompactionMarkerRow()
                            // EXP-1324: the reasoning line; open only while
                            // the run lives.
                            is AgentFeedItem.Thought -> ThoughtRow(item, running = live && !item.ended)
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
                            is AgentFeedItem.Subagent -> SubagentLineRow(
                                live = live,
                                run = AgentFeedRow.SubagentRun(
                                    id = item.id,
                                    subagentId = item.subagentId,
                                    agentType = item.agentType,
                                    completed = item.completed,
                                    detail = item.detail,
                                    title = item.title,
                                    items = emptyList(),
                                ),
                                onOpen = {},
                            )
                            is AgentFeedItem.Question -> AnswerCardRow(row, cards)
                        }
                    }
                }
            }
            // EXP-850 (S3): a card whose `Workflow` tool row is not in the
            // rendered window (evicted, or below it) still has to be seen —
            // it lands at the tail as a row of its own rather than taking its
            // agents and its warnings down with it.
            if (orphanWorkflows.isNotEmpty()) {
                items(orphanWorkflows, key = { "workflow-${it.id}" }) { workflow ->
                    TranscriptRow(TranscriptGap.Tool) {
                        WorkflowWorkCard(
                            workflow = workflow,
                            duplicates = workflowDuplicates(feed, workflow.id),
                            onOpenAgent = { id ->
                                onAgentTabChange(id)
                                onFollowChange(true)
                            },
                        )
                    }
                }
            }
            // EXP-389: the agent-is-busy footer under the newest event (iOS
            // parity) — main conversation only, subagent chips carry their
            // own spinner.
            if (working) {
                item(key = "working-indicator") {
                    TranscriptRow(
                        transcriptGap(rows.lastOrNull()?.rowClass, AgentRowClass.Tool),
                    ) {
                        WorkingIndicatorRow(
                            startedAt = turnStartedAt,
                            tokens = turnTokens,
                            workflowCaption = workflowCaption,
                            agent = workingAgent,
                        )
                    }
                }
            }
            }
        }
        // EXP-1191: the shared round scroll-to-bottom button, centred 12dp
        // above the bar's rung ([bottomInset] = the floating band, whose top
        // 8dp is the bar's own padding). The tap animates to the true end,
        // then re-pins follow-mode.
        JumpToBottomButton(
            visible = !follow && feed.isNotEmpty(),
            onClick = {
                jumpScope.launch {
                    val last = listState.layoutInfo.totalItemsCount - 1
                    if (last >= 0) {
                        listState.animateScrollToItem(last)
                        listState.scrollBy(1_000_000f)
                    }
                    onFollowChange(true)
                }
            },
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = bottomInset + 4.dp),
        )
        // EXP-1162: the EXP-698 top fade is the header band's edge strip now
        // (`HeaderEdgeChrome`), over every face alike. EXP-1175: the list
        // starts below the status row (and the tabs), so its own head dissolves.
        run {
            // EXP-1256: the thread's edge in the page's own ground, no tint.
            EdgeFade(
                FadeEdge.Top,
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .fillMaxWidth()
                    .height(DetailChrome.EDGE_TOP.dp),
                ground = true,
            )
        }
        }
    }
}

/**
 * EXP-1175: everything a plan/question card needs, held once so the
 * transcript and the thread draw the SAME cards from the same inputs.
 */
private class AnswerCards(
    /** Lock keys of the steps whose answer is out (sent or acknowledged). */
    val answered: Set<String>,
    val activeQuestionIds: Set<Long>,
    val answerEnabled: Boolean,
    val answerStates: Map<String, AnswerState>,
    /** EXP-588: lock key → the locally picked answer summary. */
    val answerLabels: Map<String, String>,
    val onAnswer: (AgentFeedItem.Question, List<String>, String?) -> Unit,
    val onPlanFollowUp: (AgentFeedItem.Question, String, String) -> Unit,
)

/** EXP-1175: one card row — an ask's stepper or a single question/plan card;
 *  any other row draws nothing. */
@Composable
private fun AnswerCardRow(row: AgentFeedRow, cards: AnswerCards) {
    when (row) {
        is AgentFeedRow.QuestionStepper -> QuestionStepperCard(
            askId = row.askId,
            // EXP-1316: every step offers the free-text answer.
            steps = remember(row.steps) { row.steps.map { it.withFreeTextRow() } },
            answered = cards.answered,
            activeQuestionIds = cards.activeQuestionIds,
            answerEnabled = cards.answerEnabled,
            answerStates = cards.answerStates,
            answerLabels = cards.answerLabels,
            onAnswer = cards.onAnswer,
        )
        is AgentFeedRow.Single -> {
            val raw = row.item as? AgentFeedItem.Question ?: return
            // EXP-1316: the free-text answer on every question card.
            val item = remember(raw) { raw.withFreeTextRow() }
            QuestionCard(
                item = item,
                active = item.id in cards.activeQuestionIds,
                answerEnabled = cards.answerEnabled,
                state = cards.answerStates[item.wireId],
                stepLabel = null,
                onAnswer = { keys, text -> cards.onAnswer(item, keys, text) },
                onPlanFollowUp = { key, text -> cards.onPlanFollowUp(item, key, text) },
            )
        }
        else -> Unit
    }
}
/** THE ask dock ×session view: the ONE card row waiting on this viewer (the
 *  newest stepper or question/plan card with an active step), drawn in the
 *  composer's slot; null = nothing waits. */
internal fun dockedAskRow(rows: List<AgentFeedRow>, active: Set<Long>): AgentFeedRow? =
    rows.lastOrNull { row ->
        when (row) {
            is AgentFeedRow.QuestionStepper -> row.steps.any { it.id in active }
            is AgentFeedRow.Single -> (row.item as? AgentFeedItem.Question)?.id?.let { it in active } == true
            else -> false
        }
    }

/** Whether [row] is the docked card row (a stepper by its ask, a card by id). */
private fun isSameAskRow(row: AgentFeedRow, docked: AgentFeedRow): Boolean = when {
    row is AgentFeedRow.QuestionStepper && docked is AgentFeedRow.QuestionStepper -> row.askId == docked.askId
    row is AgentFeedRow.Single && docked is AgentFeedRow.Single -> row.item.id == docked.item.id
    else -> false
}

/** Visually-at-the-bottom, with slack: the last item's bottom edge sits within
 *  [slackPx] of the viewport end (an empty list counts as bottom). A last item
 *  taller than the slack correctly reads as NOT near while its tail is off
 *  screen. */


internal fun LazyListState.isNearBottom(slackPx: Float): Boolean {
    val info = layoutInfo
    val last = info.visibleItemsInfo.lastOrNull() ?: return true
    if (last.index < info.totalItemsCount - 1) return false
    return last.offset + last.size - info.viewportEndOffset <= slackPx
}

/** EXP-356: conversation tabs — Main plus one chip per RUNNING subagent
 *  (ended tabs are dropped, EXP-387), labeled with the run's DESCRIPTION when
 *  the spawning call gave one and its agent type otherwise (EXP-847), and a
 *  spinner while it works. */
@Composable
private fun AgentTabStrip(
    agents: List<AgentFeedRow.SubagentRun>,
    selected: String?,
    onSelect: (String?) -> Unit,
) {
    // EXP-927: the tabs read in the transcript's own column — they name the
    // conversations the rows below belong to, so they may not start anywhere
    // else (their horizontal inset is the face's, like every row's).
    ReadingColumn {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .horizontalScroll(rememberScrollState())
                .padding(vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AgentTabChip(label = "Main", running = false, selected = selected == null) {
                onSelect(null)
            }
            agents.forEach { run ->
                AgentTabChip(
                    label = run.label,
                    running = !run.completed,
                    selected = selected == run.subagentId,
                ) {
                    onSelect(run.subagentId)
                }
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
    GlassPill(
        label,
        size = PillSize.Sm,
        mode = PillMode.Select,
        selected = selected,
        maxLines = 1,
        onClick = onClick,
        leading = if (running) {
            {
                CircularProgressIndicator(
                    modifier = Modifier.size(10.dp),
                    strokeWidth = 1.5.dp,
                    color = LocalContentColor.current,
                )
            }
        } else {
            null
        },
    )
}

// Assistant prose — a small glyph + plain full-width selectable text. EXP-274
// dropped the glass speech bubble: agent output is the feed's bulk, and the
// bubble insets cost real width on a phone.
@Composable
private fun NarrationBubble(
    text: String,
    /** Inside a subagent group, where the surrounding rows keep their own
     *  compact rhythm — the feed's rows get theirs from the gap ladder
     *  (EXP-787), so a top-level bubble carries no outer padding. */
    nested: Boolean = false,
) {
    // MarkdownView renders nothing for blank input — without this the icon
    // would be left behind as an orphan row.
    if (text.isBlank()) return
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (nested) Modifier.padding(vertical = 5.dp) else Modifier),
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

/**
 * The trailing "agent is busy" row (EXP-389/EXP-850): the running agent's own
 * brand mark, pulsing, beside what the agent is doing —
 * `{verb}… ({duration} · ↓ {tokens} tokens)`, or the running workflow's
 * caption in place of the verb. The clock ticks here, once a second, so the
 * transcript around it is never recomposed for it.
 *
 * The PULSE is the mark alone (EXP-850 S5): the caption is text somebody is
 * reading and must not breathe under them. Reduced motion pins the mark at
 * full opacity.
 */
@Composable
private fun WorkingIndicatorRow(
    startedAt: Long? = null,
    tokens: Long? = null,
    workflowCaption: String? = null,
    agent: String? = null,
) {
    // One tick a second, only while a start is known — with no clock to show
    // there is nothing to recompose for.
    var nowMs by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(startedAt) {
        if (startedAt == null) return@LaunchedEffect
        while (true) {
            nowMs = System.currentTimeMillis()
            delay(WORKING_CLOCK_TICK_MS)
        }
    }
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // EXP-850: WHICH agent is working, not a generic assistant glyph.
        // EXP-1184: claude steps its own spark; any other agent pulses its
        // brand mark (the neutral `settings-agents` concept when unknown).
        AgentWorkingMark(
            agent,
            13.dp,
            fallbackTint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Text(
            workingCaption(startedAt, tokens, nowMs, workflowCaption),
            style = transcriptToolStyle(),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * The working mark's alpha (EXP-850 S5): 1 → 0.4 and back, [WORKING_PULSE_MS]
 * each way. Shared with the Work tabs' Run mark (EXP-1162), so the two beat
 * alike. Reduced motion pins it at 1.
 */
@Composable
internal fun rememberWorkingMarkPulse(): Float {
    val reduceMotion = LocalReduceMotion.current
    val pulse by rememberInfiniteTransition(label = "working").animateFloat(
        initialValue = 1f,
        targetValue = if (reduceMotion) 1f else 0.4f,
        animationSpec = infiniteRepeatable(
            tween(WORKING_PULSE_MS, easing = FastOutSlowInEasing),
            RepeatMode.Reverse,
        ),
        label = "workingAlpha",
    )
    return pulse
}

/** EXP-850 (S5): the working caption's clock tick, and the brand mark's
 *  pulse (1.4s each way, ease-in-out — the ×4 number). */
private const val WORKING_CLOCK_TICK_MS = 1_000L
private const val WORKING_PULSE_MS = 1_400

/**
 * EXP-1321/1324: THE Queue at the bottom of the transcript — the agent's own
 * task list, the machine's background tasks (their elapsed time ticking each
 * second; the wire's `startedAt`, else the first time this client saw the
 * id) and the open `wait` calls, as ONE [TaskQueueView]. Absent when nothing
 * is left to do.
 */
@Composable
private fun SessionTaskQueue(
    feed: List<AgentFeedItem>,
    taskList: List<TaskListEntry>,
    backgroundTasks: List<BackgroundTask>,
    todoOpen: Boolean,
    onTodoOpenChange: (Boolean) -> Unit,
    completedOpen: Boolean,
    onCompletedOpenChange: (Boolean) -> Unit,
) {
    // First sight per background id, for a publisher that sends no start:
    // stamped off the wall clock as the list arrives (the ticking `now` may
    // stand still between ticks), and never inside a remember calculation.
    val firstSeen = remember { mutableMapOf<String, Long>() }
    for (task in backgroundTasks) {
        if (task.startedAt == null && task.id !in firstSeen) firstSeen[task.id] = System.currentTimeMillis()
    }
    val ticking = backgroundTasks.any { it.kind != BACKGROUND_TASK_KIND_AGENT }
    val now = rememberTickingNow(ticking)
    val waits = remember(feed) { openWaits(feed) }
    val queue = remember(taskList, backgroundTasks, waits, now) {
        TaskQueues.of(
            TaskQueueInput(
                tasks = taskList.map { TaskQueueTaskInput(it.content, it.status) },
                backgroundTasks = backgroundTasks.map {
                    TaskQueueBackgroundInput(
                        id = it.id,
                        kind = it.kind,
                        description = it.description,
                        startedAt = it.startedAt ?: firstSeen[it.id] ?: now,
                    )
                },
                waits = waits,
                now = now,
            ),
        )
    } ?: return
    ReadingColumn {
        TaskQueueView(
            queue = queue,
            todoOpen = todoOpen,
            onTodoOpenChange = onTodoOpenChange,
            completedOpen = completedOpen,
            onCompletedOpenChange = onCompletedOpenChange,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 6.dp),
        )
    }
    Spacer(Modifier.height(8.dp))
}

/**
 * EXP-1323: a thread's live tail — THE Queue (EXP-1324: the same block as the
 * session view's transcript bottom) and its working status line (mid-turn,
 * nothing parked), both off [activity] = that session's relay state. Nothing
 * shows while neither has anything to say.
 */
@Composable
internal fun SessionLiveTail(activity: ActivityFeedState, session: SessionEntity?) {
    var todoOpen by remember { mutableStateOf(true) }
    var completedOpen by remember { mutableStateOf(false) }
    val working = agentWorking(
        live = true,
        sessionEnded = session?.isEnded == true,
        turnState = activity.turnState,
        awaitingInput = false,
        needsInput = session?.needsInput == true,
        blocked = AgentUsagePresentation.parseBlocked(session?.blocked) != null,
        compacting = activity.compacting != null,
    )
    Column(
        modifier = Modifier.fillMaxWidth().testTag("thread-live-tail"),
    ) {
        if (session?.isEnded != true) {
            SessionTaskQueue(
                feed = activity.feed,
                taskList = activity.taskList,
                backgroundTasks = activity.backgroundTasks,
                todoOpen = todoOpen,
                onTodoOpenChange = { todoOpen = it },
                completedOpen = completedOpen,
                onCompletedOpenChange = { completedOpen = it },
            )
        }
        if (working) {
            WorkingIndicatorRow(
                startedAt = activity.turnStartedAt,
                tokens = activity.turnTokens,
                workflowCaption = activity.runningWorkflow()?.caption(),
                agent = session?.cli?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT,
            )
        }
    }
}

/**
 * EXP-861: the queue bar above the composer — the messages the agent has not
 * read yet, oldest first, each on ONE truncated line (a multi-line message
 * reads as one); a held line carries the ghost X that revokes it
 * ([QUEUE_REMOVE_LABEL]), a `sent` one (EXP-873) none. Same recipe as
 * [BackgroundWorkStrip]; the bar itself reads as [QUEUE_STRIP_TITLE] to a
 * screen reader.
 */
@Composable
private fun QueueStrip(messages: List<QueuedMessage>, onRemove: (String) -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .padding(start = 12.dp, end = 4.dp, top = 2.dp, bottom = 2.dp)
            .semantics { contentDescription = QUEUE_STRIP_TITLE }
            .testTag("queue-strip"),
    ) {
        messages.forEach { message ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Icon(
                    imageVector = ExpIcons.uiQueued,
                    contentDescription = null,
                    modifier = Modifier.size(12.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
                Text(
                    message.text.replace(QUEUE_LINE_WHITESPACE, " ").trim(),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                // EXP-873: a line already with the agent cannot be taken back
                // one at a time — only Stop does — so it draws no X.
                if (message.sent) {
                    Spacer(modifier = Modifier.size(28.dp))
                } else {
                    IconButton(
                        onClick = { onRemove(message.id) },
                        modifier = Modifier.size(28.dp).testTag("queue-remove"),
                    ) {
                        Icon(
                            imageVector = ExpIcons.uiClose,
                            contentDescription = QUEUE_REMOVE_LABEL,
                            modifier = Modifier.size(12.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }
        }
    }
}

/** EXP-861: a queued message renders on one line — newlines and runs of
 *  whitespace collapse to a single space before the ellipsis. */
private val QUEUE_LINE_WHITESPACE = Regex("\\s+")

/**
 * EXP-856: a SECOND copy of an agent that is still running — messaging a live
 * agent resumes it from its transcript, and both copies then edit the same
 * files. The engine writes the sentence; every client renders it verbatim in
 * amber, and a workflow's own warnings stay under its card even when the card
 * is collapsed.
 */
@Composable
private fun DuplicateAgentRow(item: AgentFeedItem.DuplicateAgent) {
    Row(
        modifier = Modifier.fillMaxWidth().testTag("duplicate-agent-row"),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.uiWarning,
            contentDescription = null,
            modifier = Modifier.size(13.dp).padding(top = 1.dp),
            tint = NeedsInputAmber,
        )
        Text(
            item.detail,
            style = transcriptToolStyle(),
            color = NeedsInputAmber,
            modifier = Modifier.weight(1f),
        )
    }
}

/**
 * EXP-850 (S3) / EXP-1240: a claude `Workflow` call as THE Work card — drawn
 * IN PLACE OF the `Workflow` tool row with the same id: the phases as a
 * segment bar, the finished ones folded into one row, the running phase's
 * lanes one line each. A lane opens that agent's own tab (EXP-1225: a
 * workflow agent is a conversation tab like any subagent). EXP-856: the
 * duplicate warnings stay under the card whatever it is doing.
 */
@Composable
private fun WorkflowWorkCard(
    workflow: WorkflowState,
    duplicates: List<AgentFeedItem.DuplicateAgent>,
    onOpenAgent: (subagentId: String) -> Unit,
) {
    val card = remember(workflow) { workflow.workCard() }
    val folded = remember(workflow, card) { workflow.foldedPhases(card) }
    WorkCardView(
        card = card,
        foldedPhases = folded,
        onOpenLane = { target -> workflow.laneAgent(target)?.agentId?.let(onOpenAgent) },
    ) {
        if (duplicates.isNotEmpty()) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                duplicates.forEach { DuplicateAgentRow(it) }
            }
        }
    }
}

/**
 * EXP-1240: a subagent's work as THE Work card — its name, the calls it made,
 * done or running. [onOpen] opens the subagent's own tab, where its rows live
 * (null = the card heading that tab). EXP-748: the count is the publisher's.
 */
@Composable
private fun SubagentWorkCard(run: AgentFeedRow.SubagentRun, onOpen: (() -> Unit)?) {
    val card = remember(run.label, run.completed, run.toolCount) { run.workCard() }
    WorkCardView(card = card, onOpenSubagent = onOpen)
}

/** EXP-1259 D8.3: the transcript's first load — skeleton rows in its own
 *  column, the phase as a muted caption under them (web
 *  `transcript-placeholder`). */
@Composable
private fun TranscriptPlaceholder(caption: String) {
    ReadingColumn {
        Column(
            modifier = Modifier.padding(vertical = 16.dp).testTag("transcript-placeholder"),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            SkeletonRows(rowHeight = 36.dp)
            Text(
                caption,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/** Fold threshold for user/question text — the initial prompt can be 16 KiB. */
private const val CLAMP_LINES = 6

/** EXP-698: the fold is ONE height on every card, not six lines of whatever
 *  text style happened to be in scope — web's `max-h-40`, iOS's `maxHeight:
 *  160`. Six body lines came out a different height per card and made the feed
 *  look ragged. */
private val FOLD_HEIGHT = 160.dp
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
 *  Column, so the fold is a height clamp + clip at [FOLD_HEIGHT]; the fold
 *  decision stays the source-text heuristic. */
@Composable
private fun FoldableMarkdown(
    text: String,
    expanded: Boolean,
    softBreaks: Boolean = false,
    onToggle: () -> Unit,
) {
    val folds = remember(text) { clampable(text) }
    // MarkdownView brings its own SelectionContainer (EXP-534).
    MarkdownView(
        text,
        modifier = if (folds && !expanded) {
            Modifier.foldedTo(FOLD_HEIGHT)
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
//
// EXP-698: a steered message WITH images (EXP-511) splits into its prose —
// whose `[Image #N]` references render as pills inline with the words — and
// the embeds, which sit below it. The fold then clamps the PROSE only: four
// embed lines are four more "lines" of nothing, and clamping the wire message
// pushed a two-line steer into a fold that then hid the very images it was
// measuring. The embeds render OUTSIDE the fold for the same reason — they
// are the point of the message, and a long prose clamp must never be what
// hides them.
@Composable
private fun UserMessageBubble(text: String, nested: Boolean = false) {
    var expanded by remember { mutableStateOf(false) }
    val parsed = remember(text) { parseSteerMessage(text) }
    val hasImages = parsed.attachmentIds.isNotEmpty()
    // Wave D: file lines peel off too — rendered as file tiles below.
    val hasFiles = parsed.files.isNotEmpty()
    val prose = if (hasImages || hasFiles) parsed.text else text
    val folds = remember(prose) { clampable(prose) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            // EXP-787: the gap above a turn is the ladder's, except inside a
            // subagent group, which keeps its own rhythm.
            .then(if (nested) Modifier.padding(vertical = 5.dp) else Modifier),
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
            Column(
                modifier = if (folds && !expanded) Modifier.foldedTo(FOLD_HEIGHT) else Modifier,
            ) {
                when {
                    // Markers are only meaningful against embeds that exist —
                    // an `[Image #2]` typed into an image-less message is
                    // prose, and rendering it as a pill would promise a
                    // preview there is nothing to preview.
                    hasImages && parsed.markers.isNotEmpty() ->
                        MarkerProse(parsed.text, count = parsed.attachmentIds.size)
                    prose.isNotEmpty() ->
                        MarkdownView(prose, softBreaksAsNewlines = true)
                }
            }
            if (folds) {
                ShowMoreToggle(expanded) { expanded = !expanded }
            }
            if (hasImages) {
                MarkdownView(
                    parsed.attachmentIds.joinToString("\n") {
                        "![image](/api/attachments/$it)"
                    },
                    softBreaksAsNewlines = true,
                )
            }
            if (hasFiles) {
                SteerFileRows(parsed.files)
            }
        }
    }
}

/**
 * A user message's prose with its `[Image #N]` markers drawn as pills
 * (EXP-698). One FlowRow per LINE (so hard breaks survive), each contract
 * segment one child, so a pill wraps with the sentence around it instead of
 * forcing its own line.
 *
 * Text segments go through [annotate] with autolink on — the same linkifying
 * the feed gives every other message — but NOT through the block markdown
 * renderer: a document is a stack of blocks and blocks cannot flow around an
 * inline pill. Web's marker renderer makes the identical trade (it linkifies
 * the segments and no more), so `**bold**` stays literal in a message that
 * names its images, on both clients. A `#IDENT` here renders as its styled
 * span rather than a painted chip — the chip painter needs MarkdownView's own
 * per-line layout callback.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MarkerProse(text: String, count: Int) {
    val issueRefs = LocalIssueRefs.current
    val mentions = LocalMentions.current
    val inlineCode = LocalInlineCodeStyle.current
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        text.split("\n").forEach { line ->
            if (line.isBlank()) {
                Spacer(Modifier.height(4.dp))
                return@forEach
            }
            FlowRow(
                // NO horizontal gap: a segment carries its own spaces (the
                // composer always inserts a marker space-separated), so an
                // arrangement gap on top of them doubled the air around every
                // pill. Web renders the same split as one `pre-wrap` flow.
                horizontalArrangement = Arrangement.Start,
                verticalArrangement = Arrangement.spacedBy(2.dp),
                // TalkBack reads the line as ONE sentence; without the merge
                // it announces every segment and pill as a separate node.
                modifier = Modifier.semantics(mergeDescendants = true) {},
            ) {
                steerMessageSegments(line).forEach { segment ->
                    when (segment) {
                        // A marker outside 1..count — hand-typed, or left
                        // behind by an edit — is PROSE, not a pill.
                        is SteerMessageSegment.Marker ->
                            if (segment.index in 1L..count.toLong()) {
                                GlassPill(
                                    "Image ${segment.index}",
                                    size = PillSize.Sm,
                                    mode = PillMode.Readonly,
                                    icon = ExpIcons.editorImage,
                                    modifier = Modifier.align(Alignment.CenterVertically),
                                )
                            } else {
                                ProseText(
                                    imageMarker(segment.index),
                                    issueRefs,
                                    mentions,
                                    inlineCode,
                                )
                            }
                        is SteerMessageSegment.Text ->
                            ProseText(segment.text, issueRefs, mentions, inlineCode)
                    }
                }
            }
        }
    }
}

/** One text run of a [MarkerProse] line — a single [Text], linkified. */
@Composable
private fun FlowRowScope.ProseText(
    text: String,
    issueRefs: IssueRefHandler?,
    mentions: MentionResolver?,
    inlineCode: MdStyle.InlineCodeStyle,
) {
    if (text.isEmpty()) return
    val bare = LocalIssueRefBare.current
    Text(
        remember(text, issueRefs, mentions, inlineCode, bare) {
            annotate(
                text, emptyList(), issueRefs, mentions,
                autolink = true, inlineCode = inlineCode, bare = bare,
            )
        },
        style = LocalMarkdownBodyStyle.current,
        modifier = Modifier.align(Alignment.CenterVertically),
    )
}

// One multi-question ask (EXP-249): a claude-style stepper that shows ONE step
// at a time — "Question 2 of 3" — and advances the moment a step's answer is
// sent (web parity; the 5s no-ack timeout rolls it back), ending on the ask's
// final submit step. Once every step is answered the card collapses into the
// answered summary.
// EXP-820: back and forth — while the ask is still OPEN (`askComplete` false)
// an answered step row re-opens for a new answer: it renders expanded with the
// recorded answer pre-selected, the current step folds to one muted line, and
// the new answer goes out on the EDITED step's wire id (the engine re-records
// it and resolves that step again without moving the stepper).
@Composable
private fun QuestionStepperCard(
    askId: String,
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
    val complete = remember(steps) { askComplete(steps) }
    // A finished (or dismissed) ask shows its summary whatever its locks say.
    val current = remember(steps, answered, complete) {
        if (complete) null else currentStepperStep(steps, answered)
    }
    // EXP-820: the wire id of the answered step re-opened for editing.
    var editingStep by remember(askId) { mutableStateOf<String?>(null) }
    // The rows that may be re-answered: answered here or resolved by the
    // engine, not the one showing, and not mid-send (a locked card refuses a
    // second frame anyway) — only while the ask is open and this viewer may
    // answer at all.
    val editable = remember(steps, answered, answerStates, current, complete, answerEnabled) {
        if (!answerEnabled || complete) {
            emptySet()
        } else {
            steps.filter { step ->
                step.id != current?.id &&
                    (step.resolved || step.wireId in answered) &&
                    !answerStates[step.wireId].locksCard()
            }.map { it.wireId }.toSet()
        }
    }
    val editing = editingStep?.takeIf { it in editable }?.let { id -> steps.first { it.wireId == id } }
    val onEditStep: (AgentFeedItem.Question) -> Unit = { editingStep = it.wireId }
    val total = steps.firstOrNull { it.total != null }?.total ?: steps.count { it.index != null }
    if (editing != null) {
        val at = steps.indexOf(editing)
        val answeredHere = { step: AgentFeedItem.Question -> step.resolved || step.wireId in answered }
        val before = steps.subList(0, at).filter(answeredHere)
        val after = steps.subList(at + 1, steps.size).filter { it.id != current?.id && answeredHere(it) }
        QuestionCard(
            item = editing,
            // Answerable although resolved: the engine re-records the step.
            active = true,
            answerEnabled = answerEnabled,
            state = answerStates[editing.wireId],
            stepLabel = editing.index?.takeIf { total > 1 }?.let { "$it of $total" },
            stepHeading = "Question",
            priorSteps = before,
            priorAnswers = before.map { stepAnswer(it, answerLabels) },
            trailingSteps = after,
            trailingAnswers = after.map { stepAnswer(it, answerLabels) },
            editableSteps = editable,
            onEditStep = onEditStep,
            editing = true,
            foldedCurrent = current,
            onBackToCurrent = { editingStep = null },
            localAnswer = answerLabels[editing.wireId],
            onAnswer = { keys, text ->
                onAnswer(editing, keys, text)
                editingStep = null
            },
        )
        return
    }
    if (current == null) {
        AnsweredAskCard(steps, answerLabels, complete, editable, onEditStep)
        return
    }
    // EXP-588 (web/iOS parity): the steps already answered stay visible above
    // the current one — the question folded to one line next to its answer.
    val prior = remember(steps, current) { steps.takeWhile { it.id != current.id } }
    QuestionCard(
        item = current,
        active = current.id in activeQuestionIds,
        answerEnabled = answerEnabled,
        state = answerStates[current.wireId],
        // Web `AskStepperCard`: "k of n" beside the heading, the review step
        // (no index) says how many questions it closes.
        stepLabel = when {
            total <= 1 -> null
            current.index != null -> "${current.index} of $total"
            else -> "$total questions"
        },
        stepHeading = if (current.index != null) "Question" else "Review answers",
        priorSteps = prior,
        priorAnswers = prior.map { stepAnswer(it, answerLabels) },
        editableSteps = editable,
        onEditStep = onEditStep,
        localAnswer = answerLabels[current.wireId],
        onAnswer = { keys, text -> onAnswer(current, keys, text) },
    )
}

/** A step's answer for display: the desktop-resolved text, else what this
 *  client picked (EXP-588); null = answered elsewhere, unknown here. */
private fun stepAnswer(step: AgentFeedItem.Question, answerLabels: Map<String, String>): String? =
    step.answer?.takeIf { it.isNotBlank() } ?: answerLabels[step.wireId]

/** One already-answered step of a stepper: the question on the left, folded
 *  to one line, the answer on the right (web `AnsweredStepRow` parity).
 *  EXP-820: with [onEdit] the row is a button that re-opens the step, and
 *  says so with a trailing pencil. */
@Composable
private fun AnsweredStepRow(
    step: AgentFeedItem.Question,
    answer: String?,
    onEdit: (() -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(
                if (onEdit != null) {
                    Modifier
                        .clip(RoundedCornerShape(DesignTokens.Radius.Sm))
                        .clickable(role = Role.Button, onClick = onEdit)
                } else {
                    Modifier
                },
            ),
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
            answer?.takeIf { it.isNotBlank() } ?: PlanCard.Copy.ANSWERED,
            modifier = Modifier.widthIn(max = 180.dp),
            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            textAlign = TextAlign.End,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        if (onEdit != null) {
            Icon(
                ExpIcons.uiEdit,
                contentDescription = "Change answer",
                modifier = Modifier.size(12.dp).padding(top = 1.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/** EXP-820: the ask's CURRENT step folded to one muted line while an earlier
 *  step is being re-answered; tapping it returns to the current step. */
@Composable
private fun FoldedStepRow(step: AgentFeedItem.Question, onClick: (() -> Unit)?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(
                if (onClick != null) {
                    Modifier
                        .clip(RoundedCornerShape(DesignTokens.Radius.Sm))
                        .clickable(role = Role.Button, onClick = onClick)
                } else {
                    Modifier
                },
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(13.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Text(
            step.header?.takeIf { it.isNotBlank() } ?: step.text,
            modifier = Modifier.weight(1f),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// Every step of a fully answered ask, each with the answer it got. EXP-820:
// until the ask COMPLETES (its submit step resolves) the agent is still
// working through it — the spinner says so — and a step can still be
// re-opened for a new answer ([editable]).
@Composable
private fun AnsweredAskCard(
    steps: List<AgentFeedItem.Question>,
    answerLabels: Map<String, String>,
    complete: Boolean,
    editable: Set<String>,
    onEditStep: (AgentFeedItem.Question) -> Unit,
) {
    AskCard(
        plan = false,
        heading = "Question",
        meta = steps.size.takeIf { it > 1 }?.let { "$it questions" },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            steps.forEach { step ->
                val onEdit = if (step.wireId in editable) ({ onEditStep(step) }) else null
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .then(
                            if (onEdit != null) {
                                Modifier
                                    .clip(RoundedCornerShape(DesignTokens.Radius.Sm))
                                    .clickable(role = Role.Button, onClick = onEdit)
                            } else {
                                Modifier
                            },
                        ),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Text(
                        step.header?.takeIf { it.isNotBlank() } ?: step.text,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.Top,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Box(modifier = Modifier.weight(1f)) {
                            AnsweredRow(stepAnswer(step, answerLabels))
                        }
                        if (onEdit != null) {
                            Icon(
                                ExpIcons.uiEdit,
                                contentDescription = "Change answer",
                                modifier = Modifier.size(12.dp).padding(top = 1.dp),
                                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                    }
                }
            }
            if (!complete) {
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

/**
 * EXP-1191: the ask's chrome, web `AskCard` parity — ONE glass card (12dp
 * padding) holding ONE top-aligned row: the 14dp glyph (a question's semantic
 * yellow help, a plan's glyph in the primary paint — EXP-820: never blue)
 * nudged 2dp down onto the first text line, then the column every line of
 * the card lives in, so the options indent under the text, not the glyph.
 * [heading] (12sp medium, tinted like the glyph) and the stepper's [meta]
 * caption sit 4dp above the content.
 */
@Composable
internal fun AskCard(
    plan: Boolean,
    heading: String?,
    meta: String?,
    modifier: Modifier = Modifier,
    /** The ask dock's focused card (ask-dock.json): the back arrow LEADS the
     *  header row, before the kind glyph. */
    onBack: (() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val tint = if (plan) MaterialTheme.colorScheme.primary else DesignTokens.Semantic.Yellow
    Row(
        modifier = modifier
            .fillMaxWidth()
            .glassCard()
            .padding(12.dp),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (onBack != null) {
            // Web `-my-1 -ml-1`: the 24dp button is pulled 4dp up, down and
            // left, so it never grows the header line.
            Box(
                Modifier.layout { measurable, constraints ->
                    val placeable = measurable.measure(constraints)
                    val pull = 4.dp.roundToPx()
                    layout(placeable.width - pull, placeable.height - 2 * pull) {
                        placeable.placeRelative(-pull, -pull)
                    }
                },
            ) { AskDockBackButton(onBack) }
        }
        Icon(
            if (plan) ExpIcons.codingPlan else ExpIcons.uiHelp,
            contentDescription = null,
            modifier = Modifier.padding(top = 2.dp).size(14.dp),
            tint = tint,
        )
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            if (heading != null || meta != null) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    if (heading != null) {
                        Text(
                            heading,
                            modifier = Modifier.weight(1f, fill = false),
                            style = MaterialTheme.typography.labelSmall.copy(
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Medium,
                            ),
                            color = tint,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (meta != null) {
                        Text(
                            meta,
                            modifier = Modifier.weight(1f, fill = false),
                            style = MaterialTheme.typography.labelSmall.copy(fontSize = 11.sp),
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            content()
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
// EXP-820: the card is the WHOLE input while it waits (the composer hides): a
// free-text row ("Type something.") and a plan's reject row expand an inline
// field in place of the composer, and an answered step of an open ask
// re-opens for a new answer ([editing]).
@Composable
internal fun QuestionCard(
    item: AgentFeedItem.Question,
    /** Still answerable per the feed — the session is blocked on this card. */
    active: Boolean,
    /** Live (and not ended) — whether this client may answer at all. */
    answerEnabled: Boolean,
    /** This client's send state — non-null means the card is locked. */
    state: AnswerState?,
    /** The stepper's "2 of 3" caption beside the heading (web `meta`). */
    stepLabel: String?,
    onAnswer: (List<String>, String?) -> Unit,
    /** EXP-820: (reject key, feedback) — a plan rejected WITH text: the key
     *  answers the card and the text goes out as the next message. */
    onPlanFollowUp: (String, String) -> Unit = { _, _ -> },
    /** The ask's already-answered steps, summarized above this one (EXP-588). */
    priorSteps: List<AgentFeedItem.Question> = emptyList(),
    /** Per prior step: its answer text, or null when unknown here. */
    priorAnswers: List<String?> = emptyList(),
    /** EXP-820: answered steps AFTER this one (an earlier step re-opened for
     *  editing sits above them), summarized under the options. */
    trailingSteps: List<AgentFeedItem.Question> = emptyList(),
    trailingAnswers: List<String?> = emptyList(),
    /** EXP-820: wire ids of the answered steps that may be re-opened. */
    editableSteps: Set<String> = emptySet(),
    onEditStep: (AgentFeedItem.Question) -> Unit = {},
    /** EXP-820: this card is an already-answered step re-opened for a new
     *  answer — the options render although resolved, the recorded answer
     *  pre-selected. */
    editing: Boolean = false,
    /** EXP-820: while editing, the ask's current step folded to one line. */
    foldedCurrent: AgentFeedItem.Question? = null,
    onBackToCurrent: (() -> Unit)? = null,
    /** What this client picked for THIS card — the resolved row's fallback
     *  when the desktop's resolution carried no answer text (EXP-588). */
    localAnswer: String? = null,
    /** A stepper step's heading when the question carries no header (web
     *  `Question` / `Review answers`). */
    stepHeading: String? = null,
) {
    // Both keyed on the card id: the stepper reuses ONE card slot across the
    // ask's steps, so an unkeyed `expanded` leaked a "Show more" from a long
    // step onto the next one (EXP-274).
    var expanded by remember(item.id) { mutableStateOf(false) }
    val recorded = item.answer?.takeIf { it.isNotBlank() } ?: localAnswer
    // EXP-820: a re-opened step starts from what it was answered with — the
    // matching rows selected, a typed answer back in its inline field.
    val typed = if (editing) typedAnswer(item, recorded) else null
    var picked by remember(item.id, editing) {
        mutableStateOf(if (editing) recordedKeys(item, recorded) else emptySet())
    }
    // EXP-820: the option whose inline field is open, and its text.
    var inlineKey by remember(item.id, editing) {
        mutableStateOf(if (typed != null) item.options.firstOrNull { it.freeText }?.key else null)
    }
    var inlineText by remember(item.id, editing) { mutableStateOf(typed.orEmpty()) }
    // A Failed state does NOT lock (EXP-334) — the card is answerable again
    // and renders the retry hint below instead of the sent row.
    val locked = state.locksCard()
    val answerable = active && answerEnabled && !locked
    // THE ask dock (ask-dock.json): a card drawn in the composer's slot caps
    // its body and may lead with the back arrow.
    val docked = LocalAskDockCard.current
    // Web `AskCard` parity (EXP-1191): ONE glass card, the glyph INSIDE it
    // beside the text, everything else — heading, body, options, status
    // lines — in the column the glyph leads, so options indent under the text.
    AskCard(
        plan = item.planMode,
        heading = if (item.planMode) "Plan ready" else item.header?.takeIf { it.isNotBlank() } ?: stepHeading,
        meta = stepLabel,
        // EXP-627: the store slide's pop-out rect is measured off the
        // question card (`PopRects`), iOS parity.
        modifier = Modifier.testTag("agent-feed-question"),
        onBack = docked?.onBack,
    ) {
        if (priorSteps.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                priorSteps.forEachIndexed { position, step ->
                    AnsweredStepRow(
                        step,
                        priorAnswers.getOrNull(position),
                        onEdit = if (step.wireId in editableSteps) ({ onEditStep(step) }) else null,
                    )
                }
            }
        }
        if (docked != null) {
            // Docked: the body never exceeds the fixture's
            // `bodyMaxHeightFraction` of the screen and scrolls inside; the
            // options stay below it (no Show more).
            Box(
                Modifier
                    .fillMaxWidth()
                    .heightIn(max = docked.bodyMaxHeight)
                    .verticalScroll(rememberScrollState()),
            ) { MarkdownView(item.text) }
        } else if (item.planMode) {
            // The plan is GFM markdown — always fully rendered, never
            // folded behind a Show more (EXP-197).
            MarkdownView(item.text)
        } else {
            FoldableMarkdown(item.text, expanded) { expanded = !expanded }
        }
        if (item.resolved && !editing) {
            // Resolved (EXP-197/EXP-249): the answer replaces the options.
            Box(Modifier.padding(top = 4.dp)) { AnsweredRow(recorded) }
        } else Column(
            // Web `mt-2 gap-1`: the options 8dp under the body (the
            // column's 4dp + this 4dp), stacked 4dp apart.
            modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            // EXP-788: ONE option list on every client — full-width
            // buttons with a numbered chip (1..9, the row's POSITION, the
            // digit a keyboard client presses), the description under the
            // label, and the plan's plain "Yes" (index 0 since EXP-788)
            // promoted as the primary button. EXP-820: a free-text row
            // and the plan's reject row (its LAST option) open an inline
            // field under themselves instead of sending at once.
            val options = item.options
            options.forEachIndexed { index, option ->
                val selected = option.key in picked
                val planReject = item.planMode && index == options.lastIndex
                val inline = option.freeText || planReject
                val fieldOpen = inline && inlineKey == option.key
                QuestionOptionButton(
                    option = option,
                    ordinal = index + 1,
                    primary = item.planMode && index == 0,
                    selected = selected || fieldOpen,
                    checked = if (item.multiSelect) selected else null,
                    enabled = answerable,
                    dimmed = locked,
                    onClick = {
                        when {
                            // Tapping the row again folds its field.
                            inline -> inlineKey = if (fieldOpen) null else option.key
                            item.multiSelect -> {
                                // Every picked key goes out at once when the
                                // card submits.
                                picked = if (selected) picked - option.key
                                else picked + option.key
                            }
                            else -> {
                                picked = setOf(option.key)
                                onAnswer(listOf(option.key), null)
                            }
                        }
                    },
                )
                if (fieldOpen) {
                    val text = inlineText.trim()
                    InlineAnswerField(
                        value = inlineText,
                        onValueChange = { inlineText = it },
                        placeholder = if (planReject) PLAN_FEEDBACK_PLACEHOLDER else FREE_TEXT_ANSWER_PLACEHOLDER,
                        // A plan's reject sends with or without feedback
                        // (empty = the plain reject the row used to be); a
                        // typed answer needs text.
                        sendEnabled = answerable && (planReject || text.isNotEmpty()),
                        onSend = {
                            when {
                                planReject && text.isNotEmpty() -> onPlanFollowUp(option.key, text)
                                planReject -> onAnswer(listOf(option.key), null)
                                item.multiSelect -> onAnswer((picked + option.key).toList(), text)
                                else -> onAnswer(listOf(option.key), text)
                            }
                        },
                    )
                }
            }
            if (item.multiSelect && (answerable || locked)) {
                // One frame carrying every picked key — plus the free-text
                // row when its field holds a reply (EXP-820).
                val freeKey = inlineKey?.takeIf { inlineText.isNotBlank() }
                val keys = picked.toList() + listOfNotNull(freeKey)
                val enabled = answerable && keys.isNotEmpty()
                GlassPill(
                    "Submit",
                    size = PillSize.Sm,
                    mode = PillMode.Select,
                    selected = true,
                    enabled = enabled,
                    onClick = { onAnswer(keys, freeKey?.let { inlineText.trim() }) },
                )
            }
            if (editing && onBackToCurrent != null) {
                TextButton(
                    onClick = onBackToCurrent,
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 2.dp),
                    modifier = Modifier.height(28.dp),
                ) {
                    Text(
                        BACK_TO_CURRENT_STEP_LABEL,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
            }
        }
        if (trailingSteps.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                trailingSteps.forEachIndexed { position, step ->
                    AnsweredStepRow(
                        step,
                        trailingAnswers.getOrNull(position),
                        onEdit = if (step.wireId in editableSteps) ({ onEditStep(step) }) else null,
                    )
                }
            }
        }
        if (foldedCurrent != null) {
            FoldedStepRow(foldedCurrent, onClick = onBackToCurrent)
        }
        if (locked) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                // Locked until the synced row flips: ONE in-flight caption
                // ×4 (plan-card.json `copy.answering`, web `AskPrompt`).
                CircularProgressIndicator(
                    modifier = Modifier.size(12.dp),
                    strokeWidth = 2.dp,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    PlanCard.Copy.ANSWERING,
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

/** EXP-820: the option keys a recorded answer names — the rows whose label
 *  equals it (a multi-select answer lists its labels joined by ", ") — or,
 *  when no label matches, the free-text row that carried a typed answer. */
private fun recordedKeys(item: AgentFeedItem.Question, answer: String?): Set<String> {
    if (answer.isNullOrBlank()) return emptySet()
    val parts = if (item.multiSelect) answer.split(", ") else listOf(answer)
    val keys = item.options.filter { it.label in parts }.map { it.key }.toSet()
    if (keys.isNotEmpty()) return keys
    return setOfNotNull(item.options.firstOrNull { it.freeText }?.key)
}

/** EXP-820: the recorded answer when it was TYPED — no option label matches
 *  it and the card has a free-text row to hold it; null otherwise. */
private fun typedAnswer(item: AgentFeedItem.Question, answer: String?): String? {
    if (answer.isNullOrBlank()) return null
    if (item.options.none { it.freeText }) return null
    val parts = if (item.multiSelect) answer.split(", ") else listOf(answer)
    return if (item.options.any { it.label in parts }) null else answer
}

/**
 * EXP-820: the inline field a free-text or plan-reject row opens — the
 * composer's own look (a borderless glass field, the round Send glyph), taking
 * focus (and the keyboard) as it appears; the IME's Send action is the button.
 */
@Composable
private fun InlineAnswerField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    sendEnabled: Boolean,
    onSend: () -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) { focusRequester.requestFocus() }
    val shape = RoundedCornerShape(GlassTokens.RowRadius)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(GlassTokens.RowFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeActive, shape)
            .padding(start = 4.dp, end = 2.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        GlassTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier
                .weight(1f)
                .focusRequester(focusRequester),
            placeholder = placeholder,
            maxLines = 4,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { if (sendEnabled) onSend() }),
            containerColor = Color.Transparent,
            // The row owns the chrome; the field is just its text.
            bordered = false,
        )
        ComposerSubmitButton(
            ExpIcons.uiSubmit,
            contentDescription = "Send",
            onClick = onSend,
            enabled = sendEnabled,
        )
    }
}

// The chosen answer of a resolved card — a dismissed ask carries none.
@Composable
internal fun AnsweredRow(answer: String?) {
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
                answer?.takeIf { it.isNotBlank() } ?: PlanCard.Copy.ANSWERED,
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
    }
}

/**
 * EXP-788: one option of a question or plan card as a REAL button — the same
 * row web, iOS and the IDE draw. [ordinal] is the row's 1-based position; it
 * is chipped for 1..9 only (the digits a keyboard client can press), a longer
 * list keeps its rows bare. EXP-820 (styleguide, no blue): [primary] (the
 * plan's plain "Yes") is the app's ONE emphatic paint — the solid primary fill
 * with dark content, like `glassButton(primary = true)` — and a [selected]
 * row lifts to the glass active fill + stroke. A multi-select row leads with
 * its checkbox ([checked]); the number chip TRAILS every row (EXP-1191).
 */
@Composable
internal fun QuestionOptionButton(
    option: QuestionOption,
    ordinal: Int,
    primary: Boolean,
    selected: Boolean,
    checked: Boolean?,
    enabled: Boolean,
    /** Locked: the answer is out and the row waits on the ack. */
    dimmed: Boolean,
    onClick: () -> Unit,
    /** EXP-1280 (plan-card.json `card`): a picked SINGLE-choice row trails a
     *  foreground check (no leading dot). */
    trailingCheck: Boolean = false,
    /** EXP-1280: a CLOSED card's row — true = a pick (a leading emerald check
     *  on the active fill), false = the rest (muted at `inertOpacity`, no
     *  fill); null = a live row. */
    closed: Boolean? = null,
) {
    val shape = RoundedCornerShape(PlanCard.Look.ROW_RADIUS.dp)
    val content = if (primary) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
    val rest = closed == false
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(if (dimmed) PlanCard.Look.INERT_OPACITY else 1f)
            .clip(shape)
            .background(
                when {
                    primary -> MaterialTheme.colorScheme.primary
                    rest -> Color.Transparent
                    selected || closed == true -> GlassTokens.RowFillActive
                    else -> GlassTokens.RowFill
                },
                shape,
            )
            .then(
                // The solid primary fill has no hairline (the pill's rule);
                // an outline row wears the glass card stroke (web `outline`).
                if (primary || rest) {
                    Modifier
                } else {
                    Modifier.border(
                        GlassTokens.Hairline,
                        if (selected || closed == true) GlassTokens.StrokeActive else GlassTokens.StrokeCard,
                        shape,
                    )
                },
            )
            .then(
                if (enabled && closed == null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier,
            )
            .heightIn(min = PlanCard.Look.ROW_MIN_HEIGHT.dp)
            .padding(horizontal = 12.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (closed != null) {
            // The closed look (web `PlanCardOptions` closed): the check slot
            // stays reserved so the labels line up.
            Box(Modifier.size(14.dp), contentAlignment = Alignment.Center) {
                if (closed) {
                    Icon(ExpIcons.uiCheck, contentDescription = null, modifier = Modifier.size(14.dp), tint = LiveGreen)
                }
            }
        } else if (checked != null) {
            Icon(
                if (checked) ExpIcons.uiSelected else ExpIcons.uiUnselected,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = if (checked) content else content.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(
                option.label,
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
                color = if (rest) MaterialTheme.colorScheme.onSurfaceVariant else content,
                modifier = if (rest) Modifier.alpha(PlanCard.Look.INERT_OPACITY) else Modifier,
            )
            option.description?.takeIf { it.isNotBlank() }?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.labelSmall,
                    color = content.copy(alpha = if (primary) TextEmphasis.Secondary else TextEmphasis.Tertiary),
                    modifier = if (rest) Modifier.alpha(PlanCard.Look.INERT_OPACITY) else Modifier,
                )
            }
        }
        if (trailingCheck) {
            Icon(ExpIcons.uiCheck, contentDescription = null, modifier = Modifier.size(14.dp), tint = content)
        }
        if (ordinal in 1..9) {
            // EXP-1191 (web `kbd` parity): the number key TRAILS the row — a
            // small outlined chip (radius 4, 1dp stroke), monospace 10sp,
            // 16dp tall, muted; on the primary row primary-foreground /30.
            val chipShape = RoundedCornerShape(4.dp)
            Box(
                modifier = Modifier
                    .height(16.dp)
                    .border(
                        1.dp,
                        if (primary) content.copy(alpha = 0.3f) else GlassTokens.StrokeCard,
                        chipShape,
                    )
                    .padding(horizontal = 4.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    "$ordinal",
                    // The digit a keyboard client presses — monospace, like
                    // every other key in the app (web `font-mono`).
                    style = MaterialTheme.typography.labelSmall.copy(
                        fontFamily = FontFamily.Monospace,
                        fontWeight = FontWeight.Normal,
                        fontSize = 10.sp,
                        lineHeight = 16.sp,
                    ),
                    color = if (primary) content.copy(alpha = 0.8f) else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

// A permission prompt the agent hit (EXP-249) — the card itself has nothing to
// press (the desktop TUI owns the decision), but a reply typed below reaches
// the same prompt, so say so instead of dead-ending the viewer (EXP-529).
/**
 * A transient API error the agent hit mid-turn (`api_error`): a quiet inline
 * row — never the rate-limit banner, never a wall. The message is the CLI's
 * own words, shown as plain text (not markdown); [errorType] trails it dimmer.
 */
@Composable
private fun ApiErrorRow(message: String, errorType: String?) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.uiWarning,
            contentDescription = null,
            modifier = Modifier.padding(top = 2.dp).size(12.dp),
            tint = MaterialTheme.colorScheme.error.copy(alpha = 0.7f),
        )
        Text(
            message,
            style = MaterialTheme.typography.bodySmall,
            color = muted,
            modifier = Modifier.weight(1f),
        )
        if (!errorType.isNullOrBlank()) {
            Text(
                remember(errorType) { errorType.replace('_', ' ') },
                style = MaterialTheme.typography.labelSmall,
                color = muted.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(top = 1.dp),
            )
        }
    }
}

@Composable
private fun PermissionRow(tool: String, detail: String?, showHint: Boolean) {
    Column(
        modifier = Modifier.fillMaxWidth(),
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
                style = transcriptToolStyle(),
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

/**
 * EXP-916: one row of a LANE's projection ([projectLaneRows]) — an
 * edited-files card, a run of our own MCP calls (EXP-948), or the single item's
 * own row. A lane holds no generic tool runs and no nested subagents, so those
 * never reach here.
 */
@Composable
private fun LaneRow(row: AgentFeedRow, liveItemId: Long?, live: Boolean, nested: Boolean = false) {
    when (row) {
        is AgentFeedRow.Edits -> EditedFilesCard(items = row.items, liveItemId = liveItemId)
        is AgentFeedRow.ExpRun -> ExpToolGroupRow(items = row.items, nested = nested)
        is AgentFeedRow.Single -> SubagentItemRow(row.item, live = live, nested = nested)
        else -> Unit
    }
}

// EXP-773: one row of a subagent's conversation — its prose, a turn addressed
// to it, or one of its tool calls. Anything else a group somehow collected
// renders nothing rather than breaking the feed. `live` = the RUN still
// lives: an ended session's open reasoning never shimmers (main-site rule).
@Composable
private fun SubagentItemRow(item: AgentFeedItem, live: Boolean, nested: Boolean = false) {
    when (item) {
        is AgentFeedItem.Tool -> ToolRow(item, nested = nested)
        is AgentFeedItem.Narration -> NarrationBubble(item.text, nested = nested)
        is AgentFeedItem.ApiError -> ApiErrorRow(item.message, item.errorType)
        is AgentFeedItem.UserMessage -> UserMessageBubble(item.text, nested = nested)
        is AgentFeedItem.Thought -> ThoughtRow(item, running = live && !item.ended)
        else -> Unit
    }
}

/**
 * One tool call. EXP-846: a call to one of OUR tools renders as itself
 * ([ExpToolCallRow]) — the Exponential mark, the caption in the tense the call
 * is in, and what came back; every other tool keeps the generic wrench row.
 */
@Composable
private fun ToolRow(
    item: AgentFeedItem.Tool,
    nested: Boolean = false,
    /** EXP-895: this call is the ONE still running ([liveToolRowId]). The
     *  transcript runs inside the flow, so this row — and only this row — opens
     *  itself. */
    live: Boolean = false,
) {
    // EXP-1279: a code-mode call is its Script card, the calls it made
    // grouped by tool.
    if (item.isScriptCall) {
        val card = remember(item) { ScriptCards.of(item.scriptCardInput()) }
        ScriptCardView(
            card = card,
            running = !item.settled,
            failed = item.failed,
            source = item.script?.source,
            modifier = if (nested) Modifier.padding(vertical = 2.dp) else Modifier,
        )
        return
    }
    // EXP-1241: a SETTLED comment call is the comment itself, in the comment
    // look; running or failed it keeps its ordinary row.
    if (isCommentCallCard(item)) {
        Box(modifier = if (nested) Modifier.padding(vertical = 2.dp) else Modifier) { CommentCallRow(item) }
        return
    }
    val exp = remember(item.name, item.settled) {
        ExpToolDisplay.forName(item.name, item.settled)
    }
    if (exp != null) {
        ExpToolCallRow(item = item, display = exp, nested = nested, live = live)
        return
    }
    TraceToolRow(item = item, live = live)
}

/**
 * EXP-1324: one tool call as its Trace line (`Trace.tool`): the kind's icon,
 * "Running ./gradlew …" (shimmering) → "Ran ./gradlew …", rose when it
 * failed. A settled call that printed is COLLAPSIBLE and opens onto its
 * output in THE CodeBlock (`execute` = the `ansi` Terminal pattern, a failed
 * call the destructive block). A settled `execute` whose output parses as a
 * test run with failures IS the failed-tests line instead ("Failed: 1 of 12
 * tests in StartupBenchmark"), its content the output.
 *
 * EXP-1206: [live] — the ONE call still running ([liveToolRowId]) — is the
 * line ALONE: nothing streams under it, so the transcript never jumps; a
 * settled row's output stays folded until the reader's tap. A call that
 * never settled and is not live reads settled.
 */
@Composable
private fun TraceToolRow(item: AgentFeedItem.Tool, live: Boolean) {
    var open by remember { mutableStateOf(false) }
    val model = remember(item.toolKind, item.settled, item.output, item.name, item.detail, item.failed, live) {
        val tests = if (item.toolKind == "execute" && item.settled && item.output != null) {
            TraceModels.failedTests(TraceModels.parseTestResults(item.output))
        } else {
            null
        }
        tests ?: TraceModels.tool(
            toolKind = item.toolKind ?: "other",
            name = item.name,
            detail = item.detail,
            settled = item.settled || !live,
            failed = item.failed,
        )
    }
    val output = item.output
    Trace(size = TraceSize.Sm, modifier = Modifier.testTag("session-tool-line")) {
        if (output == null || live) {
            TraceLine(model)
        } else {
            TraceCollapsible(
                open = open,
                onOpenChange = { open = it },
                trigger = { isOpen, toggle ->
                    TraceLine(
                        model,
                        open = isOpen,
                        onClick = toggle,
                        onClickLabel = if (isOpen) "Hide the output" else "Show the output",
                    )
                },
            ) {
                CodeBlock(
                    code = output,
                    language = if (item.toolKind == "execute") CodeBlockLanguage.Ansi else CodeBlockLanguage.Plain,
                    destructive = item.failed || model.icon == TraceSpec.ICON_FAILED,
                    scrollToEnd = true,
                    modifier = Modifier.testTag("session-tool-output"),
                )
            }
        }
    }
}

/**
 * EXP-1324: the reasoning line — "Thinking" (shimmering) while the thought
 * is open, "Thought for 5s" once its `ended` landed (`Trace.reasoning`).
 * Never collapsible: no text rides the wire.
 */
@Composable
private fun ThoughtRow(item: AgentFeedItem.Thought, running: Boolean) {
    Trace(size = TraceSize.Sm, modifier = Modifier.testTag("session-thought-line")) {
        TraceLine(TraceModels.reasoning(running, item.durationMs))
    }
}

/**
 * EXP-1324: a subagent as its Trace line — "Running the Explore agent"
 * (shimmering) → "Ran the Explore agent" with the duration as the outline
 * badge (`Trace.subagent`, ticking while it runs); the line opens the
 * subagent's own tab, where its rows live. The label is the agent's TYPE.
 */
@Composable
private fun SubagentLineRow(run: AgentFeedRow.SubagentRun, live: Boolean, onOpen: () -> Unit) {
    val running = live && !run.completed
    val now = rememberTickingNow(running)
    val type = run.agentType.trim()
    val model = TraceModels.subagent(
        label = if (type == SUBAGENT_FALLBACK_TYPE) "" else type,
        running = running,
        failed = false,
        durationMs = if (run.completed || running) run.durationMs(now) else null,
    )
    Trace(size = TraceSize.Sm) {
        TraceLine(
            model,
            badge = true,
            chevron = true,
            onClick = onOpen,
            onClickLabel = run.label,
            modifier = Modifier.testTag("session-subagent-line"),
        )
    }
}

/**
 * EXP-846: one call to an EXPONENTIAL MCP tool. The product's own mark instead
 * of the generic wrench, the contract caption in the tense the call is in
 * ("Creating issue" → "Created issue"), the subject the call named, and — once
 * it settles — a small preview of the row it touched. Nothing is forced: a call
 * whose answer named nothing is the mark and the caption, which already says
 * more than `mcp__exponential__exponential_issues_create` ever did.
 *
 * A FAILED call keeps the generic failed styling (the rose caption): what it
 * was trying to do still reads, what came back is nothing to preview.
 */
@Composable
private fun ExpToolCallRow(
    item: AgentFeedItem.Tool,
    display: ExpToolRow,
    nested: Boolean,
    live: Boolean = false,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (nested) Modifier.padding(vertical = 2.dp) else Modifier),
    ) {
        // EXP-1324: our call is a Trace line too — the mark, the sentence,
        // the SUBJECT (the input field the contract's `subjectKey` names,
        // published as the call's `detail`) muted after it; rose when it
        // failed, shimmering while it runs.
        val subject = item.detail?.takeIf { it.isNotBlank() && display.subjectKey.isNotEmpty() }
        Trace(size = TraceSize.Sm, modifier = Modifier.testTag("session-exp-tool-line")) {
            TraceLine(
                title = remember(display.caption, subject) {
                    traceTitleWithSubject(display.caption, subject?.let { middleTruncate(it, EXP_TOOL_SUBJECT_MAX) })
                },
                leading = { ExponentialMark(size = 14.dp) },
                status = when {
                    item.failed -> TraceStatus.Error
                    live && !item.settled -> TraceStatus.Running
                    else -> null
                },
            )
        }
        if (item.settled && !item.failed) {
            if (display.result == ExpToolDisplay.RESULT_RESULTS) {
                // EXP-933: the run published its report — one tap to the
                // Work screen's Results face, not a preview of it.
                val openResults = LocalOpenResults.current
                if (openResults != null) {
                    ExpToolPreviewRow(onClick = openResults) {
                        Icon(
                            ExpIcons.workResults,
                            contentDescription = null,
                            modifier = Modifier.size(12.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                        Text(
                            OPEN_RESULTS_LABEL,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface,
                            maxLines = 1,
                        )
                    }
                }
            } else if (display.result == ExpToolDisplay.RESULT_PICTURE) {
                ExpToolPicture(attachmentId = item.preview?.id)
            } else {
                ExpToolPreview(display = display, preview = item.preview)
            }
        }
    }
}

/**
 * EXP-1172: the picture an `exponential_sessions_show` call filed — ONE tile
 * under the row, the run's synced results looked up by the call's answer id
 * (`preview.id`). The Results face's tile at the inline base height, fitted to
 * the column by the same one-factor rule; caption = the call's text, else the
 * label; a tap opens the same full-size viewer. Nothing until the picture
 * synced (an upload in flight) or once it was removed.
 */
@Composable
private fun ExpToolPicture(attachmentId: String?) {
    val results = LocalSessionResults.current
    val entry = remember(results, attachmentId) { sessionResultPicture(results, attachmentId) } ?: return
    InlineResultPicture(
        entry = entry,
        modifier = Modifier
            .padding(start = EXP_TOOL_PREVIEW_INSET, top = 6.dp)
            .testTag("exp-tool-picture"),
    )
}

/**
 * One published picture as the transcript's inline tile (EXP-1172) — fitted to
 * the width at the inline height, opening the full preview. Shared by a
 * settled `sessions_show` row and the thread's pictures (EXP-1175).
 */
@Composable
private fun InlineResultPicture(entry: SessionResultEntry, modifier: Modifier = Modifier) {
    var open by remember { mutableStateOf(false) }
    BoxWithConstraints(
        modifier = Modifier
            .fillMaxWidth()
            .then(modifier),
    ) {
        val available = maxWidth.value.toInt()
        val tileHeight = remember(entry, available) {
            sessionResultTileHeightFitting(listOf(entry), available, SESSION_INLINE_TILE_HEIGHT)
        }
        ResultTile(
            entry = entry,
            tileHeight = tileHeight,
            onOpen = { open = true },
            caption = sessionResultTileCaption(entry),
        )
    }
    if (open) ResultPreviewDialog(entry = entry, onDismiss = { open = false })
}

/** How much of a subject a row shows before it middle-truncates — a title is
 *  not a path, so it gets less room than a file does. */
private const val EXP_TOOL_SUBJECT_MAX = 48

/**
 * EXP-846: what came back, per the contract's result kind — the issue preview
 * an issue-ref pill opens, a PR's link, `N results` for a list, a name chip for
 * everything else that names a row, and nothing at all for `none`.
 */
@Composable
private fun ExpToolPreview(display: ExpToolRow, preview: ToolResultPreview?) {
    val result = preview ?: return
    // EXP-920: an answer that named its entities renders them as chips — one
    // per group, each opening a preview sheet. A pre-EXP-920 publisher sends
    // no refs and the single-subject rendering below stays exactly as it was.
    if (result.refs.isNotEmpty()) {
        EntityRefChips(
            refs = result.refs,
            modifier = Modifier.padding(start = EXP_TOOL_PREVIEW_INSET, top = 4.dp),
        )
        return
    }
    when (display.result) {
        ExpToolDisplay.RESULT_ISSUE -> ExpToolIssuePreview(result)
        ExpToolDisplay.RESULT_PR -> {
            val url = result.url?.takeIf { it.isNotBlank() } ?: return
            val uriHandler = LocalUriHandler.current
            ExpToolPreviewRow(onClick = { uriHandler.openUri(url) }) {
                Icon(
                    ExpIcons.prOpen,
                    contentDescription = null,
                    modifier = Modifier.size(12.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
                Text(
                    remember(url) { middleTruncate(url, EXP_TOOL_SUBJECT_MAX) },
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        ExpToolDisplay.RESULT_LIST -> {
            val count = result.count ?: return
            Text(
                "$count " + if (count == 1) "result" else "results",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(start = EXP_TOOL_PREVIEW_INSET, top = 2.dp),
            )
        }
        // A row that has a NAME: the session, board, action or comment the
        // call touched. Its identifier wins when it has one.
        ExpToolDisplay.RESULT_SESSION,
        ExpToolDisplay.RESULT_BOARD,
        ExpToolDisplay.RESULT_ACTION,
        ExpToolDisplay.RESULT_COMMENT,
        -> {
            val label = result.identifier?.takeIf { it.isNotBlank() }
                ?: result.title?.takeIf { it.isNotBlank() }
                ?: return
            ExpToolPreviewRow(onClick = null) {
                Text(
                    remember(label) { middleTruncate(label, EXP_TOOL_SUBJECT_MAX) },
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        // `results` renders its `Open Results` button in [ExpToolCallRow],
        // `picture` its tile (EXP-1172).
        ExpToolDisplay.RESULT_RESULTS, ExpToolDisplay.RESULT_PICTURE -> Unit
        // `none`, and any result kind a newer contract invents: the caption
        // already said what happened.
        else -> Unit
    }
}

/**
 * EXP-846: the issue a call answered with, as the SAME preview an issue-ref
 * pill opens — the synced row's status glyph, its identifier and title, tapping
 * through to the issue. A preview naming an issue this client has not synced
 * (another team's, a board in the trash) still renders what the answer said,
 * inert: the agent did the thing, and a blank row would deny it.
 */
@Composable
private fun ExpToolIssuePreview(preview: ToolResultPreview) {
    val identifier = preview.identifier?.takeIf { it.isNotBlank() }
    val target = identifier?.let { LocalIssueRefs.current?.resolve(it) }
    val title = target?.title?.takeIf { it.isNotBlank() }
        ?: preview.title?.takeIf { it.isNotBlank() }
    if (identifier == null && title == null) return
    val refs = LocalIssueRefs.current
    // EXP-885: the SHARED chip, so a tool result names an issue exactly like
    // the `#IDENTIFIER` chip in the line above it does — this used to be the
    // preview shell's glass row with its own glyph size and label rung.
    IssueChip(
        identifier = identifier.orEmpty(),
        title = title,
        status = target?.resolvedStatus,
        onClick = if (target != null && refs?.canOpen == true) {
            { refs.onOpen(target) }
        } else {
            null
        },
        modifier = Modifier.padding(start = EXP_TOOL_PREVIEW_INSET, top = 4.dp),
    )
}

/** The shell every settled-call preview sits in: a glass row under the
 *  caption, inset past the mark, tappable only when it has somewhere to go. */
@Composable
private fun ExpToolPreviewRow(
    onClick: (() -> Unit)?,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier = Modifier
            .padding(start = EXP_TOOL_PREVIEW_INSET, top = 4.dp)
            .glassRow()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        content = content,
    )
}

/** Aligned under the caption, past the mark (12dp) and its 8dp gap. */
private val EXP_TOOL_PREVIEW_INSET = 20.dp

/** The folded-open output's ceiling — the contract's number (web `max-h-72`). */
private val ToolDiffMaxHeight = DomainContract.diffUiInlineDiffMaxHeight.dp

// A run of ≥2 consecutive tool calls collapsed into one row (EXP-97),
// expandable to the individual rows. EXP-785: the caption says what happened
// ("Ran 4 commands · edited 2 files · 1 failed") — `ToolGroupSummary`, the
// derivation shared ×4 — instead of counting calls. While the run is the
// trailing row of a live session, the latest call stays visible under the
// caption so the viewer still sees live progress.
@Composable
private fun ToolGroupRow(items: List<AgentFeedItem.Tool>, liveTail: Boolean) {
    var expanded by remember { mutableStateOf(false) }
    val caption = remember(items) {
        ToolGroupSummary.summarize(
            items.map { ToolCallSummary(it.toolKind ?: "other", it.detail, it.failed) },
        )
    }
    // EXP-1324: the fold is a TraceCollapsible over the members' own lines;
    // while it is the live tail the caption shimmers and the latest call's
    // LINE stays under it while folded (EXP-1206: never its output).
    Trace(size = TraceSize.Sm, modifier = Modifier.testTag("session-tool-group")) {
        TraceCollapsible(
            open = expanded,
            onOpenChange = { expanded = it },
            trigger = { isOpen, toggle ->
                TraceLine(
                    title = caption,
                    icon = ExpIcons.traceTool,
                    status = if (liveTail) TraceStatus.Running else null,
                    open = isOpen,
                    onClick = toggle,
                )
            },
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(TraceSize.Sm.geometry.lineGap.dp)) {
                items.forEach {
                    ToolRow(it, nested = true, live = liveTail && it.id == items.last().id)
                }
            }
        }
        if (!expanded && liveTail) {
            Box(modifier = Modifier.padding(start = TraceSize.Sm.geometry.contentInset.dp)) {
                ToolRow(items.last(), nested = true, live = true)
            }
        }
    }
}

/**
 * EXP-948: a run of consecutive calls to ONE of our own MCP tools, as ONE row.
 * The generic group's fold — chevron, caption, expands to the individual rows —
 * but under the Exponential mark the single call rows already wear, and
 * captioned from the contract ("Reading 3 issues" → "Read 3 issues", plus
 * ` · N failed`): our tools are the most meaningful rows a transcript has, and
 * a pile of them used to disappear inside "N other tools".
 */
@Composable
private fun ExpToolGroupRow(items: List<AgentFeedItem.Tool>, nested: Boolean = false) {
    var expanded by remember { mutableStateOf(false) }
    val caption = remember(items) { ExpToolGroup.expToolGroupCaption(items) }
    Trace(
        size = TraceSize.Sm,
        modifier = Modifier
            .fillMaxWidth()
            .then(if (nested) Modifier.padding(vertical = 2.dp) else Modifier)
            .testTag("session-exp-tool-group"),
    ) {
        TraceCollapsible(
            open = expanded,
            onOpenChange = { expanded = it },
            trigger = { isOpen, toggle ->
                TraceLine(
                    title = caption,
                    leading = { ExponentialMark(size = 14.dp) },
                    open = isOpen,
                    onClick = toggle,
                )
            },
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(TraceSize.Sm.geometry.lineGap.dp)) {
                items.forEach { ToolRow(it, nested = true) }
            }
        }
        if (!expanded) {
            // EXP-1172: a picture never hides in a fold — a folded run of
            // `exponential_sessions_show` calls draws its calls' tiles in call
            // order under the caption (expanded, each row draws its own).
            items.forEach { item ->
                if (!item.settled || item.failed) return@forEach
                val display = ExpToolDisplay.forName(item.name, settled = true) ?: return@forEach
                if (display.result == ExpToolDisplay.RESULT_PICTURE) {
                    ExpToolPicture(attachmentId = item.preview?.id)
                }
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
 * THE session composer ×4 (dispatcher roots, thread 2eb4b1a1): one box for the
 * session view, the THREAD (fed by its ROOT session's view model, so the
 * message rides `sessions.send({sessionId: root})`) and the agent channel —
 * the queue strip, the `/` menu (steerCommands ∪ agent commands) and the
 * `@`/`#`/`:` autocomplete above the card, and the card's footer: `+`, the
 * ONE `/model` picker, the context ring and Send ⇄ Stop while busy. A
 * dispatcher (`sessions.dispatch`) never shows the plan chip.
 *
 * The host's own blocks slot in around the menus: [banners] (status /
 * rate-limit lines), [beforeMenus] (the background-work strip) and
 * [beforeComposer] (the task list). [onOpenUsage] opens [SessionUsageSheet];
 * null = the ring is a gauge only.
 */
@Composable
internal fun SessionComposer(
    viewModel: AgentSessionViewModel,
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = STEER_COMPOSER_PLACEHOLDER,
    capsuleTestTag: String = "steer-composer-capsule",
    onOpenUsage: (() -> Unit)? = null,
    /** The queue strip above the card. The thread + agent channel pass false:
     *  their Conversation already renders queued rows with the Remove X
     *  (EXP-1317). */
    showQueue: Boolean = true,
    trailing: (@Composable () -> Unit)? = null,
    merge: (@Composable () -> Unit)? = null,
    banners: @Composable ColumnScope.() -> Unit = {},
    beforeMenus: @Composable ColumnScope.() -> Unit = {},
    beforeComposer: @Composable ColumnScope.() -> Unit = {},
    /** THE ask dock (ask-dock.json): drawn in the composer's slot while a
     *  card waits on the viewer; null = the slot stays empty. */
    dock: (@Composable () -> Unit)? = null,
    /** Whether a card waits on the viewer (the thread: ANY pending card in
     *  the tree); null = this session's own live feed decides. */
    askPending: Boolean? = null,
) {
    val session by viewModel.session.collectAsStateWithLifecycle()
    val phase by viewModel.phase.collectAsStateWithLifecycle()
    val activity by viewModel.activity.collectAsStateWithLifecycle()
    val pendingImages by viewModel.pendingImages.collectAsStateWithLifecycle()
    val steerSending by viewModel.steerSending.collectAsStateWithLifecycle()
    val steerImageError by viewModel.steerImageError.collectAsStateWithLifecycle()
    val draft by viewModel.draft.collectAsStateWithLifecycle()
    val sessionConfig by viewModel.sessionConfig.collectAsStateWithLifecycle()
    val sessionUsage by viewModel.sessionUsage.collectAsStateWithLifecycle()
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    // EXP-1211: the session's quick replies; a refused tap = `Couldn't send the reply`.
    val composerReplies by viewModel.composerReplies.collectAsStateWithLifecycle()
    val replyPending by viewModel.replyPending.collectAsStateWithLifecycle()
    val replyError by viewModel.replyError.collectAsStateWithLifecycle()
    com.exponential.app.ui.components.FailedActionToast(replyError, onShown = viewModel::clearReplyError)
    // FEED-79: a refused send / unqueue = `Couldn't send the message`.
    val sendError by viewModel.sendError.collectAsStateWithLifecycle()
    com.exponential.app.ui.components.FailedActionToast(
        sendError?.let { com.exponential.app.ui.components.FailedAction("send", "the message", it.ifBlank { null }) },
        onShown = viewModel::clearSendError,
    )
    // A send the composer refused outright (a slash command with attachments)
    // is its own plain toast, the web composer's copy.
    val sendRefusal by viewModel.sendRefusal.collectAsStateWithLifecycle()
    val refusalToaster = LocalToaster.current
    LaunchedEffect(sendRefusal) {
        val refusal = sendRefusal ?: return@LaunchedEffect
        refusalToaster.error(refusal)
        viewModel.clearSendRefusal()
    }
    // The `#` typeahead's handler (the composer sits OUTSIDE the feed's
    // provider: what the user types stays the stored `#IDENTIFIER`).
    val composerRefHandler = remember(issueRefCandidates) {
        IssueRefHandler(issueRefCandidates, searchServer = viewModel::searchIssueRefs) { }
    }
    val catalogAgent = SlashCommands.agentId(session?.cli, sessionConfig != null)
    val sessionEnded = session?.isEnded == true
    // The session view's ONE rule ×4: a card waiting on this viewer takes the
    // composer's place (EXP-820, `SessionTree.composerYieldsToCard`).
    val awaitingInput = phase == AgentPhase.Live &&
        remember(activity.feed) { activeQuestionIds(activity.feed) }.isNotEmpty()
    val composerHidden = if (askPending != null) {
        askPending
    } else {
        SessionTree.composerYieldsToCard(
            live = phase == AgentPhase.Live && !sessionEnded,
            pendingAsk = awaitingInput,
        )
    }
    // EXP-790: mid-turn, the empty field's send glyph is a Stop.
    val agentWorking = agentWorking(
        live = phase == AgentPhase.Live,
        sessionEnded = sessionEnded,
        turnState = activity.turnState,
        awaitingInput = awaitingInput,
        needsInput = session?.needsInput == true,
        blocked = AgentUsagePresentation.parseBlocked(session?.blocked) != null,
        compacting = activity.compacting != null,
    )
    // The context ring ×4 (web `percent !== null`): only once the run has
    // reported its context numbers — never an empty circle.
    val contextPercent = AgentUsagePresentation.contextPercent(sessionUsage)
    val hasUsage = contextPercent != null
    val hostLaunchDefaults by viewModel.hostLaunchDefaults.collectAsStateWithLifecycle()

    // EXP-1334: THE session composer's "+" = composer-menu.json's rows; in a
    // running session Plan / Effort / Ultracode / Computer use / MCP servers
    // change live (`sessions.configure`).
    val picks by viewModel.configurePicks.collectAsStateWithLifecycle()
    val mcpServers by viewModel.mcpServers.collectAsStateWithLifecycle()
    val canComputerUse by viewModel.hostCanToggleComputerUse.collectAsStateWithLifecycle()
    val configureError by viewModel.configureError.collectAsStateWithLifecycle()
    com.exponential.app.ui.components.FailedActionToast(
        configureError?.let { com.exponential.app.ui.components.FailedAction("change", "the session options", it.ifBlank { null }) },
        onShown = viewModel::clearConfigureError,
    )
    // EXP-1252: the options line's Device pin; a refused move =
    // `Couldn't move the session`.
    val deviceMove by viewModel.deviceMove.collectAsStateWithLifecycle()
    val moveError by viewModel.moveError.collectAsStateWithLifecycle()
    com.exponential.app.ui.components.FailedActionToast(
        moveError?.let { com.exponential.app.ui.components.FailedAction("move", "the session", it.ifBlank { null }) },
        onShown = viewModel::clearMoveError,
    )
    LaunchedEffect(session?.teamId) { viewModel.loadMcpServers() }
    val composerAgent = session?.cli?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT
    val plusMenu = SessionComposerMenu(
        agent = composerAgent,
        effort = picks.effort ?: session?.effort.orEmpty(),
        ultracode = picks.ultracode ?: (session?.ultracode == true),
        computerUse = picks.computerUse ?: (session?.computerUse == true),
        canComputerUse = canComputerUse,
        mcpServers = mcpServers,
        // Until `sessions.mcpPicks` lands the row reads empty (web parity),
        // never the team defaults.
        mcpServerIds = picks.mcpServerIds ?: emptyList(),
        issues = issueRefCandidates.map {
            IssueOption(
                id = it.issueId,
                identifier = it.identifier,
                title = it.title,
                repositoryId = null,
                status = null,
                priority = null,
                description = it.description,
                createdAt = it.createdAt,
                updatedAt = it.updatedAt,
            )
        },
        planMode = picks.planMode
            ?: (planModeBadge(sessionConfig) != null || (sessionConfig == null && session?.planMode == true)),
        // An ENDED session takes no configure: the rows and the Plan switch
        // read-only (the server refuses `This session has ended`).
        readOnly = sessionEnded,
        onConfigure = viewModel::configure,
        onImplement = viewModel::implementIssues,
    )

    // Steer attach (EXP-511, wave D: ANY file, images included) — the system
    // document picker feeds the VM's pending list; the upload rides the
    // SESSION route (EXP-698), so every kind of run accepts one.
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val filePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris: List<Uri> ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            // ContentResolver reads can stream from a cloud-backed provider —
            // [readComposerPick] stays off the main thread. One pick at a time
            // through the VM, so its caps (4 images, 4 files) + toasts apply.
            for (uri in uris) when (val picked = readComposerPick(context, uri)) {
                is ComposerPick.Read ->
                    viewModel.addPendingImage(picked.uri, picked.bytes, picked.filename, picked.mime)
                is ComposerPick.Refused -> viewModel.refusePendingAttachment(picked.message)
            }
        }
    }

    // EXP-724: the `/` command menu. `slashSelected` is the highlighted row
    // (↑/↓ wrap); `slashDismissedFor` is the draft Escape/Back was pressed on
    // — the menu stays shut until the text changes, so dismissing is not
    // undone by the next recomposition. `slashConfirm` holds the command
    // awaiting its "context is discarded" confirmation.
    var slashSelected by remember { mutableIntStateOf(0) }
    var slashDismissedFor by remember { mutableStateOf<String?>(null) }
    var slashConfirm by remember { mutableStateOf<SlashCommand?>(null) }
    // NEW-2: the builtins, then the team's prompts (a prompt's pick puts its
    // body in the draft).
    val slashMatches by viewModel.slashCandidates.collectAsStateWithLifecycle()
    val promptFailure by viewModel.promptFailure.collectAsStateWithLifecycle()
    com.exponential.app.ui.components.FailedActionToast(promptFailure, onShown = viewModel::clearPromptFailure)
    val acceptSlash: (com.exponential.app.domain.SlashCandidate) -> Unit = { candidate ->
        when (candidate) {
            is com.exponential.app.domain.SlashCandidate.Command -> {
                viewModel.setDraft(candidate.command.insertion)
                // Accepting closes the menu; it reopens as soon as the
                // draft changes again (never auto-sends).
                slashDismissedFor = candidate.command.insertion
            }
            is com.exponential.app.domain.SlashCandidate.Prompt -> {
                slashDismissedFor = draft
                viewModel.insertPrompt(candidate.prompt.id)
            }
        }
    }
    val slashMenuOpen = slashMatches.isNotEmpty() && slashDismissedFor != draft
    // A changed candidate list can never leave the highlight past its end.
    LaunchedEffect(slashMatches) { slashSelected = 0 }

    // ── EXP-802/EXP-805: the composer's `@` / `#` / `:` autocomplete ─────────
    // The steer composer speaks the same three triggers as the issue comment
    // composer, so the field's TextFieldValue is hoisted HERE: the menu mounts
    // as a plain sibling above the composer (exactly like the `/` menu), and a
    // picked row splices against this very value. The draft string still lives
    // in the connection — it has to survive a reconnect — so this is the
    // caret's home, not the text's.
    var composerField by remember { mutableStateOf(TextFieldValue(draft, TextRange(draft.length))) }
    // The ARMED latch (BlockTextField parity, EXP-322): only a TEXT change may
    // OPEN the menu. Moving the caret back into an existing `#EXP-238` or
    // `@ann@example.com` must not pop it — on a phone the caret moves on every
    // tap, and a menu that reopens on a tap is a menu nothing can dismiss.
    var composerArmed by remember { mutableStateOf(false) }
    // The draft is the authority (EXP-621). Reconciled in an EFFECT and only
    // when the text ACTUALLY differs: the draft round-trips through a
    // StateFlow, so doing it in composition reset the caret mid-typing and
    // dropped the IME's in-flight composing word. The selection is CLAMPED
    // into the new text rather than jumped to its end, and `composition` is
    // dropped on purpose — it indexes characters that are gone. An outside
    // rewrite (a send clearing the draft, a `/` pick, a restored draft) is not
    // typing, so it also DISARMS.
    LaunchedEffect(draft) {
        if (composerField.text != draft) {
            val start = composerField.selection.start.coerceIn(0, draft.length)
            val end = composerField.selection.end.coerceIn(0, draft.length)
            composerField = TextFieldValue(draft, TextRange(start, end))
            composerArmed = false
        }
    }
    val mentionMembers by viewModel.mentionMembers.collectAsStateWithLifecycle()
    // `#` needs no plumbing beyond the handler the feed already builds — the
    // composer sits OUTSIDE that provider (what the user types must stay the
    // stored `#IDENTIFIER` contract, never a bare identifier), so it reads the
    // same object directly.
    val composerTriggers = autocompleteTriggersAt(
        beforeCaret = composerField.text.take(composerField.selection.start),
        mentionsEnabled = mentionMembers.isNotEmpty(),
        refsEnabled = issueRefCandidates.isNotEmpty(),
    )
    val composerMentions = mentionCandidatesFor(mentionMembers, composerTriggers.mentionQuery)
    // EXP-892: ranked by the shared engine, with the server's full-text hits
    // (comment bodies included) spliced in behind them once typing settles.
    // SLOP-28 (EXP-1259 D4): `#` opens THE search list in a bottom sheet
    // (the IssueTypeaheadSheetHost below), never issue rows in this menu.
    val composerRefs = emptyList<IssueRefTarget>()
    val composerEmojiMatch = composerTriggers.emoji
    val composerEmojiData = rememberEmojiData(enabled = composerEmojiMatch != null)
    val composerEmojiPrefs = rememberEmojiPrefs()
    val composerEmoji = if (composerEmojiMatch != null && composerEmojiData != null) {
        composerEmojiData.search(composerEmojiMatch.query, limit = EMOJI_TYPEAHEAD_LIMIT)
    } else {
        emptyList()
    }
    // The trigger stopped matching (the caret left the token, whitespace was
    // typed, the trigger was deleted): a fresh text change has to re-arm.
    LaunchedEffect(composerTriggers.none) {
        if (composerTriggers.none) composerArmed = false
    }
    // EXP-892: the highlighted autocomplete row — the TOP one while typing,
    // moved by a hardware ↑/↓ and picked by Enter/Tab, like the `/` menu.
    val composerCandidateCount =
        autocompleteCandidateCount(composerMentions, composerRefs, composerEmoji)
    var autocompleteSelected by remember { mutableIntStateOf(0) }
    LaunchedEffect(composerMentions, composerRefs, composerEmoji) { autocompleteSelected = 0 }
    // Every pick splices against the LIVE value (EXP-655) and clears the latch.
    fun commitComposerToken(spliced: TextFieldValue?) {
        val next = spliced ?: return
        composerField = next
        viewModel.setDraft(next.text)
        composerArmed = false
    }
    IssueTypeaheadSheetHost(
        handler = composerRefHandler.takeIf { issueRefCandidates.isNotEmpty() },
        field = composerField,
        armed = composerArmed && composerTriggers.issueRef != null,
        onDisarm = { composerArmed = false },
        onCommit = { next -> commitComposerToken(next) },
    )
    // `:tada:` — the closing colon plus an EXACT shortcode commits immediately
    // (no trailing space), so a user who habitually types the closed form never
    // leaves literal shortcode text in a steered message. Web/iOS/desktop
    // parity, and armed-gated like the menu: re-reading an old `:tada:` with
    // the caret must not rewrite it.
    val composerClosedShortcode = composerEmojiMatch?.takeIf { it.closed }?.query
    LaunchedEffect(composerClosedShortcode, composerEmojiData, composerArmed) {
        val code = composerClosedShortcode ?: return@LaunchedEffect
        if (!composerArmed) return@LaunchedEffect
        val record = composerEmojiData?.findShortcode(code) ?: return@LaunchedEffect
        commitComposerToken(composerField.withEmoji(record, trailingSpace = false))
        composerEmojiPrefs.pushRecent(record.unicode)
    }

    // The ONE menu above the composer. `/` commands and the three
    // composer triggers are already mutually exclusive on CONTENT (a
    // slash command matches only a draft that opens with `/`, the
    // others only a token at the caret), so resolving them into one
    // value is DEFENCE, not arbitration: it makes it impossible for
    // two menus to stack, or for the key guards below to drive a menu
    // that is not on screen.
    val composerMenu = when {
        // EXP-1291: an ended session's composer still sends (the server
        // continues it), so only a card waiting on the viewer retires it.
        composerHidden -> ComposerMenu.None
        slashMenuOpen -> ComposerMenu.Slash
        composerArmed &&
            (composerMentions.isNotEmpty() || composerRefs.isNotEmpty() ||
                composerEmoji.isNotEmpty()) -> ComposerMenu.Autocomplete
        else -> ComposerMenu.None
    }

    Column(modifier = modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp),
        ) {
            banners()
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

            // EXP-861: what the device holds for the agent's next turn — one
            // line per queued message, each with the X that revokes it.
            // Never while the run is over: an ended run delivers nothing. The
            // composer stays live beneath it.
            // EXP-1291: messages ride `sessions.send` now, so the server's
            // not-yet-consumed rows are the queue (X = `sessions.unqueue`);
            // the device's own relay queue still shows while it reports one.
            val serverQueue by viewModel.queuedMessages.collectAsStateWithLifecycle()
            if (showQueue && serverQueue.isNotEmpty()) {
                ReadingColumn {
                    QueueStrip(messages = serverQueue, onRemove = viewModel::unqueueMessage)
                }
                Spacer(Modifier.height(8.dp))
            } else if (showQueue && phase == AgentPhase.Live && !sessionEnded && activity.queue.isNotEmpty()) {
                ReadingColumn {
                    QueueStrip(messages = activity.queue, onRemove = viewModel::unqueue)
                }
                Spacer(Modifier.height(8.dp))
            }

            beforeMenus()

            // EXP-724/EXP-802: both menus are a normal child above the
            // composer, not a Popup — the composer is already pinned to the
            // bottom above the IME, so a menu simply grows upward from it, and
            // a sibling cannot steal the field's focus by construction.
            when (composerMenu) {
                ComposerMenu.Slash -> {
                    SlashCommandMenu(
                        candidates = slashMatches,
                        selected = slashSelected,
                        onPick = acceptSlash,
                    )
                    Spacer(Modifier.height(8.dp))
                }
                ComposerMenu.Autocomplete -> {
                    AutocompleteRows(
                        mentionCandidates = composerMentions,
                        refCandidates = composerRefs,
                        emojiCandidates = composerEmoji,
                        selectedIndex = autocompleteSelected,
                        onPickMention = { commitComposerToken(composerField.withMention(it)) },
                        onPickIssueRef = { commitComposerToken(composerField.withIssueRef(it)) },
                        onPickEmoji = { record ->
                            commitComposerToken(
                                composerField.withEmoji(record, trailingSpace = true),
                            )
                            composerEmojiPrefs.pushRecent(record.unicode)
                        },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Spacer(Modifier.height(8.dp))
                }
                ComposerMenu.None -> Unit
            }
            beforeComposer()
            // EXP-1211: the quick replies sit directly above the card, and
            // hide whenever the composer does (the ask dock).
            val replies = composerReplies
            if (!composerHidden && replies != null) {
                ComposerRepliesRow(
                    replies = replies,
                    pending = replyPending,
                    onSend = { itemId -> viewModel.sendReply(replies.messageId, itemId) },
                    onDismiss = { viewModel.dismissReplies(replies.messageId) },
                )
                Spacer(Modifier.height(8.dp))
            }
            // Escape has no hardware key on most phones — Back dismisses
            // the menu, and only the menu (EXP-724). One handler per menu,
            // and [composerMenu] guarantees at most one is ever enabled.
            BackHandler(enabled = composerMenu == ComposerMenu.Slash) {
                slashDismissedFor = draft
            }
            BackHandler(enabled = composerMenu == ComposerMenu.Autocomplete) {
                composerArmed = false
            }
        }
        // ── The floating bottom bar / steering input ─────────────────────────
        // Fully seamless (EXP-312): no captions, no operator state; live
        // implies ownership, input just sends. EXP-621: the composer is
        // present for the WHOLE life of the session, not only while the
        // socket happens to be up — a mid-reconnect blip used to yank the
        // keyboard and the typed text away. Only a finished session retires
        // it; until the stream is live, sending is disabled rather than
        // hidden. EXP-820: a card waiting on this viewer takes its place (see
        // [composerHidden]). EXP-893: retired or displaced, the BAR stays —
        // with only the host's trailing circle — and goes without one.
        if (!composerHidden) {
            SteerComposer(
                value = composerField,
                // A user edit is the ONLY write that may arm the `@`/`#`/`:`
                // menu (web parity: the autocomplete opens on a document
                // change, never on a caret move).
                onValueChange = { next ->
                    if (next.text != composerField.text) composerArmed = true
                    composerField = next
                    viewModel.setDraft(next.text)
                },
                // The composer's own rewrites — the `[Image #k]` markers and
                // their renumbering — write the same state and never arm.
                onValueRewrite = { next ->
                    composerField = next
                    viewModel.setDraft(next.text)
                },
                fieldModifier = Modifier.onPreviewKeyEvent { event ->
                    if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    // EXP-892: the autocomplete owns the same four keys while
                    // IT is the menu that is up.
                    if (composerMenu == ComposerMenu.Autocomplete && composerCandidateCount > 0) {
                        return@onPreviewKeyEvent when (event.key) {
                            Key.DirectionUp -> {
                                autocompleteSelected =
                                    (autocompleteSelected - 1 + composerCandidateCount) %
                                        composerCandidateCount
                                true
                            }
                            Key.DirectionDown -> {
                                autocompleteSelected =
                                    (autocompleteSelected + 1) % composerCandidateCount
                                true
                            }
                            Key.Enter, Key.NumPadEnter, Key.Tab -> {
                                pickAutocompleteAt(
                                    index = autocompleteSelected
                                        .coerceIn(0, composerCandidateCount - 1),
                                    mentionCandidates = composerMentions,
                                    refCandidates = composerRefs,
                                    emojiCandidates = composerEmoji,
                                    onPickMention = {
                                        commitComposerToken(composerField.withMention(it))
                                    },
                                    onPickIssueRef = {
                                        commitComposerToken(composerField.withIssueRef(it))
                                    },
                                    onPickEmoji = { record ->
                                        commitComposerToken(
                                            composerField.withEmoji(record, trailingSpace = true),
                                        )
                                        composerEmojiPrefs.pushRecent(record.unicode)
                                    },
                                )
                                true
                            }
                            Key.Escape -> {
                                composerArmed = false
                                true
                            }
                            else -> false
                        }
                    }
                    // Keyed on the RESOLVED menu, not on `slashMenuOpen`:
                    // ↑/↓/Enter must only ever drive the menu that is up.
                    if (composerMenu != ComposerMenu.Slash) return@onPreviewKeyEvent false
                    when (event.key) {
                        // ↑/↓ wrap around the candidate list.
                        Key.DirectionUp -> {
                            slashSelected =
                                (slashSelected - 1 + slashMatches.size) % slashMatches.size
                            true
                        }
                        Key.DirectionDown -> {
                            slashSelected = (slashSelected + 1) % slashMatches.size
                            true
                        }
                        // Enter ACCEPTS while the menu is open; it never
                        // sends the half-typed command underneath it.
                        Key.Enter, Key.NumPadEnter -> {
                            slashMatches.getOrNull(slashSelected)?.let(acceptSlash)
                            true
                        }
                        Key.Escape -> {
                            slashDismissedFor = draft
                            true
                        }
                        else -> false
                    }
                },
                pendingImages = pendingImages,
                sending = steerSending,
                // EXP-1291: a message is `sessions.send({sessionId})` — the
                // server holds it until the device takes it (an idle session
                // resumes, an ended one continues), so the relay socket no
                // longer gates sending.
                live = true,
                // EXP-790: mid-turn, the empty field's send glyph is a
                // Stop that interrupts the agent.
                working = agentWorking,
                onInterrupt = viewModel::interrupt,
                expanded = expanded,
                onExpandedChange = onExpandedChange,
                onPickImages = { filePicker.launch(arrayOf("*/*")) },
                menu = plusMenu,
                onRemoveImage = viewModel::removePendingImage,
                // EXP-724: a command that discards the conversation is
                // confirmed first; everything else sends straight out. A
                // command with attachments never reaches the confirm: the
                // send refuses it (`Remove the attachments to send a command`).
                onSend = {
                    val command = viewModel.pendingSlashCommand()
                    if (command?.confirm == true && pendingImages.isEmpty()) slashConfirm = command else viewModel.sendDraft()
                },
                // EXP-1334: the Plan switch under the card (a dispatcher's =
                // the work pick its children inherit). Before the relay's first
                // `config_state` the synced row's `planMode` stands in.
                planMode = plusMenu.planMode,
                model = composerModel(
                    config = sessionConfig,
                    rowModel = session?.model,
                    cli = session?.cli,
                    dispatch = session?.dispatch == true,
                    launchDefaults = hostLaunchDefaults,
                ),
                modelPickable = catalogAgent == DEFAULT_AGENT,
                onPickModel = { alias -> viewModel.sendCommand("/model $alias") },
                deviceMove = deviceMove,
                onMoveDevice = viewModel::moveToDevice,
                contextPercent = contextPercent,
                hasUsage = hasUsage,
                onOpenUsage = { onOpenUsage?.invoke() },
                placeholder = placeholder,
                capsuleTestTag = capsuleTestTag,
                trailing = trailing,
                merge = merge,
            )
        } else {
            if (dock != null) {
                Box(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp)) { dock() }
            }
            if (trailing != null || merge != null) {
                FloatingBottomBar(right = trailing, afterCentre = merge) {
                    Spacer(Modifier.weight(1f))
                }
            }
        }
    }

    // EXP-724: `/clear` throws the conversation away — the same
    // confirm shape as the kill dialog, with the copy pinned ×4.
    val confirmCommand = slashConfirm
    if (confirmCommand != null) {
        GlassAlert(
            title = SlashCommands.confirmTitle(confirmCommand.name),
            body = SlashCommands.CONFIRM_BODY,
            onDismiss = { slashConfirm = null },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { slashConfirm = null }),
                GlassAlertAction(
                    SlashCommands.confirmButton(confirmCommand.name),
                    destructive = true,
                    onClick = {
                        slashConfirm = null
                        viewModel.sendDraft()
                    },
                ),
            ),
            defaultAction = 0,
        )
    }
}

/**
 * Which menu is up over the steer composer (EXP-802/EXP-805). The `/` commands
 * and the `@`/`#`/`:` autocomplete cannot both match the same draft, so this is
 * DEFENCE rather than arbitration: one value means two menus can never stack,
 * and the composer's key guards can only ever drive the one on screen.
 */
private enum class ComposerMenu { None, Slash, Autocomplete }

/**
 * The steering composer (EXP-511) restyled to the comment composer's chrome
 * (EXP-554): ONE rounded card — the near-opaque bottom-bar pill fill under a
 * hairline stroke — holding the pending-image strip, a transparent text field,
 * and the footer row.
 *
 * EXP-790: it folds while it is unfocused and empty (the issue's comment bar
 * rule) — since EXP-893 into the Work screen's floating bar, whose capsule
 * reopens it — the send glyph is the shared `ui-submit` concept, and while
 * the agent works with nothing typed that glyph is a Stop that interrupts the
 * turn. EXP-893: the footer is the desktop composer's — `Plan mode` (read-only,
 * blue while on), attach, the model (a `/model <alias>` picker on claude), and
 * the context ring into the Usage sheet.
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
    /**
     * EXP-802/EXP-805: the value is a TextFieldValue owned by the FACE, not
     * a string owned here — the `@`/`#`/`:` menu is a sibling above this
     * composer and splices at the caret, so the caret has to be visible up
     * there. The draft text itself still belongs to the connection.
     */
    value: TextFieldValue,
    /** A user edit — the only write that may arm the autocomplete. */
    onValueChange: (TextFieldValue) -> Unit,
    /** The composer's own rewrites (image markers, renumbering): the same
     *  state, but never arming — nobody typed a trigger. */
    onValueRewrite: (TextFieldValue) -> Unit,
    /** EXP-724: the face's key handling for the open `/` menu (↑/↓/Enter/
     *  Escape) — the composer itself knows nothing about commands. */
    fieldModifier: Modifier = Modifier,
    pendingImages: List<PendingAttachment>,
    sending: Boolean,
    /** The relay stream is up — only then can a message actually go out. */
    live: Boolean,
    /** EXP-790: the agent is mid-turn — with nothing typed, the send glyph
     *  is a Stop that interrupts it. */
    working: Boolean,
    onInterrupt: () -> Unit,
    /** EXP-790: expanded (the card with its field) or folded to the bar. */
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    onPickImages: () -> Unit,
    /** EXP-1334: the "+" menu's rows and the live picks. */
    menu: SessionComposerMenu,
    onRemoveImage: (Int) -> Unit,
    onSend: () -> Unit,
    /** EXP-893: the footer — `Plan mode` while on, the run's model (a picker
     *  on claude, a label elsewhere; hidden when the engine named none), and
     *  the context ring that opens the Usage sheet. */
    planMode: Boolean,
    model: String?,
    modelPickable: Boolean,
    onPickModel: (String) -> Unit,
    /** EXP-1252: the Device pin before Model (hidden while not [visible]). */
    deviceMove: SessionDeviceMoveState,
    onMoveDevice: (String) -> Unit,
    contextPercent: Int?,
    hasUsage: Boolean,
    onOpenUsage: () -> Unit,
    /** The collapsed bar's right circle — the host's Start (EXP-1150);
     *  null = none. The expanded composer carries no switcher:
     *  the face tabs sit above it, under the top bar. */
    trailing: (@Composable () -> Unit)?,
    /** EXP-1191: the collapsed bar's Merge circle, right of the capsule. */
    merge: (@Composable () -> Unit)? = null,
    /** The host's copy (the thread's `Reply…`, an agent channel's). */
    placeholder: String = STEER_COMPOSER_PLACEHOLDER,
    capsuleTestTag: String = "steer-composer-capsule",
) {
    // EXP-893: ONE placeholder ×4 (`STEER_COMPOSER_PLACEHOLDER`); typing is
    // always allowed while the stream is down, the message just waits for it
    // to come back (EXP-621), and the placeholder says so.
    val shownPlaceholder = if (live) placeholder else "$placeholder (reconnecting…)"
    // EXP-1316: the fold / expand / collapse-on-blur machinery is THE
    // [FloatingComposer] the thread wears too; this keeps the steer card.
    // EXP-893: the folded composer IS the Work screen's bar — the placeholder
    // capsule that opens the card, the host's circle on the right.
    FloatingComposer(
        expanded = expanded,
        onExpandedChange = onExpandedChange,
        placeholder = shownPlaceholder,
        hasDraft = value.text.isNotBlank() || pendingImages.isNotEmpty(),
        working = working,
        capsuleTestTag = capsuleTestTag,
        // The folded bar is the capsule alone: the ring lives in the card.
        left = null,
        right = trailing,
        afterCentre = merge,
    ) {
        ExpandedSteerComposer(
            value = value,
            onValueChange = onValueChange,
            onValueRewrite = onValueRewrite,
            fieldModifier = fieldModifier.then(this.fieldModifier),
            placeholder = shownPlaceholder,
            pendingImages = pendingImages,
            sending = sending,
            live = live,
            working = working,
            onInterrupt = onInterrupt,
            onPickImages = onPickImages,
            menu = menu,
            onRemoveImage = onRemoveImage,
            onSend = onSend,
            planMode = planMode,
            model = model,
            modelPickable = modelPickable,
            onPickModel = onPickModel,
            deviceMove = deviceMove,
            onMoveDevice = onMoveDevice,
            contextPercent = contextPercent,
            hasUsage = hasUsage,
            onOpenUsage = onOpenUsage,
            onMenuOpenChange = onMenuOpenChange,
        )
    }
}

/** EXP-931: the expanded composer's own menus, as the keys of its open set. */
private const val COMPOSER_MENU_MODEL = "model"
private const val COMPOSER_MENU_DEVICE = "device"
private const val COMPOSER_MENU_PLUS = "plus"
private const val COMPOSER_MENU_EFFORT = "effort"
private const val COMPOSER_MENU_MCP = "mcp"

/** The options line's Plan switch label (composer-menu.json options line). */
private const val PLAN_LABEL = "Plan"

/**
 * EXP-1334: what THE session composer's "+" offers in a running session —
 * composer-menu.json's rows, the live picks (the synced row, overridden by
 * the person's changes until they land) and the `sessions.configure` call.
 * Immutable and value-equal, so the composer skips the feed's ticks.
 */
@Immutable
internal data class SessionComposerMenu(
    val agent: String,
    val effort: String,
    val ultracode: Boolean,
    val computerUse: Boolean,
    val canComputerUse: Boolean,
    val mcpServers: List<McpServerOption>,
    val mcpServerIds: List<String>,
    val issues: List<IssueOption>,
    val planMode: Boolean,
    /** An ended session: the option rows and the Plan switch are read-only. */
    val readOnly: Boolean,
    val onConfigure: (AgentSessionViewModel.ConfigurePicks) -> Unit,
    val onImplement: (List<String>) -> Unit,
)

@Composable
private fun ExpandedSteerComposer(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    onValueRewrite: (TextFieldValue) -> Unit,
    fieldModifier: Modifier,
    placeholder: String,
    pendingImages: List<PendingAttachment>,
    sending: Boolean,
    live: Boolean,
    working: Boolean,
    onInterrupt: () -> Unit,
    onPickImages: () -> Unit,
    menu: SessionComposerMenu,
    onRemoveImage: (Int) -> Unit,
    onSend: () -> Unit,
    planMode: Boolean,
    model: String?,
    modelPickable: Boolean,
    onPickModel: (String) -> Unit,
    deviceMove: SessionDeviceMoveState,
    onMoveDevice: (String) -> Unit,
    contextPercent: Int?,
    hasUsage: Boolean,
    onOpenUsage: () -> Unit,
    /** EXP-931: this composer's own menus, so the host never collapses under
     *  one — keyed per menu, so two menus never clear each other's guard. */
    onMenuOpenChange: (id: String, open: Boolean) -> Unit = { _, _ -> },
) {
    val canSend = (value.text.isNotBlank() || pendingImages.isNotEmpty()) && !sending && live
    // EXP-790: nothing to send and the agent mid-turn — the glyph is a Stop.
    val stop = working && live && value.text.isBlank() && pendingImages.isEmpty()
    // EXP-698: the value carries its SELECTION, because picking an image drops
    // an `[Image #k]` marker at the caret. Since EXP-802/EXP-805 that value —
    // and its reconciliation with the connection's draft string — lives on the
    // FACE, where the autocomplete menu can splice at the caret; this
    // composer only reads it and reports its edits back.
    // Each newly picked image inserts its own marker, so the writer can say
    // "crop [Image #2]" without typing the token. Removing one renumbers the
    // draft (below), so the markers always name images the composer still has.
    // Wave D: only IMAGES are numbered — a file tile carries no marker.
    val imageCount = pendingImages.count { it.isImage }
    var markedImages by remember { mutableIntStateOf(imageCount) }
    LaunchedEffect(imageCount) {
        if (imageCount > markedImages) {
            var next = value
            for (k in (markedImages + 1)..imageCount) {
                val (text, caret) = insertImageMarker(next.text, next.selection.end, k)
                next = TextFieldValue(text, TextRange(caret))
            }
            onValueRewrite(next)
        }
        markedImages = imageCount
    }
    // Opening the card is what a tap on the folded capsule means: the field
    // takes focus (and the keyboard) at once.
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) { focusRequester.requestFocus() }
    // EXP-893: the model menu = THE model list (all four claude models, no
    // Default: every model is explicit); a pick sends `/model <alias>` as a
    // plain message and the re-emitted `config_state` is the confirmation,
    // so nothing here waits or locks.
    var modelMenuOpen by remember { mutableStateOf(false) }
    // The model in force leads the list when this build has no row for it (a
    // legacy alias still renders checked), as on web.
    val modelChoices = remember(model) {
        val options = modelOptionsFor(DEFAULT_AGENT)
        if (model != null && model.isNotEmpty() && model !in options) listOf(model) + options else options
    }
    var plusOpen by remember { mutableStateOf(false) }
    var effortOpen by remember { mutableStateOf(false) }
    var mcpOpen by remember { mutableStateOf(false) }
    Column(modifier = Modifier.fillMaxWidth()) {
    GlassComposer(
        // The composer floats over the scrolling activity feed.
        opaque = true,
        modifier = Modifier.padding(start = 12.dp, end = 12.dp, top = 8.dp),
        strip = {
            PendingAttachmentStrip(
                items = pendingImages,
                enabled = !sending,
                onRemove = { index ->
                    // Renumber BEFORE the list shrinks: `[Image #k]` goes, and
                    // every higher marker comes down one. A file has no marker.
                    val number = pendingImages.imageNumberAt(index)
                    val renumbered = if (number != null) renumberImageMarkers(value.text, number) else value.text
                    if (renumbered != value.text) {
                        onValueRewrite(
                            TextFieldValue(
                                renumbered,
                                TextRange(value.selection.end.coerceAtMost(renumbered.length)),
                            ),
                        )
                    }
                    onRemoveImage(index)
                },
            )
        },
        // EXP-1334: the card's footer = the "+" (composer-menu.json's rows)
        // · spacer · send; the model, the Plan switch and the ring sit UNDER
        // the card (the options line).
        footer = {
            ComposerToolButton(
                ExpIcons.uiAdd,
                contentDescription = com.exponential.app.domain.ComposerMenu.PLUS_LABEL,
                onClick = {
                    plusOpen = true
                    onMenuOpenChange(COMPOSER_MENU_PLUS, true)
                },
                enabled = !sending,
                modifier = Modifier.testTag(com.exponential.app.domain.ComposerMenu.PLUS_TEST_ID),
            )
            Spacer(Modifier.weight(1f))
            ComposerSubmitButton(
                if (stop) ExpIcons.uiStop else ExpIcons.uiSubmit,
                contentDescription = if (stop) "Stop" else "Send",
                // The draft is cleared by the send itself, and only once the
                // message is out (EXP-621) — a failed image upload leaves the
                // whole composition intact to retry.
                onClick = {
                    when {
                        stop -> onInterrupt()
                        canSend -> onSend()
                    }
                },
                enabled = stop || canSend,
                sending = sending,
            )
        },
    ) {
        GlassTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier
                .fillMaxWidth()
                .focusRequester(focusRequester)
                .then(fieldModifier),
            placeholder = placeholder,
            maxLines = 4,
            // The composer card owns the chrome; the field is just its text.
            bordered = false,
        )
    }
    // The options line under the card: model ▾ · Plan switch · ring.
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, bottom = 4.dp)
            .testTag("steer-options-line"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // EXP-1252: the Device pin BEFORE Model — the launch line's device
        // picker, its trigger the same inline pin as Model. A disabled pin
        // floats its reason on tap (the account rows' rule).
        if (deviceMove.visible) {
            val toaster = LocalToaster.current
            val blocked = deviceMove.blockedReason
            val current = deviceMove.rows.firstOrNull { it.current }
            DevicePicker(
                devices = deviceMove.rows.map { row ->
                    DevicePickerDevice(
                        id = row.id,
                        name = row.name,
                        icon = row.icon,
                        isServer = row.isServer,
                        description = row.reason,
                        disabled = row.reason != null,
                    )
                },
                value = deviceMove.deviceId,
                onChange = onMoveDevice,
                title = SessionDeviceMove.PICKER_TITLE,
                onOpenChange = { open -> onMenuOpenChange(COMPOSER_MENU_DEVICE, open) },
                trigger = { open ->
                    ComposerPin(
                        label = deviceMove.label,
                        icon = deviceIcon(current?.icon, isServer = current?.isServer == true),
                        enabled = blocked == null,
                        contentDescription = "${SessionDeviceMove.PICKER_TITLE} ${deviceMove.label}",
                        // The move's note (or the refusal) rides the pin as its
                        // a11y hint, like the account switch: no picker footer.
                        hint = blocked ?: SessionDeviceMove.NOTE,
                        testTag = "steer-device",
                        onClick = open,
                        onDisabledClick = blocked?.let { reason -> { toaster.error(reason) } },
                    )
                },
            )
        }
        if (model != null) {
            // EXP-1345: THE picker (a bottom sheet, the picked row's TRAILING
            // check). codex: the model is what it is — the same read-out,
            // disabled (desktop keeps the chevron, dimmed).
            com.exponential.app.ui.components.picker.ModelPicker(
                options = modelChoices.map { com.exponential.app.ui.components.picker.ModelPickerOption(it, modelLabel(it)) },
                value = model,
                onChange = { alias -> if (live && alias != model) onPickModel(alias) },
                enabled = modelPickable,
                open = modelPickable && modelMenuOpen,
                onOpenChange = { next ->
                    modelMenuOpen = next
                    // EXP-931: an open menu is not a blur.
                    onMenuOpenChange(COMPOSER_MENU_MODEL, next)
                },
                trigger = { open ->
                    ComposerModelPin(
                        label = modelLabel(model),
                        enabled = modelPickable,
                        onClick = open,
                    )
                },
            )
        }
        if (supportsPlanMode(menu.agent)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier
                    .clip(RoundedCornerShape(6.dp))
                    // An ended session's switch is a read-out (web `disabled`).
                    .clickable(enabled = !menu.readOnly, role = Role.Switch) {
                        menu.onConfigure(AgentSessionViewModel.ConfigurePicks(planMode = !planMode))
                    }
                    .padding(horizontal = 4.dp, vertical = 6.dp)
                    .testTag("steer-plan-mode"),
            ) {
                Text(
                    PLAN_LABEL,
                    style = MaterialTheme.typography.labelLarge,
                    color = Color.White.copy(alpha = if (menu.readOnly) TextEmphasis.Quaternary else TextEmphasis.Secondary),
                    maxLines = 1,
                )
                GlassSwitch(checked = planMode, onCheckedChange = null, size = GlassSwitchSize.Pill)
            }
        }
        Spacer(Modifier.weight(1f))
        if (hasUsage) {
            IconButton(onClick = onOpenUsage) {
                ContextRing(percent = contextPercent, size = 20.dp)
            }
        }
    }
    }

    if (plusOpen) {
        val claude = menu.agent == DEFAULT_AGENT
        val closePlus = {
            plusOpen = false
            onMenuOpenChange(COMPOSER_MENU_PLUS, false)
        }
        var implementPicks by remember { mutableStateOf<Set<String>>(emptySet()) }
        ComposerPlusMenuSheet(
            layout = com.exponential.app.domain.ComposerMenu.composerMenuLayout(
                ComposerMenuConditions(
                    // The subagent model is a launch pick: not live-changeable.
                    subagentModel = false,
                    ultracode = claude,
                    mcp = menu.mcpServers.isNotEmpty(),
                    computerUse = menu.canComputerUse,
                    agents = false,
                ),
            ),
            codex = menu.agent == "codex",
            effortValue = menu.effort.takeIf { it.isNotEmpty() }?.let(::effortLabel),
            effortEnabled = !(claude && menu.ultracode),
            // An ended session: the option rows read, never change (web
            // `rowsDisabled`); Implement issue, Run action and Add file stay.
            rowsReadOnly = menu.readOnly,
            subagentsValue = null,
            ultracode = menu.ultracode,
            mcpValue = mcpPickValue(menu.mcpServerIds.size),
            computerUse = menu.computerUse,
            canAttach = pendingImages.count { it.isImage } < MAX_STEER_IMAGES ||
                pendingImages.count { !it.isImage } < MAX_STEER_FILES,
            onRow = { row ->
                closePlus()
                when (row) {
                    ComposerMenuRowId.AddFile -> onPickImages()
                    ComposerMenuRowId.Effort -> {
                        effortOpen = true
                        onMenuOpenChange(COMPOSER_MENU_EFFORT, true)
                    }
                    ComposerMenuRowId.McpServers -> {
                        mcpOpen = true
                        onMenuOpenChange(COMPOSER_MENU_MCP, true)
                    }
                    else -> Unit
                }
            },
            onUltracodeChange = { menu.onConfigure(AgentSessionViewModel.ConfigurePicks(ultracode = it)) },
            onComputerUseChange = { menu.onConfigure(AgentSessionViewModel.ConfigurePicks(computerUse = it)) },
            onDismiss = closePlus,
            // Implement issue › in a running session = link the picks to
            // the thread (a manual link wakes the dispatcher).
            issuePage = { close ->
                AgentIssuePickerPage(
                    issues = menu.issues,
                    checkedIds = implementPicks,
                    onToggle = { id -> implementPicks = if (id in implementPicks) implementPicks - id else implementPicks + id },
                    onDone = {
                        menu.onImplement(implementPicks.toList())
                        implementPicks = emptySet()
                        close()
                    },
                )
            },
        )
    }
    if (effortOpen) {
        Picker(
            // `CLI default` first, as the launch pickers: the VM sends `""`,
            // which the server trims to a null effort (web parity).
            items = (listOf(CLI_DEFAULT_EFFORT) + effortValuesFor(menu.agent)).map { PickerItem(it, effortLabel(it)) },
            mode = PickerMode.Single,
            value = setOf(menu.effort),
            onChange = { picked ->
                picked.firstOrNull()?.let { menu.onConfigure(AgentSessionViewModel.ConfigurePicks(effort = it)) }
            },
            title = if (menu.agent == "codex") "Reasoning" else "Effort",
            open = true,
            onOpenChange = { open ->
                if (!open) {
                    effortOpen = false
                    onMenuOpenChange(COMPOSER_MENU_EFFORT, false)
                }
            },
        )
    }
    if (mcpOpen) {
        McpServerPicker(
            servers = menu.mcpServers,
            value = menu.mcpServerIds,
            onToggle = { id ->
                val next = if (id in menu.mcpServerIds) menu.mcpServerIds - id else menu.mcpServerIds + id
                menu.onConfigure(AgentSessionViewModel.ConfigurePicks(mcpServerIds = next))
            },
            open = true,
            onOpenChange = { open ->
                if (!open) {
                    mcpOpen = false
                    onMenuOpenChange(COMPOSER_MENU_MCP, false)
                }
            },
        )
    }
}

/**
 * The composer row's model read-out ×4: plain muted text + a down chevron, no
 * pill background (web/iOS/desktop). Disabled (codex) dims both and takes no
 * tap.
 */
@Composable
private fun ComposerModelPin(label: String, enabled: Boolean, onClick: () -> Unit) =
    ComposerPin(
        label = label,
        enabled = enabled,
        contentDescription = "Model $label",
        testTag = "steer-model",
        onClick = onClick,
    )

/**
 * THE options line's inline pin: plain muted text (+ an optional leading
 * glyph, EXP-1252's device) and a down chevron, no pill background. A
 * disabled pin dims; [onDisabledClick] (when given) still takes the tap so a
 * refusal can say why.
 */
@Composable
private fun ComposerPin(
    label: String,
    enabled: Boolean,
    contentDescription: String,
    testTag: String,
    onClick: () -> Unit,
    icon: androidx.compose.ui.graphics.vector.ImageVector? = null,
    onDisabledClick: (() -> Unit)? = null,
    /** An accessibility-only hint (EXP-1252: the device move's note). */
    hint: String? = null,
) {
    val textAlpha = if (enabled) TextEmphasis.Secondary else TextEmphasis.Tertiary
    val chevronAlpha = if (enabled) TextEmphasis.Tertiary else TextEmphasis.Quaternary
    val tap = if (enabled) onClick else onDisabledClick
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier
            .heightIn(min = 40.dp)
            .clip(RoundedCornerShape(6.dp))
            .clickable(enabled = tap != null, role = Role.Button) { tap?.invoke() }
            .padding(horizontal = 4.dp)
            .semantics {
                this.contentDescription = contentDescription
                if (hint != null) stateDescription = hint
            }
            .testTag(testTag),
    ) {
        if (icon != null) {
            Icon(
                icon,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = Color.White.copy(alpha = textAlpha),
            )
        }
        Text(
            label,
            style = MaterialTheme.typography.labelLarge,
            color = Color.White.copy(alpha = textAlpha),
            maxLines = 1,
        )
        Icon(
            ExpIcons.uiChevronDown,
            contentDescription = null,
            modifier = Modifier.size(10.dp),
            tint = Color.White.copy(alpha = chevronAlpha),
        )
    }
}

/**
 * EXP-849 phase 3 / EXP-909: ONE OTHER login of this run's agent on its
 * machine — the identity line, its health, its own numbers in the mini form,
 * and an ICON-ONLY switch. A refused switch keeps the button — the reason is
 * nearly always something the person can change (wait for the turn, sign in on
 * the machine), and a control that vanishes mid-run reads as a bug.
 *
 * EXP-1051: the reason is no longer a caption UNDER the row. Three of those
 * stacked under three rows pushed the sheet's real content off a phone, and
 * the reason is only wanted by whoever just tried: tapping the disabled
 * control hands it to [onRefused], which floats it once.
 *
 * No "Active login" / "Default" caption: the header above names the login the
 * run is ON, so every row here is by definition another one.
 */
@Composable
private fun SessionAccountRow(
    option: SessionAccountOption,
    refusal: String?,
    switching: Boolean,
    nowMs: Long,
    onSwitch: () -> Unit,
    onRefused: (String) -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            // Per-profile, matching iOS's a11y ids: two account rows in one
            // overlay have to be tellable apart from a test.
            .testTag("session-account-${option.profileId}"),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Text(
                option.caption,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            AgentHealthRules.badgeLabel(option.health)?.let { badge ->
                Spacer(Modifier.width(6.dp))
                Text(
                    badge,
                    style = MaterialTheme.typography.labelSmall,
                    color = NeedsInputAmber,
                    maxLines = 1,
                )
            }
            Spacer(Modifier.width(8.dp))
            if (switching) {
                CircularProgressIndicator(
                    modifier = Modifier.size(16.dp),
                    strokeWidth = 1.5.dp,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            } else {
                // A refused switch: the button stays, disabled, and the AREA
                // around it takes the tap so the reason has somewhere to come
                // from (a disabled control swallows nothing).
                Box(
                    modifier = if (refusal != null) {
                        Modifier.clickable { onRefused(refusal) }
                    } else {
                        Modifier
                    },
                ) {
                    CircleIconButton(
                        ExpIcons.uiSwap,
                        contentDescription = SessionAccountSwitch.SWITCH_LABEL,
                        onClick = onSwitch,
                        enabled = refusal == null,
                        borderless = true,
                        modifier = Modifier.testTag("switch-account-${option.profileId}"),
                    )
                }
            }
        }
        AgentUsageMini(usage = option.usage)
        AgentUsagePresentation.usageAge(option.usage, nowMs)?.let { age ->
            Text(
                age,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
            )
        }
    }
}

/**
 * EXP-784/818: the rate-limit banner's two strings, web `rateLimitBanner`: the
 * agent's own message (trimmed, else `Rate limit reached`) and, when it named
 * a reset, the RELATIVE countdown (`resets in 2h 57m`, the usage cards'
 * wording) shown as a separate muted caption, never joined. Mirrored ×4.
 */
internal data class RateLimitCaption(val message: String, val countdown: String?) {
    /** The banner's accessibility label: `"<message> <countdown>"`. */
    val accessibilityLabel: String
        get() = if (countdown == null) message else "$message $countdown"

    /**
     * FEED-61: the muted caption: the countdown when a reset is known, then
     * the wall's scope, `resets in 2h 10m · per account`, or the bare note
     * (web `rateLimitDetail`).
     */
    val detail: String
        get() = if (countdown == null) RATE_LIMIT_ACCOUNT_NOTE else "$countdown · $RATE_LIMIT_ACCOUNT_NOTE"
}

/**
 * FEED-61: what a wall is scoped to. A usage window belongs to the ACCOUNT,
 * so a model switch never lifts it; another account does (the pill beside
 * it). Byte-identical ×4 (web `RATE_LIMIT_ACCOUNT_NOTE`).
 */
internal const val RATE_LIMIT_ACCOUNT_NOTE = "per account"

internal fun rateLimitCaption(message: String?, resetsAtMs: Long?, nowMs: Long): RateLimitCaption {
    val head = message?.trim()?.takeIf { it.isNotEmpty() } ?: "Rate limit reached"
    val countdown = resetsAtMs?.takeIf { it > 0 }?.let {
        AgentUsagePresentation.resetCountdownAt(it, nowMs)
    }
    return RateLimitCaption(head, countdown)
}

/**
 * EXP-773: what the journal fetch is doing, named after the machine that holds
 * it. Byte-identical ×4 (web `session-history.ts`, iOS `historyStatus`,
 * desktop `session.rs`).
 */
private fun historyStatus(state: HistoryState?, device: String): String = when (state) {
    HistoryState.Pending -> "Fetching the transcript from $device…"
    HistoryState.DeviceOffline ->
        "$device is offline. The transcript lives on that machine."
    HistoryState.Unavailable -> "No transcript on $device."
    null -> ""
}


/**
 * EXP-772: the run's MODE above the composer, and nothing else. Model and
 * effort pickers are gone from a live session — they are launch decisions, and
 * a mid-run swap only ever muddied the transcript.
 *
 * Fire-and-forget: the tap sends one `set_mode` frame and nothing here goes
 * into a pending state, because the publisher's re-emitted `config_state` IS
 * the confirmation. An agent that refuses re-emits the old mode and the chip
 * snaps back.
 *
 * `plan` plus exactly one other mode draws as a compact Plan SWITCH: a
 * two-entry dropdown for "Plan or Build" is a menu that can only ever say the
 * thing the pill already shows.
 */
/**
 * EXP-724: the curated slash-command menu above the composer — the same rows
 * the desktop can execute, filtered by this run's agent. It sits in the layout
 * (no Popup): the composer is already pinned above the IME, so the menu grows
 * upward from it and the keyboard never has to move.
 */
@Composable
private fun SlashCommandMenu(
    candidates: List<com.exponential.app.domain.SlashCandidate>,
    selected: Int,
    onPick: (com.exponential.app.domain.SlashCandidate) -> Unit,
) {
    GlassMenuSurface(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .heightIn(max = 240.dp)
                .verticalScroll(rememberScrollState())
                // Scrolls with the content, matching M3's menu padding.
                .padding(vertical = 4.dp),
        ) {
            candidates.forEachIndexed { index, candidate ->
                val highlighted = index == selected
                if (candidate is com.exponential.app.domain.SlashCandidate.Prompt) {
                    // NEW-2: a team prompt — its icon and name (web
                    // `PromptCandidateRow`).
                    SlashPromptRow(
                        prompt = candidate.prompt,
                        highlighted = highlighted,
                        onClick = { onPick(candidate) },
                    )
                    return@forEachIndexed
                }
                val command = (candidate as com.exponential.app.domain.SlashCandidate.Command).command
                // A command that discards the conversation reads as
                // destructive here, exactly as its confirm dialog does.
                val nameColor = if (command.confirm) {
                    MaterialTheme.colorScheme.error
                } else {
                    MaterialTheme.colorScheme.onSurface
                }
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 44.dp)
                        .background(
                            if (highlighted) {
                                Color.White.copy(alpha = 0.06f)
                            } else {
                                Color.Transparent
                            },
                        )
                        .clickable { onPick(candidate) }
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        "/${command.name}",
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        color = nameColor,
                        maxLines = 1,
                    )
                    if (command.argHint.isNotEmpty()) {
                        Spacer(Modifier.width(6.dp))
                        Text(
                            command.argHint,
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Tertiary,
                            ),
                            maxLines = 1,
                        )
                    }
                    Spacer(Modifier.width(10.dp))
                    Text(
                        command.description,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Secondary,
                        ),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        textAlign = TextAlign.End,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

/** NEW-2: one team prompt in a `/` menu: its curated glyph and its name. */
@Composable
internal fun SlashPromptRow(
    prompt: com.exponential.app.domain.SlashPrompt,
    highlighted: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 44.dp)
            .background(if (highlighted) Color.White.copy(alpha = 0.06f) else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp)
            .testTag("slash-prompt-${prompt.id}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            prompt.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.actionDefault,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(10.dp))
        Text(
            prompt.name,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// ── Misc rows ────────────────────────────────────────────────────────────────

/** EXP-724: what a finished compaction leaves behind — a quiet centered
 *  divider, so "why did it forget what we discussed" has an answer later. */
@Composable
private fun CompactionMarkerRow() {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
    ) {
        Icon(
            ExpIcons.codingCompact,
            contentDescription = null,
            modifier = Modifier.size(12.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        Text(
            COMPACTED_LABEL,
            style = transcriptToolStyle(),
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/** EXP-724: a steered message that IS a catalog command — the pill says a
 *  command ran instead of pretending the human typed prose. */
@Composable
private fun CommandRow(command: SlashCommand, text: String) {
    val args = text.trim().drop(1 + command.name.length).trim()
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.End,
    ) {
        Row(
            modifier = Modifier
                .glassButton(active = false)
                .padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Icon(
                ExpIcons.codingCommand,
                contentDescription = null,
                modifier = Modifier.size(12.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Text(
                "/${command.name}",
                style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (args.isNotEmpty()) {
                Text(
                    args,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(
                        alpha = TextEmphasis.Secondary,
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun BannerRow(content: @Composable RowScope.() -> Unit) {
    // Centred in the transcript's reading column, like every row above it —
    // on a tablet the banner no longer runs edge to edge.
    ReadingColumn {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            content = content,
        )
    }
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
