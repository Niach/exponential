package com.exponential.app.ui.agent

import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.threads.ThreadsViewModel
import com.exponential.app.ui.threads.ThreadListRow
import com.exponential.app.ui.threads.ThreadRowMeta
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.clickable
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.layout
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.ActionInputValues
import com.exponential.app.domain.AgentComposerPrompt
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.AgentHome
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.ComposerMenuConditions
import com.exponential.app.domain.ComposerMenuRowId
import androidx.compose.foundation.lazy.LazyListScope
import com.exponential.app.domain.mcpPickValue
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.SUBAGENT_MODEL_LABEL
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.subagentModelLabel
import com.exponential.app.ui.components.subagentModelOptions
import com.exponential.app.ui.components.supportsSubagentModel
import com.exponential.app.ui.components.picker.McpServerPicker
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.domain.MAX_STEER_FILES
import com.exponential.app.domain.MAX_STEER_IMAGES
import com.exponential.app.domain.resumeWorktreeFor
import com.exponential.app.ui.components.DeviceNotReadyRow
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.emoji.rememberEmojiData
import com.exponential.app.ui.emoji.rememberEmojiPrefs
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.IssueGraphList
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.markdown.AutocompleteRows
import com.exponential.app.ui.markdown.EMOJI_TYPEAHEAD_LIMIT
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.autocompleteCandidateCount
import com.exponential.app.ui.markdown.autocompleteTriggersAt
import com.exponential.app.ui.markdown.mentionCandidatesFor
import com.exponential.app.ui.markdown.pickAutocompleteAt
import com.exponential.app.ui.markdown.withEmoji
import com.exponential.app.ui.markdown.withIssueRef
import com.exponential.app.ui.markdown.withMention
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.domain.ToastAction
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.gettingstarted.GettingStartedSection
import com.exponential.app.ui.markdown.IssueRefTarget
import com.exponential.app.ui.markdown.IssueTypeaheadSheetHost
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import java.time.LocalDateTime
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import kotlinx.coroutines.launch

