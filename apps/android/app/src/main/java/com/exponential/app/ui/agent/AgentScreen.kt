package com.exponential.app.ui.agent

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.AutomationEntity
import com.exponential.app.domain.ActionInputValues
import com.exponential.app.domain.AgentComposerPrompt
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.ChatSuggestions
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.MAX_STEER_IMAGES
import com.exponential.app.domain.MergeTarget
import com.exponential.app.domain.resumeWorktreeFor
import com.exponential.app.ui.actions.ActionEditSheet
import com.exponential.app.ui.actions.ActionsViewModel
import com.exponential.app.ui.actions.AutomationFormSheet
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.emoji.rememberEmojiData
import com.exponential.app.ui.emoji.rememberEmojiPrefs
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.AutocompleteRows
import com.exponential.app.ui.markdown.EMOJI_TYPEAHEAD_LIMIT
import com.exponential.app.ui.markdown.IssueRefHandler
import com.exponential.app.ui.markdown.MENTION_CANDIDATE_LIMIT
import com.exponential.app.ui.markdown.MarkdownMediaUtils
import com.exponential.app.ui.markdown.autocompleteTriggersAt
import com.exponential.app.ui.markdown.mentionCandidatesFor
import com.exponential.app.ui.markdown.withEmoji
import com.exponential.app.ui.markdown.withIssueRef
import com.exponential.app.ui.markdown.withMention
import com.exponential.app.ui.session.AgentRow
import com.exponential.app.ui.session.AgentsViewModel
import com.exponential.app.ui.steer.ActionRunState
import com.exponential.app.ui.steer.SteerRunCaptionRow
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * EXP-825: the team's AGENT page — the ONE launcher on every client. The
 * composer card ([AgentComposer]: subject chips, the message, images, the
 * labelled submit), the `@`/`#`/`:` candidate menu under it, the options line
 * ([AgentOptionsRow]), the start captions, then the caller's Running and Past
 * sessions ([agentSessionsList], moved here from the Devices tab, which keeps
 * machines only — web parity, EXP-818).
 *
 * A PUSHED detail (no tab bar, native back), reached from the Chat FAB on
 * Devices/Actions with an empty seed and from every play button with a
 * preselection ([AgentComposerSeed] on the `agent?…` route): the issue
 * detail's Start coding, the bulk bar, an action's Run, New action / a
 * suggestion, a machine's play glyph, the Fix conflicts pills. A start is
 * only a COMMAND — the shared delegate waits for the desktop's synced row and
 * this page opens the live session once (EXP-536).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentScreen(
    onBack: () -> Unit,
    onOpenSteer: (codingSessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    viewModel: AgentComposerViewModel = hiltViewModel(),
    dataViewModel: AgentLaunchDataViewModel = hiltViewModel(),
    // The sessions under the composer: the Devices tab's own model, reused
    // rather than mirrored (both read the same synced shapes).
    sessionsViewModel: AgentsViewModel = hiltViewModel(),
    // EXP-694: a session row's trailing control opens the run's ACTION or
    // its AUTOMATION, so the list needs the Actions surface's rows and its
    // owner-gated automation mutation.
    actionsViewModel: ActionsViewModel = hiltViewModel(),
) {
    // ── Composer state ──────────────────────────────────────────────────────
    val teamId by viewModel.teamId.collectAsStateWithLifecycle()
    val steerEnabled by viewModel.steerEnabled.collectAsStateWithLifecycle()
    val runState by viewModel.runState.collectAsStateWithLifecycle()
    val candidateDevices by viewModel.candidateDevices.collectAsStateWithLifecycle()
    val onlineDevices by viewModel.onlineDevices.collectAsStateWithLifecycle()
    val device by viewModel.device.collectAsStateWithLifecycle()
    val launch by viewModel.launch.collectAsStateWithLifecycle()
    val subject by viewModel.subject.collectAsStateWithLifecycle()
    val draft by viewModel.draft.collectAsStateWithLifecycle()
    val images by viewModel.images.collectAsStateWithLifecycle()
    val imageError by viewModel.imageError.collectAsStateWithLifecycle()
    val chatRepoId by viewModel.chatRepoId.collectAsStateWithLifecycle()
    val resume by viewModel.resume.collectAsStateWithLifecycle()
    val sending by viewModel.sending.collectAsStateWithLifecycle()
    val pendingPrIssueId by viewModel.pendingPrIssueId.collectAsStateWithLifecycle()
    val issueRefCandidates by viewModel.issueRefCandidates.collectAsStateWithLifecycle()
    val mentionMembers by viewModel.mentionMembers.collectAsStateWithLifecycle()
    val pool by viewModel.startCandidates.collectAsStateWithLifecycle()

    // ── Lookup data ─────────────────────────────────────────────────────────
    val actionsState by dataViewModel.actionsState.collectAsStateWithLifecycle()
    val teamRepos by dataViewModel.repos.collectAsStateWithLifecycle()
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val pullRequestOptions by dataViewModel.pullRequestOptions.collectAsStateWithLifecycle()
    val deviceWorktrees by dataViewModel.deviceWorktrees.collectAsStateWithLifecycle()
    val syncedDeviceRows by dataViewModel.deviceRows.collectAsStateWithLifecycle()

    // ── Sessions ────────────────────────────────────────────────────────────
    val sessionsState by sessionsViewModel.state.collectAsStateWithLifecycle()
    val pastRuns by sessionsViewModel.pastRuns.collectAsStateWithLifecycle()
    val merging by sessionsViewModel.merging.collectAsStateWithLifecycle()
    val mergeErrors by sessionsViewModel.mergeErrors.collectAsStateWithLifecycle()
    val teamActions by actionsViewModel.state.collectAsStateWithLifecycle()
    val automations by actionsViewModel.automations.collectAsStateWithLifecycle()
    val automationDevices by actionsViewModel.automationDevices.collectAsStateWithLifecycle()
    val automationBusy by actionsViewModel.automationBusy.collectAsStateWithLifecycle()
    val automationError by actionsViewModel.automationError.collectAsStateWithLifecycle()
    val isTeamOwner by actionsViewModel.isTeamOwner.collectAsStateWithLifecycle()

    // The desktop picked the start up — open the live session ONCE (EXP-536).
    val startedSessionId by viewModel.startedSessionId.collectAsStateWithLifecycle()
    LaunchedEffect(startedSessionId) {
        startedSessionId?.let {
            viewModel.consumeStartedSession()
            onOpenSteer(it)
        }
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
        viewModel.setInput(key, option.issueId)
        viewModel.consumePendingPr()
    }
    // EXP-349: repo-typed inputs seed from the action's bound repository.
    LaunchedEffect(selectedAction) { selectedAction?.let(viewModel::seedRepoInputs) }
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
            viewModel.requiredInputsFilled(selectedActionInputs) &&
            // The creator derives everything from the request text.
            (actionSubject?.id != DomainContract.builtinCreateActionId || hasText)
    }
    val busy = sending || runState is ActionRunState.Sending
    val canSubmit = device != null && !agentNotReady && !busy && subjectOk &&
        AgentComposerPrompt.withinLimit(draft, images.size)
    val submitLabel = AgentComposerPrompt.submitTitle(
        when {
            actionSubject != null -> AgentComposerPrompt.Subject.Action
            checkedCount > 0 -> AgentComposerPrompt.Subject.Issues(checkedCount)
            else -> AgentComposerPrompt.Subject.None
        },
    )
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
            "No desktop online. Open the Exponential desktop app to start a run."
        }
        agentNotReady -> "Not ready on ${device?.deviceLabel?.ifBlank { device?.deviceId }}. Run the doctor there."
        multiRepo -> "Pick issues from a single repository per run."
        overCap -> "At most $MAX_BATCH_ISSUES issues per run. Split the batch."
        selectedAction != null && ActionInputValues.hasUnsupportedType(selectedActionInputs) ->
            "This action needs a newer app version."
        !AgentComposerPrompt.withinLimit(draft, images.size) ->
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
    // EXP-820: ONE draw per mount (web `useState(() => pickChatSuggestions())`)
    // — chips that reshuffled on every recomposition would be unreadable.
    val suggestions = remember { ChatSuggestions.pick() }
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
        IssueRefHandler(issueRefCandidates) { target -> currentOnOpenIssue(target.issueId) }
    }
    val triggers = autocompleteTriggersAt(
        beforeCaret = composerField.text.take(composerField.selection.start),
        mentionsEnabled = mentionMembers.isNotEmpty(),
        refsEnabled = issueRefCandidates.isNotEmpty(),
    )
    val mentionCandidates = mentionCandidatesFor(mentionMembers, triggers.mentionQuery)
    val refCandidates = triggers.issueRefQuery
        ?.let { issueRefHandler.search(it, limit = MENTION_CANDIDATE_LIMIT) }
        ?: emptyList()
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
    // Back dismisses the menu, and only the menu.
    BackHandler(enabled = menuOpen) { composerArmed = false }

    // ── Images (EXP-511): the system photo picker feeds the pending list ────
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val imagePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.PickMultipleVisualMedia(MAX_STEER_IMAGES),
    ) { uris: List<Uri> ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            uris.forEach { uri ->
                // ContentResolver reads can stream from a cloud-backed
                // provider — never on the main thread.
                val picked = withContext(Dispatchers.IO) {
                    val bytes = MarkdownMediaUtils.readBytes(context, uri) ?: return@withContext null
                    Triple(bytes, MarkdownMediaUtils.guessFilename(context, uri), MarkdownMediaUtils.guessMimeType(context, uri))
                } ?: return@forEach
                viewModel.addImage(uri, picked.first, picked.second, picked.third)
            }
        }
    }

    // ── Sheets + dialogs ────────────────────────────────────────────────────
    var issuePickerOpen by remember { mutableStateOf(false) }
    var actionPickerOpen by remember { mutableStateOf(false) }
    var optionsOpen by remember { mutableStateOf(false) }
    var editActionId by remember { mutableStateOf<String?>(null) }
    var editAutomation by remember { mutableStateOf<AutomationEntity?>(null) }
    var mergeConfirmRow by remember { mutableStateOf<AgentRow?>(null) }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text("Agent") },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .testTag("agent-page"),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            when (steerEnabled) {
                // Web parity: without the relay nothing here can be started —
                // the page still lists the sessions that synced in.
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
                            "Remote start isn't available on this server. Start runs from the desktop app; live sessions show up below.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
                true -> {
                    // EXP-820: a few suggestion chips over an EMPTY new chat —
                    // drawn once per mount from the pool shared ×4, and gone the
                    // moment there is a subject or a word typed, where they
                    // would only be in the way. Tapping one puts it in the field
                    // and, for a `#` suggestion, leaves the caret behind the
                    // `#` so the issue picker opens at once.
                    if (subject == null && draft.isEmpty()) {
                        item(key = "__suggestions__") {
                            ChatSuggestionChips(
                                suggestions = suggestions,
                                onPick = { suggestion ->
                                    val caret = ChatSuggestions.caretOffset(suggestion)
                                    composerField = TextFieldValue(
                                        suggestion,
                                        TextRange(caret),
                                    )
                                    viewModel.setDraft(suggestion)
                                    // A `#` suggestion is exactly the case the
                                    // armed latch exists for: this IS a text
                                    // change, so the menu may open.
                                    composerArmed = suggestion.contains('#')
                                },
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
                            fieldModifier = Modifier,
                            // The field's prompt per subject — a chat asks for
                            // the message, a picked action shows its own
                            // composer hint (EXP-825), anything else what is
                            // optional next to it.
                            placeholder = composerPlaceholder(subject, selectedAction),
                            issueChips = checkedOptions,
                            onRemoveIssue = viewModel::toggleIssue,
                            actionChip = selectedAction,
                            onClearAction = viewModel::clearAction,
                            inputValues = actionSubject?.inputs.orEmpty(),
                            repos = teamRepos,
                            boards = boardOptions,
                            pullRequests = pullRequestOptions,
                            onInputChange = viewModel::setInput,
                            pendingImages = images,
                            imageError = imageError,
                            canAttach = images.size < MAX_STEER_IMAGES,
                            sending = sending,
                            onPickIssues = { issuePickerOpen = true },
                            onPickActions = { actionPickerOpen = true },
                            onPickImages = {
                                imagePicker.launch(
                                    PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly),
                                )
                            },
                            onRemoveImage = viewModel::removeImage,
                            submitLabel = submitLabel,
                            canSubmit = canSubmit,
                            onSubmit = { viewModel.submit(selectedAction, resumeCandidate != null) },
                        )
                    }
                    if (menuOpen) {
                        item(key = "__autocomplete__") {
                            AutocompleteRows(
                                mentionCandidates = mentionCandidates,
                                refCandidates = refCandidates,
                                emojiCandidates = emojiCandidates,
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
                            availableAgents = availableAgentsFor(device),
                            onAgentChange = viewModel::selectAgent,
                            onModelChange = viewModel::setModel,
                            onPlanModeChange = viewModel::setPlanMode,
                            resumeCandidate = resumeCandidate,
                            resume = resume,
                            onResumeChange = viewModel::setResume,
                            showRepository = subject == null,
                            repos = teamRepos,
                            chatRepoId = chatRepoId,
                            onChatRepoChange = viewModel::setChatRepoId,
                            onMore = { optionsOpen = true },
                        )
                    }
                    item(key = "__captions__") {
                        Column(modifier = Modifier.padding(horizontal = 4.dp)) {
                            when {
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
                            // "Sending…" / "Start sent to … Waiting for the
                            // desktop…" / a refused send (EXP-536).
                            SteerRunCaptionRow(runState)
                        }
                    }
                }
                null -> Unit
            }
            item(key = "__sessions_gap__") { Spacer(Modifier.height(4.dp)) }
            agentSessionsList(
                rows = sessionsState.rows,
                pastRuns = pastRuns,
                steerEnabled = steerEnabled == true,
                merging = merging,
                mergeErrors = mergeErrors,
                actions = teamActions.actions,
                automations = automations,
                isTeamOwner = isTeamOwner,
                onOpenSteer = onOpenSteer,
                onOpenIssue = onOpenIssue,
                onEditAction = { editActionId = it },
                onEditAutomation = {
                    actionsViewModel.clearAutomationError()
                    editAutomation = it
                },
                onMerge = { mergeConfirmRow = it },
                // The composer IS the launcher: seed the builtin with this
                // row's PR in place (EXP-486, Reviews parity EXP-323).
                onFixConflicts = { issueId ->
                    viewModel.applySeed(
                        AgentComposerSeed(
                            actionId = DomainContract.builtinFixConflictsId,
                            prIssueId = issueId,
                        ),
                    )
                },
            )
        }
    }

    if (issuePickerOpen) {
        AgentIssuePickerSheet(
            issues = pool,
            checkedIds = issueSubject?.ids?.toSet().orEmpty(),
            onToggle = viewModel::toggleIssue,
            onDismiss = { issuePickerOpen = false },
        )
    }
    if (actionPickerOpen) {
        AgentActionPickerSheet(
            actions = actionsState.actions,
            error = actionsState.error,
            selectedId = actionSubject?.id,
            onPick = viewModel::pickAction,
            onDismiss = { actionPickerOpen = false },
        )
    }
    if (optionsOpen) {
        AgentOptionsSheet(
            launch = launch,
            device = device,
            onEffortChange = viewModel::setEffort,
            onUltracodeChange = viewModel::setUltracode,
            onAccountChange = viewModel::setAccount,
            onDismiss = { optionsOpen = false },
        )
    }

    // EXP-694 (S6): the run's action, opened straight off its session row —
    // the full editor, read-only for members.
    editActionId?.let { id ->
        ActionEditSheet(actionId = id, onDismiss = { editActionId = null })
    }

    // An automated run opens the automation that fired it instead (the
    // Automations tab's own form, owner-gated).
    editAutomation?.let { automation ->
        AutomationFormSheet(
            actions = teamActions.actions,
            devices = automationDevices,
            busy = automationBusy,
            error = automationError,
            editing = automation,
            onSubmit = { actionId, deviceId, trigger, agent, model, effort ->
                actionsViewModel.updateAutomation(
                    automationId = automation.id,
                    actionId = actionId,
                    deviceId = deviceId,
                    trigger = trigger,
                    agent = agent,
                    model = model,
                    effort = effort,
                    onDone = { editAutomation = null },
                )
            },
            onDismiss = { editAutomation = null },
        )
    }

    // EXP-498: merging always closes the session too, so the merge is
    // confirm-gated — same shape as the Reviews dialog.
    mergeConfirmRow?.let { row ->
        val target = row.mergeTarget
        AlertDialog(
            onDismissRequest = { mergeConfirmRow = null },
            title = { Text("Merge pull request?") },
            text = {
                Text(
                    when (target) {
                        is MergeTarget.Session ->
                            "Merges this run's pull request and closes the coding session."
                        else ->
                            "Merges the pull request, completes every linked issue, " +
                                "and closes the coding session."
                    },
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        target?.let(sessionsViewModel::merge)
                        mergeConfirmRow = null
                    },
                ) { Text("Merge") }
            },
            dismissButton = {
                TextButton(onClick = { mergeConfirmRow = null }) { Text("Cancel") }
            },
        )
    }
}

/**
 * EXP-820: the suggestion chips over an empty new chat. A wrapping row of
 * ordinary action pills — the chip IS the prompt, so nothing truncates it: a
 * long suggestion wraps onto the next line rather than becoming "Do a code
 * review of the op…".
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ChatSuggestionChips(suggestions: List<String>, onPick: (String) -> Unit) {
    FlowRow(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 4.dp)
            .testTag("chat-suggestions"),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        suggestions.forEach { suggestion ->
            GlassPill(
                suggestion,
                size = PillSize.Sm,
                onClick = { onPick(suggestion) },
            )
        }
    }
}