/**
 * EXP-825: the team's AGENT page — the ONE launcher on every client. The
 * composer card ([AgentComposer]: subject chips, the message, images, the
 * labelled submit), the `@`/`#`/`:` candidate menu under it, the options line
 * ([AgentOptionsRow]) and the start captions, under the dawn
 * ([AgentPageFrame], FEED-100). The "+" opens THE composer menu
 * ([ComposerPlusMenuSheet], composer-menu.json). EXP-1291: a send is ONE
 * `sessions.send` new root in the current team; the page lands on it.
 *
 * Two mounts: the Agent TAB (the bar's chat arm and the app's landing
 * screen; empty seed, no back button, the floating bar overlays it — [onBack]
 * null) and a PUSHED detail (no tab bar, native back) reached from every play
 * button with a preselection ([AgentComposerSeed] on the `agent?…` route): the issue
 * detail's Start coding, the bulk bar, an action's Run, New action / a
 * suggestion, a machine's play glyph, the Fix conflicts pills. A start is
 * only a COMMAND — the shared delegate waits for the desktop's synced row and
 * this page opens the live session once (EXP-536).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentScreen(
    /** Null on the Agent TAB: no back button, and the list clears the bar. */
    onBack: (() -> Unit)?,
    /** A raw session view (`session/{id}`). */
    onOpenSession: (sessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    /** A thread (`thread/{id}`). */
    onOpenThread: (threadId: String) -> Unit = {},
    /** EXP-1291: an accepted send LANDS on its thread (the new root IS the
     *  thread), replacing this page when it was pushed; default = [onOpenThread]. */
    onLandInThread: (threadId: String) -> Unit = onOpenThread,
    viewModel: AgentComposerViewModel = hiltViewModel(),
    dataViewModel: AgentLaunchDataViewModel = hiltViewModel(),
) {
    // ── Composer state ──────────────────────────────────────────────────────
    val teamId by viewModel.teamId.collectAsStateWithLifecycle()
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val runState by viewModel.runState.collectAsStateWithLifecycle()
    val candidateDevices by viewModel.candidateDevices.collectAsStateWithLifecycle()
    val onlineDevices by viewModel.onlineDevices.collectAsStateWithLifecycle()
    val device by viewModel.device.collectAsStateWithLifecycle()
    // EXP-836: what a play button's machine request could not be honoured as.
    val deviceRequestNote by viewModel.deviceRequestNote.collectAsStateWithLifecycle()
    val launch by viewModel.launch.collectAsStateWithLifecycle()
    val subject by viewModel.subject.collectAsStateWithLifecycle()
    val draft by viewModel.draft.collectAsStateWithLifecycle()
    val images by viewModel.images.collectAsStateWithLifecycle()
    val imageError by viewModel.imageError.collectAsStateWithLifecycle()
    val resume by viewModel.resume.collectAsStateWithLifecycle()
    val sending by viewModel.sending.collectAsStateWithLifecycle()
    val pendingPrIssueId by viewModel.pendingPrIssueId.collectAsStateWithLifecycle()
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    val mentionMembers by viewModel.mentionMembers.collectAsStateWithLifecycle()
    val pool by viewModel.startCandidates.collectAsStateWithLifecycle()
    // EXP-1291: an action's `issue` inputs pick from the team's issues.
    val inputIssues by viewModel.inputIssues.collectAsStateWithLifecycle()
    // EXP-1249: the "+" menu's MCP servers pick and per-run computer use.
    val mcpServers by viewModel.mcpServers.collectAsStateWithLifecycle()
    val mcpServerIds by viewModel.mcpServerIds.collectAsStateWithLifecycle()
    val computerUse by viewModel.computerUse.collectAsStateWithLifecycle()
    // EXP-1270: whether Getting started still follows the card (no board yet).
    val teamHasBoard by viewModel.teamHasBoard.collectAsStateWithLifecycle()
    // EXP-1271: the start in flight, until its row lands or it times out.
    val starting by viewModel.starting.collectAsStateWithLifecycle()
    val startTimedOut by viewModel.startTimedOut.collectAsStateWithLifecycle()

    // ── Lookup data ─────────────────────────────────────────────────────────
    val actionsState by dataViewModel.actionsState.collectAsStateWithLifecycle()
    val teamRepos by dataViewModel.repos.collectAsStateWithLifecycle()
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val pullRequestOptions by dataViewModel.pullRequestOptions.collectAsStateWithLifecycle()
    val deviceWorktrees by dataViewModel.deviceWorktrees.collectAsStateWithLifecycle()
    val syncedDeviceRows by dataViewModel.deviceRows.collectAsStateWithLifecycle()

    // EXP-1291: an accepted send answers with its session — a NEW ROOT, i.e.
    // the thread itself — and the page lands there ONCE.
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    LaunchedEffect(startedSessionId) {
        val sessionId = startedSessionId ?: return@LaunchedEffect
        viewModel.consumeStartedSession()
        onLandInThread(sessionId)
    }
    // ── Subject resolution ──────────────────────────────────────────────────
    val actionSubject = subject as? ComposerSubject.Action
    val issueSubject = subject as? ComposerSubject.Issues
    val selectedAction = actionSubject?.let { picked ->
        actionsState.actions?.firstOrNull { it.id == picked.id }
    }
    val selectedActionInputs = selectedAction?.inputs.orEmpty()
    // The checked issues' rows. A seeded id outside the codeable pool (a done
    // issue the detail's play button still offers) falls back to the team's
    // issue index so its chip still renders — the server decides the rest.
    val checkedOptions = remember(issueSubject, pool, issueRefCandidates) {
        val byId = pool.associateBy { it.id }
        issueSubject?.ids.orEmpty().mapNotNull { id ->
            byId[id] ?: issueRefCandidates.firstOrNull { it.issueId == id }?.let {
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
            }
        }
    }
    val checkedCount = issueSubject?.ids?.size ?: 0

    // EXP-323: the `pr` seed resolves once the options pool has the row —
    // normalised through `optionForIssue` because the caller's issue is
    // rarely the option's representative.
    LaunchedEffect(pendingPrIssueId, pullRequestOptions, selectedActionInputs) {
        val issueId = pendingPrIssueId ?: return@LaunchedEffect
        val key = selectedActionInputs.firstOrNull { it.type == "pr" }?.key ?: return@LaunchedEffect
        val option = pullRequestOptions.optionForIssue(issueId) ?: return@LaunchedEffect
        viewModel.setInput(key, option.pullRequestId)
        viewModel.consumePendingPr()
    }
    // A team with exactly one repository pre-picks it for a chat.
    LaunchedEffect(teamRepos) { viewModel.defaultChatRepo(teamRepos.map { it.id }) }

    // ── Resume (EXP-481) ────────────────────────────────────────────────────
    val deviceRowId = device?.rowId
        ?: syncedDeviceRows.firstOrNull { it.deviceId == device?.deviceId }?.id
    val resumeCandidate = if (checkedCount == 1 && device != null) {
        resumeWorktreeFor(deviceWorktrees, deviceRowId, checkedOptions.firstOrNull()?.identifier, launch.agent)
    } else {
        null
    }

    // ── Gate ────────────────────────────────────────────────────────────────
    val repoIds = checkedOptions.mapNotNull { it.repositoryId }.toSet()
    val multiRepo = repoIds.size > 1
    val overCap = checkedCount > MAX_BATCH_ISSUES
    val agentNotReady = device?.agentNotReady(launch.agent) == true
    val hasText = draft.isNotBlank()
    val subjectOk = when (subject) {
        // A chat needs SOMETHING to say — text or an image (EXP-739: the
        // repo is optional).
        null -> hasText || images.isNotEmpty()
        is ComposerSubject.Issues -> checkedCount in 1..MAX_BATCH_ISSUES && !multiRepo
        is ComposerSubject.Action -> selectedAction != null &&
            !ActionInputValues.hasUnsupportedType(selectedActionInputs) &&
            viewModel.requiredInputsFilled(selectedActionInputs)
    }
    val busy = sending || starting != null || runState is ActionRunState.Sending
    val canSubmit = device != null && !agentNotReady && !busy && subjectOk &&
        AgentComposerPrompt.withinLimit(
            draft,
            images.count { it.isImage },
            images.filter { !it.isImage }.map { it.filename },
        )
    // The ONE pure place the subject turns into copy: the submit's contract
    // label and (EXP-1038) the headline verb above the field.
    val promptSubject = when {
        actionSubject != null -> AgentComposerPrompt.Subject.Action
        checkedCount > 0 -> AgentComposerPrompt.Subject.Issues(checkedCount)
        else -> AgentComposerPrompt.Subject.None
    }
    val submitLabel = AgentComposerPrompt.submitTitle(promptSubject)
    val headline = AgentComposerPrompt.headline(promptSubject)
    // The reason the composer cannot start right now (iOS `blocker` parity);
    // null when it can, or when the only thing missing is the message itself.
    val signedOut = DomainContract.codingAgentValues.filter { agent ->
        onlineDevices.orEmpty().any { agent in it.unauthedAgentIds }
    }
    val blocker: String? = when {
        teamId == null -> "Pick a team first."
        // The pool is still resolving (steer config / the devices shape's
        // first snapshot): no verdict yet, so no "no desktop" note either.
        device == null && candidateDevices == null -> null
        device == null -> if (signedOut.isNotEmpty()) {
            "${signedOut.joinToString(", ")} not signed in on your machines. Sign in on the machine first."
        } else {
            "No desktop online. Open the Exponential desktop app to start a session."
        }
        agentNotReady -> "Not ready on ${device?.deviceLabel?.ifBlank { device?.deviceId }}. Run the doctor there."
        multiRepo -> "Pick issues from a single repository per session."
        overCap -> "At most $MAX_BATCH_ISSUES issues per session. Split the batch."
        selectedAction != null && ActionInputValues.hasUnsupportedType(selectedActionInputs) ->
            "This action needs a newer app version."
        !AgentComposerPrompt.withinLimit(
            draft,
            images.count { it.isImage },
            images.filter { !it.isImage }.map { it.filename },
        ) ->
            "The message is too long (${AgentComposerPrompt.MAX_LENGTH} characters at most)."
        else -> null
    }

    // ── The field + its `@` / `#` / `:` autocomplete (EXP-802/EXP-805) ──────
    // The same hoisted-value pattern the steer composer uses: the draft
    // string lives in the ViewModel (it has to survive the page), the caret
    // here, and the candidate rows mount as a sibling UNDER the card so a pick
    // splices against this very value.
    var composerField by remember { mutableStateOf(TextFieldValue(draft, TextRange(draft.length))) }
    var composerArmed by remember { mutableStateOf(false) }
    LaunchedEffect(draft) {
        if (composerField.text != draft) {
            val start = composerField.selection.start.coerceIn(0, draft.length)
            val end = composerField.selection.end.coerceIn(0, draft.length)
            composerField = TextFieldValue(draft, TextRange(start, end))
            composerArmed = false
        }
    }
    val currentOnOpenIssue by rememberUpdatedState(onOpenIssue)
    val issueRefHandler = remember(issueRefCandidates) {
        IssueRefHandler(
            issueRefCandidates,
            searchServer = viewModel::searchIssueRefs,
        ) { target -> currentOnOpenIssue(target.issueId) }
    }
    val triggers = autocompleteTriggersAt(
        beforeCaret = composerField.text.take(composerField.selection.start),
        mentionsEnabled = mentionMembers.isNotEmpty(),
        refsEnabled = issueRefCandidates.isNotEmpty(),
    )
    val mentionCandidates = mentionCandidatesFor(mentionMembers, triggers.mentionQuery)
    // SLOP-28 (EXP-1259 D4): `#` opens THE search list in a bottom sheet
    // ([IssueTypeaheadSheetHost] below), so the rows under the card only ever
    // carry members and emoji.
    val refCandidates = emptyList<IssueRefTarget>()
    val emojiMatch = triggers.emoji
    val emojiData = rememberEmojiData(enabled = emojiMatch != null)
    val emojiPrefs = rememberEmojiPrefs()
    val emojiCandidates = if (emojiMatch != null && emojiData != null) {
        emojiData.search(emojiMatch.query, limit = EMOJI_TYPEAHEAD_LIMIT)
    } else {
        emptyList()
    }
    LaunchedEffect(triggers.none) { if (triggers.none) composerArmed = false }
    fun commitToken(spliced: TextFieldValue?) {
        val next = spliced ?: return
        composerField = next
        viewModel.setDraft(next.text)
        composerArmed = false
    }
    // `:tada:` — the closing colon plus an EXACT shortcode commits at once.
    val closedShortcode = emojiMatch?.takeIf { it.closed }?.query
    LaunchedEffect(closedShortcode, emojiData, composerArmed) {
        val code = closedShortcode ?: return@LaunchedEffect
        if (!composerArmed) return@LaunchedEffect
        val record = emojiData?.findShortcode(code) ?: return@LaunchedEffect
        commitToken(composerField.withEmoji(record, trailingSpace = false))
        emojiPrefs.pushRecent(record.unicode)
    }
    val menuOpen = composerArmed &&
        (mentionCandidates.isNotEmpty() || refCandidates.isNotEmpty() || emojiCandidates.isNotEmpty())
    // EXP-892: the highlighted row — the TOP one while typing, moved by a
    // hardware ↑/↓ and picked by Enter/Tab, exactly like the `/` command menu.
    val candidateCount =
        autocompleteCandidateCount(mentionCandidates, refCandidates, emojiCandidates)
    var menuSelected by remember { mutableIntStateOf(0) }
    LaunchedEffect(mentionCandidates, refCandidates, emojiCandidates) { menuSelected = 0 }
    fun pickSelectedToken() {
        pickAutocompleteAt(
            index = menuSelected.coerceIn(0, (candidateCount - 1).coerceAtLeast(0)),
            mentionCandidates = mentionCandidates,
            refCandidates = refCandidates,
            emojiCandidates = emojiCandidates,
            onPickMention = { commitToken(composerField.withMention(it)) },
            onPickIssueRef = { commitToken(composerField.withIssueRef(it)) },
            onPickEmoji = { record ->
                commitToken(composerField.withEmoji(record, trailingSpace = true))
                emojiPrefs.pushRecent(record.unicode)
            },
        )
    }
    // Back dismisses the menu, and only the menu.
    BackHandler(enabled = menuOpen) { composerArmed = false }
    // The `#` sheet takes the token over and hands focus back to the field.
    val composerFocus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    IssueTypeaheadSheetHost(
        handler = issueRefHandler.takeIf { issueRefCandidates.isNotEmpty() },
        field = composerField,
        armed = composerArmed && triggers.issueRef != null,
        onDisarm = { composerArmed = false },
        onCommit = { next -> commitToken(next) },
        onRefocus = {
            runCatching { composerFocus.requestFocus() }
            keyboard?.show()
        },
    )

    // ── Feedback (FEED-79, EXP-1271): a refused start and a start that never
    // landed are toasts; Retry re-submits the kept draft ────────────────────
    val toaster = LocalToaster.current
    // Retry re-sends the same draft under the SAME startId (the device
    // starts an id once); a draft changed meanwhile is a fresh start. It
    // passes the Start button's gate (web `submitWith`: a no-op while
    // `blocked`), so a draft emptied or made multi-repo after the failure
    // never reaches the server to be refused a second time.
    val retryStart by rememberUpdatedState {
        if (canSubmit) viewModel.submit(selectedAction, resumeCandidate != null)
    }
    LaunchedEffect(runState) {
        val failed = runState as? ActionRunState.Failed ?: return@LaunchedEffect
        val toastId = toaster.error(
            FeedbackStates.errorTitle("start", "the session"),
            failed.message,
            ToastAction(FeedbackStates.RETRY) { retryStart() },
        )
        // Recheck R3: a start of unknown fate is still followed; its late
        // row takes this toast down.
        viewModel.clearRunFailure(toastId)
    }
    LaunchedEffect(startTimedOut) {
        val label = startTimedOut ?: return@LaunchedEffect
        val toastId = toaster.error(
            FeedbackStates.STARTING_TIMEOUT_TITLE,
            FeedbackStates.startingTimeoutDescription(label),
            ToastAction(FeedbackStates.RETRY) { retryStart() },
        )
        viewModel.consumeStartTimedOut(toastId)
    }
    // Review #3: the timed-out start's row landed late: its toast goes.
    val dismissToast by viewModel.dismissToast.collectAsStateWithLifecycle()
    LaunchedEffect(dismissToast) {
        val id = dismissToast ?: return@LaunchedEffect
        toaster.dismiss(id)
        viewModel.consumeDismissToast()
    }
    // A start whose attachment upload failed: landed uploads are kept, so
    // Retry uploads the rest and starts (web parity).
    val uploadFailure by viewModel.uploadFailure.collectAsStateWithLifecycle()
    FailedActionToast(uploadFailure, "upload", "the attachment", onShown = viewModel::clearUploadFailure, retry = { retryStart() })

    // ── Attachments (EXP-511, wave D): the system document picker, ANY type
    // (images included), feeds the pending list ─────────────────────────────
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
                is ComposerPick.Read -> viewModel.addImage(picked.uri, picked.bytes, picked.filename, picked.mime)
                is ComposerPick.Refused -> viewModel.refuseAttachment(picked.message)
            }
        }
    }

    // ── Sheets + dialogs ────────────────────────────────────────────────────
    // EXP-1249: the "+" menu and the pickers its submenu rows open (a picker
    // sheet IS the pushed page: two stacked sheets are a dead end here).
    var plusMenuOpen by remember { mutableStateOf(false) }
    var effortPickerOpen by remember { mutableStateOf(false) }
    var subagentPickerOpen by remember { mutableStateOf(false) }
    var mcpPickerOpen by remember { mutableStateOf(false) }

    AgentPageFrame(
        onBack = onBack,
        dawn = promptSubject == AgentComposerPrompt.Subject.None,
        // EXP-1316 (1f): a board-less team's Getting started block sits
        // ABOVE the dawn (the column stands on the bottom).
        lead = {
            if (steerEnabled == true && teamHasBoard == false) {
                item(key = "__home_getting_started__") {
                    GettingStartedSection(
                        onOpenAgent = {
                            runCatching { composerFocus.requestFocus() }
                            keyboard?.show()
                        },
                        modifier = Modifier.padding(bottom = (AgentHome.COMPOSER_TO_GETTING_STARTED - 6).dp),
                    )
                }
            }
        },
    ) {
            when (steerEnabled) {
                // Web parity: without the relay nothing here can be started.
                false -> item(key = "__relay_off__") {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .glassRow()
                            .padding(horizontal = 12.dp, vertical = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(
                            ExpIcons.uiDeviceOffline,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            "Remote start isn't available on this server. Start sessions from the desktop app; live sessions show up in Threads.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
                true -> {
                    // EXP-1038: the run's SUBJECT heads the page — the
                    // contract verb ("Run" · "Implement") with the action or
                    // issue chips beside it, above everything the composer
                    // offers, so a prefilled start reads as a thing about to
                    // be sent, not as a chip. A CHAT emits no item at all
                    // (web `LaunchHeadline` returns null, ×4): the field's
                    // placeholder already says "Ask the agent…", and the same
                    // sentence twice is not a heading.
                    if (promptSubject != AgentComposerPrompt.Subject.None) {
                        item(key = "__composer_headline__") {
                            AgentComposerHeadline(
                                headline = headline,
                                issueChips = checkedOptions,
                                onRemoveIssue = viewModel::toggleIssue,
                                actionChip = selectedAction,
                                onClearAction = viewModel::clearAction,
                            )
                        }
                    }
                    item(key = "__composer__") {
                        AgentComposer(
                            value = composerField,
                            // A user edit is the ONLY write that may arm the
                            // `@`/`#`/`:` menu (web parity: the autocomplete
                            // opens on a document change, never a caret move).
                            onValueChange = { next ->
                                if (next.text != composerField.text) composerArmed = true
                                composerField = next
                                viewModel.setDraft(next.text)
                            },
                            onValueRewrite = { next ->
                                composerField = next
                                viewModel.setDraft(next.text)
                            },
                            // EXP-892: while the `#`/`@`/`:` menu is up it
                            // owns ↑/↓/Enter/Tab/Escape — and only then.
                            fieldModifier = Modifier.focusRequester(composerFocus).onPreviewKeyEvent { event ->
                                if (event.type != KeyEventType.KeyDown) {
                                    return@onPreviewKeyEvent false
                                }
                                if (!menuOpen || candidateCount == 0) {
                                    return@onPreviewKeyEvent false
                                }
                                when (event.key) {
                                    Key.DirectionUp -> {
                                        menuSelected =
                                            (menuSelected - 1 + candidateCount) % candidateCount
                                        true
                                    }
                                    Key.DirectionDown -> {
                                        menuSelected = (menuSelected + 1) % candidateCount
                                        true
                                    }
                                    Key.Enter, Key.NumPadEnter, Key.Tab -> {
                                        pickSelectedToken()
                                        true
                                    }
                                    Key.Escape -> {
                                        composerArmed = false
                                        true
                                    }
                                    else -> false
                                }
                            },
                            // The field's prompt per subject — a chat asks for
                            // the message, a picked action shows its own
                            // composer hint (EXP-825), anything else what is
                            // optional next to it.
                            placeholder = composerPlaceholder(subject, selectedAction, home = true),
                            // EXP-1038: the CHIP is in the headline above; the
                            // card takes the action only for its pick rows.
                            actionChip = selectedAction,
                            inputValues = actionSubject?.inputs.orEmpty(),
                            repos = teamRepos,
                            boards = boardOptions,
                            pullRequests = pullRequestOptions,
                            onInputChange = viewModel::setInput,
                            pendingImages = images,
                            imageError = imageError,
                            sending = sending || starting != null,
                            starting = starting,
                            onOpenMenu = { plusMenuOpen = true },
                            onRemoveImage = viewModel::removeImage,
                            submitLabel = submitLabel,
                            canSubmit = canSubmit,
                            onSubmit = { viewModel.submit(selectedAction, resumeCandidate != null) },
                            issues = inputIssues,
                        )
                    }
                    if (menuOpen) {
                        item(key = "__autocomplete__") {
                            AutocompleteRows(
                                mentionCandidates = mentionCandidates,
                                refCandidates = refCandidates,
                                emojiCandidates = emojiCandidates,
                                selectedIndex = menuSelected,
                                onPickMention = { commitToken(composerField.withMention(it)) },
                                onPickIssueRef = { commitToken(composerField.withIssueRef(it)) },
                                onPickEmoji = { record ->
                                    commitToken(composerField.withEmoji(record, trailingSpace = true))
                                    emojiPrefs.pushRecent(record.unicode)
                                },
                                modifier = Modifier.fillMaxWidth(),
                            )
                        }
                    }
                    item(key = "__options__") {
                        AgentOptionsRow(
                            devices = candidateDevices.orEmpty(),
                            device = device,
                            onDeviceChange = viewModel::setDevice,
                            launch = launch,
                            // EXP-872: the settled machine's logins — picking
                            // one sets the account AND its agent.
                            accountOptions = accountOptionsFor(
                                device,
                                availableAgentsFor(device),
                            ),
                            onAccountChange = viewModel::selectAccount,
                            onModelChange = viewModel::setModel,
                            onPlanModeChange = viewModel::setPlanMode,
                            resumeCandidate = resumeCandidate,
                            resume = resume,
                            onResumeChange = viewModel::setResume,
                        )
                    }
                    item(key = "__captions__") {
                        Column(modifier = Modifier.padding(horizontal = 4.dp)) {
                            // EXP-836: the machine a play button named is not
                            // the one this run would go to — say which and why
                            // (web `deviceRequestNote`, same sentences) instead
                            // of silently running on the default machine. It
                            // sits ABOVE the blocker: the request is about the
                            // options line right above it.
                            deviceRequestNote?.let { note ->
                                Text(
                                    note,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = NeedsInputAmber,
                                    modifier = Modifier.testTag("launch-device-request-note"),
                                )
                            }
                            val notReadyDevice = device
                            when {
                                // EXP-1196: the picked agent cannot run on the
                                // picked machine — its failing readiness ROW
                                // (with the action), the sentence as fallback.
                                blocker != null && agentNotReady && notReadyDevice != null &&
                                    teamId != null -> DeviceNotReadyRow(
                                    device = notReadyDevice,
                                    agent = launch.agent,
                                    fallback = blocker,
                                )
                                blocker != null -> Text(
                                    blocker,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                    modifier = Modifier.testTag("launch-not-ready-note"),
                                )
                                checkedCount > LARGE_BATCH_HINT_THRESHOLD -> Text(
                                    "Large batches are token-expensive.",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                            }
                            // EXP-1271: the start in flight is the CARD's
                            // starting row; a refused or lost start a toast.
                        }
                    }
                }
                null -> Unit
            }
    }

    if (plusMenuOpen) {
        val claude = launch.agent == DEFAULT_AGENT
        ComposerPlusMenuSheet(
            layout = ComposerMenu.composerMenuLayout(
                ComposerMenuConditions(
                    subagentModel = supportsSubagentModel(launch.agent),
                    ultracode = claude,
                    // EXP-1291: actions no longer carry their own MCP list, so
                    // no subject owns it — the team's servers alone decide.
                    mcp = mcpServers.isNotEmpty(),
                    computerUse = device?.canToggleComputerUse == true,
                ),
            ),
            codex = launch.agent == "codex",
            effortValue = effortLabel(launch.effort),
            effortEnabled = !(claude && launch.ultracode),
            subagentsValue = subagentModelLabel(launch.subagentModel),
            ultracode = launch.ultracode,
            mcpValue = mcpPickValue(mcpServerIds.size),
            computerUse = computerUse,
            canAttach = images.count { it.isImage } < MAX_STEER_IMAGES ||
                images.count { !it.isImage } < MAX_STEER_FILES,
            onRow = { row ->
                plusMenuOpen = false
                when (row) {
                    // SLOP-28: these two push their search-list page inside
                    // the menu ([ComposerPlusMenuSheet]) and never land here.
                    ComposerMenuRowId.ImplementIssue, ComposerMenuRowId.RunAction -> Unit
                    ComposerMenuRowId.AddFile -> filePicker.launch(arrayOf("*/*"))
                    ComposerMenuRowId.Effort -> effortPickerOpen = true
                    ComposerMenuRowId.Subagents -> subagentPickerOpen = true
                    ComposerMenuRowId.McpServers -> mcpPickerOpen = true
                    ComposerMenuRowId.Ultracode, ComposerMenuRowId.ComputerUse -> Unit
                }
            },
            onUltracodeChange = viewModel::setUltracode,
            onComputerUseChange = viewModel::setComputerUse,
            onDismiss = { plusMenuOpen = false },
            issuePage = { close ->
                AgentIssuePickerPage(
                    issues = pool,
                    checkedIds = issueSubject?.ids?.toSet().orEmpty(),
                    onToggle = viewModel::toggleIssue,
                    onDone = close,
                )
            },
            actionPage = { close ->
                AgentActionPickerPage(
                    actions = actionsState.actions,
                    error = actionsState.error,
                    selectedId = actionSubject?.id,
                    onPick = { id ->
                        viewModel.pickAction(id)
                        close()
                    },
                )
            },
        )
    }
    if (effortPickerOpen) {
        Picker(
            items = (listOf(CLI_DEFAULT_EFFORT) + effortValuesFor(launch.agent)).map { PickerItem(it, effortLabel(it)) },
            mode = PickerMode.Single,
            value = setOf(launch.effort),
            onChange = { picked -> picked.firstOrNull()?.let(viewModel::setEffort) },
            title = if (launch.agent == "codex") "Reasoning" else "Effort",
            open = true,
            onOpenChange = { open -> if (!open) effortPickerOpen = false },
        )
    }
    if (subagentPickerOpen) {
        // THE model picker rows (composer-menu.json `subagents`).
        com.exponential.app.ui.components.picker.ModelPicker(
            options = subagentModelOptions().map {
                com.exponential.app.ui.components.picker.ModelPickerOption(it, subagentModelLabel(it))
            },
            value = launch.subagentModel,
            onChange = viewModel::setSubagentModel,
            title = SUBAGENT_MODEL_LABEL,
            open = true,
            onOpenChange = { open -> if (!open) subagentPickerOpen = false },
        )
    }
    if (mcpPickerOpen) {
        McpServerPicker(
            servers = mcpServers,
            value = mcpServerIds,
            onToggle = viewModel::toggleMcpServer,
            open = true,
            onOpenChange = { open -> if (!open) mcpPickerOpen = false },
        )
    }

}

/**
 * The Agent page's frame: the transparent "Agent" top bar over the page's one
 * column ([content]) from the top down. EXP-1306 (the approved phone home =
 * iOS `AgentPageView`): [dawn] leads the column — the restored DAWN
 * (FEED-100), the big Exponential mark's upper half, dim — then the composer
 * card + options line. EXP-1316 (1f, composer-home.json `phone.composer:
 * bottom`): the column stands on the BOTTOM (above the floating tab bar,
 * lifted by the keyboard), the dawn on the card; no Running list. [lead]
 * (Getting started) sits above the dawn.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AgentPageFrame(
    onBack: (() -> Unit)?,
    /** The dawn heads the column; false while a picked subject's headline
     *  does (iOS parity). */
    dawn: Boolean = true,
    lead: LazyListScope.() -> Unit = {},
    content: LazyListScope.() -> Unit,
) {
    Box(modifier = Modifier.fillMaxSize().testTag(AgentHome.HOME_TEST_ID)) {
        Scaffold(
            containerColor = Color.Transparent,
            topBar = {
                CenterAlignedTopAppBar(
                    title = { Text("Agent") },
                    navigationIcon = { onBack?.let { TopBarBackButton(onClick = it) } },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
                )
            },
        ) { padding ->
            LazyColumn(
                modifier = Modifier
                    .padding(padding)
                    .consumeWindowInsets(padding)
                    .imePadding()
                    .fillMaxSize()
                    .testTag("agent-page"),
                // As a tab the floating bar overlays the page: the last row
                // must sit above the pill, not under it.
                contentPadding = PaddingValues(
                    start = 16.dp,
                    end = 16.dp,
                    top = DAWN_TOP.dp,
                    bottom = if (onBack == null) BottomBarInset else 24.dp,
                ),
                verticalArrangement = Arrangement.spacedBy(COLUMN_SPACING.dp, Alignment.Bottom),
            ) {
                lead()
                if (dawn) {
                    item(key = "__dawn__") {
                        AgentHomeDawn()
                    }
                }
                content()
            }
        }
    }
}

/** The dawn's geometry (dp, iOS `HomeDawn`): the mark's diameter (the dawn
 *  shows its upper half), the gap under the title, the column's spacing the
 *  dawn swallows to stand on the card. */
private const val DAWN_MARK = 240
private const val DAWN_TOP = 16
private const val COLUMN_SPACING = 6

/** The dawn's foreground alpha (iOS `HomeDawn.opacity`, the IDE's ink). */
private const val DAWN_ALPHA = 0.025f

/**
 * FEED-100: the dawn — [ExponentialMark] in the foreground colour at a low
 * alpha, clipped to its upper half (a half-disc standing on the card) and
 * FADING toward it (EXP-1314: solid for its top third, transparent at the
 * card, no gap — the IDE's `dawn` on every client, ~69% of the column). It reports
 * [COLUMN_SPACING] less height so the column's spacing lands under it and its
 * horizon meets the card's top edge.
 */
@Composable
internal fun AgentHomeDawn(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints)
                layout(placeable.width, placeable.height - COLUMN_SPACING.dp.roundToPx()) {
                    placeable.place(0, 0)
                }
            }
            .fillMaxWidth()
            .height((DAWN_MARK / 2).dp)
            .clipToBounds()
            .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)
            .drawWithContent {
                drawContent()
                drawRect(
                    brush = Brush.verticalGradient(0.33f to Color.Black, 1f to Color.Transparent),
                    blendMode = BlendMode.DstIn,
                )
            },
        contentAlignment = Alignment.TopCenter,
    ) {
        ExponentialMark(
            size = DAWN_MARK.dp,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = DAWN_ALPHA),
            modifier = Modifier
                .wrapContentHeight(align = Alignment.Top, unbounded = true)
                .testTag(AgentHome.MARK_TEST_ID),
        )
    }
}
