package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.ActionTriggerFilters
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.resumeWorktreeFor
import com.exponential.app.domain.triggerDescriptionBlock
import com.exponential.app.domain.triggerEventLabel
import com.exponential.app.domain.triggerWeekdayName
import com.exponential.app.ui.components.AgentTab
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.CLI_DEFAULT_MODEL
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconSwatchGrid
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.SectionLabel
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.defaultModelFor
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelValuesFor
import com.exponential.app.ui.components.pickableIconName
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AccentIndigo
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton

// The unified remote Start-coding sheet (EXP-156) — the Android twin of the
// desktop IDE's ONE Start-coding dialog, restyled to mirror the iOS sheet
// (EXP-208, EXP-211): a full-height sheet inset below the status bar whose
// body scrolls under an iOS-style top bar (Cancel left, Start right, no
// title), the issue picker carded like an iOS Form section (search row +
// hairline-divided rows), grouped picker rows for Desktop / Model / Effort, a
// brand-icon agent pill strip (EXP-201: claude / codex / pi, shown only when
// the chosen desktop offers more than one), the claude-only ultracode switch
// (it IS `--effort ultracode`, so it disables the Effort row) and plan-mode
// switch, and a skip-permissions switch (claude + codex — pi is always
// unguarded, so the row is simply absent). Exactly 1 checked issue launches a
// plain single session; 2+ launch a BATCH session (one agent on one
// `exp/batch-<id8>` branch spanning every issue, all from one repository).
// EXP-257 adds a top-level Issues | Actions subject switch: the Actions tab is
// a searchable single-select action list (the "Fix merge conflicts" builtin
// pinned first by its flag; "Create action" is not offered — EXP-431 gave it
// `createActionMode`, a dedicated presentation of this sheet: "New action"
// title, no subject switch, no picker, just the create builtin's input
// fields) plus typed input fields for the
// selected action (text / repo / board / pr / icon), sharing the SAME desktop / agent /
// model / effort / toggle sections; devices there are filtered to the
// `actions` cap (+ `action-inputs` for builtin/inputs-carrying runs).
// EXP-437: the sheet keeps NO last-used state of its own — the picked machine
// is the single seed source. Every option is pre-filled from the launch
// defaults that machine advertises for the chosen agent (validated against the
// contract, so a machine can never seed a value the server rejects), falling
// back to the static contract defaults when it advertises none.

// Loose batch caps (desktop parity): a hard 30-issue ceiling, and a soft note
// past 6 that a single Claude session across that many issues burns tokens.
private const val MAX_BATCH_ISSUES = 30
private const val LARGE_BATCH_HINT_THRESHOLD = 6

/**
 * One issue the sheet can queue for a run — repositoryId gates same-repo
 * batches; status/priority feed the list-style row visuals (EXP-173).
 * Deliberately no defaults: a producer that forgets them would compile fine
 * and silently render every row as Backlog/no-priority via fromWire's
 * fallback.
 */
data class StartIssueOption(
    val id: String,
    val identifier: String,
    val title: String,
    val repositoryId: String?,
    val status: String?,
    val priority: String?,
)

/** The sheet's top-level subject switch (EXP-257): what a run launches on. */
private enum class SubjectTab { Issues, Actions }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun StartCodingSheet(
    devices: List<SteerDevice>,
    issues: List<StartIssueOption>,
    preselectedIds: Set<String>,
    preferredDeviceId: String? = null,
    // Non-null opens the sheet on the Actions tab with this action selected.
    preselectedActionId: String? = null,
    // EXP-431: present the sheet as the "New action" creation flow — no
    // subject switch, no action picker, just the (preselected) create
    // builtin's input fields. Callers pair it with the create builtin's id.
    createActionMode: Boolean = false,
    // EXP-530: seed input values keyed by def key (the Suggestions tab's
    // "Use suggestion" hands over description + icon; the iOS
    // prefilledInputs pattern).
    prefilledInputs: Map<String, String>? = null,
    // Non-null pre-picks the selected action's `pr` input (EXP-323 — the
    // conflict-recovery entry points hand over the issue their surface acts
    // on; ANY issue linked to the PR resolves, see [optionForIssue]).
    preselectedPrIssueId: String? = null,
    onStart: (SteerDevice, List<String>, SteerStartOptions) -> Unit,
    onRunAction: (SteerDevice, ActionDto, SteerStartOptions, Map<String, String>) -> Unit,
    onDismiss: () -> Unit,
    dataViewModel: StartCodingSheetViewModel = hiltViewModel(),
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    // Actions-tab data (EXP-257): the team's actions plus the lookup sources
    // the typed input fields render from.
    val actionsState by dataViewModel.actionsState.collectAsStateWithLifecycle()
    val teamRepos by dataViewModel.repos.collectAsStateWithLifecycle()
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val pullRequestOptions by dataViewModel.pullRequestOptions.collectAsStateWithLifecycle()
    // EXP-481: the synced worktree inventory + device rows behind the resume
    // offer. Resolved here (not threaded through every host) so all seven
    // hosts get the toggle without new plumbing; hosts whose SteerDevice rows
    // predate the shapes (devices.list decode, no rowId) join through the
    // synced device row by steer deviceId.
    val deviceWorktrees by dataViewModel.deviceWorktrees.collectAsStateWithLifecycle()
    val syncedDeviceRows by dataViewModel.deviceRows.collectAsStateWithLifecycle()

    // The set of queue-able issue ids (the pool). ALL derived state operates on
    // the intersection of `checked` with this — a preselected id that isn't in
    // the pool (e.g. a repo-less current issue) must never be counted, or it
    // corrupts the button, the validation and submit.
    val poolIds = remember(issues) { issues.mapTo(HashSet()) { it.id } }

    // Only ONLINE machines can take a start — the relay refuses the rest with
    // device_offline. The Agents tab feeds this sheet the whole EXP-403
    // registry (offline rows included), so the filter lives here, once, for
    // every host rather than at each call site. EXP-409: a machine whose every
    // installed agent is signed out is just as unstartable — it drops out here
    // too, and the My machines list carries the "sign in" reason.
    val startable = remember(devices) { devices.filter { it.online && it.hasRunnableAgent } }

    // The initially selected desktop decides which agents are on offer before
    // any state exists, and (EXP-437) seeds every option: its configured
    // default agent clamped to what it can run, then that agent's advertised
    // model/effort/toggles — static contract defaults when it advertises none.
    val initialDevice = remember {
        startable.firstOrNull { it.deviceId == preferredDeviceId } ?: startable.firstOrNull()
    }
    val initialAgent = remember { defaultAgentFor(initialDevice) }
    val initialSeed = remember { agentSeed(initialDevice, initialAgent) }

    var agent by remember { mutableStateOf(initialAgent) }
    var model by remember { mutableStateOf(initialSeed.model) }
    var effort by remember { mutableStateOf(initialSeed.effort) }
    // EXP-532: batch runs take the same device-advertised defaults as
    // single-issue runs — no per-mode override on the 1↔2+ crossing anymore.
    var ultracode by remember { mutableStateOf(initialSeed.ultracode) }
    var planMode by remember { mutableStateOf(initialSeed.planMode) }
    var skipPermissions by remember { mutableStateOf(initialSeed.skipPermissions) }
    // Seed only with in-pool preselected ids — never carry a phantom id.
    var checked by remember { mutableStateOf(preselectedIds intersect poolIds) }
    var query by remember { mutableStateOf("") }

    // ── Actions-tab state (EXP-257) ──────────────────────────────────────────
    var subjectTab by remember {
        mutableStateOf(if (preselectedActionId != null) SubjectTab.Actions else SubjectTab.Issues)
    }
    var selectedActionId by remember { mutableStateOf(preselectedActionId) }
    var actionQuery by remember { mutableStateOf("") }
    var inputValues by remember { mutableStateOf<Map<String, String>>(prefilledInputs ?: emptyMap()) }

    // ── Automation draft (EXP-530, create mode only) ─────────────────────────
    // A draft can represent states a valid trigger cannot (a kind picked but
    // no capable device online) — the submit path converts via
    // [automationDraftToTrigger] (incomplete → null = "no automation").
    var automation by remember { mutableStateOf(AutomationDraft()) }
    val automationFilterLabels by dataViewModel.labelOptions.collectAsStateWithLifecycle()
    val automationFilterStatuses by dataViewModel.statusOptions.collectAsStateWithLifecycle()

    // Builtin rows pin FIRST by the flag (never by sort order; stable sort
    // keeps the server order otherwise), then the search filter applies. The
    // create builtin stays in the POOL (createActionMode preselects it) but
    // never renders as a picker row (EXP-431).
    val orderedActions = remember(actionsState.actions) {
        actionsState.actions?.sortedByDescending { it.isBuiltin }
    }
    val filteredActions = remember(orderedActions, actionQuery) {
        val q = actionQuery.trim()
        orderedActions
            ?.filter { it.id != DomainContract.builtinCreateActionId }
            ?.filter {
                q.isEmpty() ||
                    it.name.contains(q, ignoreCase = true) ||
                    it.description?.contains(q, ignoreCase = true) == true
            }
    }
    val selectedAction = orderedActions?.firstOrNull { it.id == selectedActionId }
    val selectedActionInputs = selectedAction?.inputs.orEmpty()
    // Builtin or inputs-carrying runs additionally need the `action-inputs`
    // device cap; an input type this build doesn't know blocks the run (the
    // desktop would render the prompt without it otherwise).
    val needsInputCap = selectedAction != null &&
        (selectedAction.isBuiltin || selectedActionInputs.isNotEmpty())
    val hasUnknownInputType =
        selectedActionInputs.any { it.type !in DomainContract.actionInputTypeValues }
    // The "Fix merge conflicts" builtin needs its own cap on top (EXP-259).
    val needsFixConflictsCap = selectedAction?.id == DomainContract.builtinFixConflictsId

    // Pre-pick the target PR (EXP-323). The seed is normalised through
    // `optionForIssue` because the caller's issue is rarely the option's
    // representative, and it runs ONCE: a manual re-pick must stick, and the
    // options flow re-emits on every sync tick.
    var seededPr by remember { mutableStateOf(preselectedPrIssueId == null) }
    LaunchedEffect(preselectedPrIssueId, pullRequestOptions, selectedActionId) {
        if (seededPr || preselectedPrIssueId == null) return@LaunchedEffect
        val key = selectedActionInputs.firstOrNull { it.type == "pr" }?.key
            ?: return@LaunchedEffect
        val option = pullRequestOptions.optionForIssue(preselectedPrIssueId)
            ?: return@LaunchedEffect
        inputValues = inputValues + (key to option.issueId)
        seededPr = true
    }

    // Pre-fill `repo` inputs with the action's bound repository (EXP-349) —
    // a picker reading "None" while the run targets the bound repo anyway
    // looked misconfigured. The id latch keeps a manual re-pick (including
    // clearing to "None") from being re-seeded on sync re-emits, and the
    // effect covers both the tap path (onSelect clears the values first) and
    // a preselected action, which never goes through onSelect.
    var seededRepoActionId by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(selectedAction) {
        val action = selectedAction ?: return@LaunchedEffect
        if (seededRepoActionId == action.id) return@LaunchedEffect
        seededRepoActionId = action.id
        val repoId = action.repositoryId ?: return@LaunchedEffect
        val seeds = action.inputs.orEmpty()
            .filter { it.type == "repo" && inputValues[it.key] == null }
            .associate { it.key to repoId }
        if (seeds.isNotEmpty()) inputValues = seeds + inputValues
    }

    var deviceId by remember {
        mutableStateOf(
            startable.firstOrNull { it.deviceId == preferredDeviceId }?.deviceId
                ?: startable.firstOrNull()?.deviceId,
        )
    }
    // Per-tab device candidates: issues take any desktop; actions need the
    // `actions` cap (+ `action-inputs` when the selection demands it, +
    // `fix-conflicts` for that builtin). A deviceId outside the current
    // candidates re-settles on the first one without clobbering the stored
    // choice for the other tab.
    val deviceCandidates = if (subjectTab == SubjectTab.Actions) {
        startable.filter {
            it.canRunActions &&
                (!needsInputCap || it.canRunActionInputs) &&
                (!needsFixConflictsCap || it.canFixConflicts)
        }
    } else {
        startable
    }
    val device = deviceCandidates.firstOrNull { it.deviceId == deviceId }
        ?: deviceCandidates.firstOrNull()
    val availableAgents = availableAgentsFor(device)

    // ── Remote resume (EXP-481) ─────────────────────────────────────────────
    // Offerable iff exactly ONE issue is checked, the machine honors the flag
    // (cap-gated — an old build would silently start fresh), and a synced
    // worktree row matches this issue + agent. The state starts ON and a
    // manual flip persists for the sheet's lifetime; a stale row degrades
    // device-side (fresh session seeded with a resume prompt).
    var resume by remember { mutableStateOf(true) }

    // Every option follows the agent: the per-agent model/effort vocabularies
    // differ, and so do the settled machine's advertised defaults (EXP-437).
    // The seed is already capability-clamped (no ultracode/plan off claude, no
    // skip-permissions on pi), so this needs no clamping of its own.
    fun applyAgentSeed(next: String) {
        agent = next
        val seed = agentSeed(device, next)
        model = seed.model
        effort = seed.effort
        ultracode = seed.ultracode
        planMode = seed.planMode
        skipPermissions = seed.skipPermissions
    }

    fun selectAgent(next: String) {
        if (next == agent) return
        applyAgentSeed(next)
    }

    // The settled device can change without an explicit pick (tab switch or a
    // stricter Actions-tab candidate filter). A real change re-seeds agent and
    // options from the new machine (EXP-437); the latch keeps a re-poll that
    // re-emits the SAME device from stomping the user's edits, and a machine
    // advertising nothing seeds the static defaults, which for the agent means
    // the pre-existing clamp to something it can run.
    var seededDeviceId by remember { mutableStateOf(initialDevice?.deviceId) }
    LaunchedEffect(device?.deviceId) {
        val settled = device ?: return@LaunchedEffect
        if (seededDeviceId == settled.deviceId) {
            if (agent !in availableAgentsFor(settled)) {
                selectAgent(availableAgentsFor(settled).firstOrNull() ?: DEFAULT_AGENT)
            }
            return@LaunchedEffect
        }
        seededDeviceId = settled.deviceId
        applyAgentSeed(defaultAgentFor(settled))
    }

    fun toggleIssue(id: String) {
        checked = if (id in checked) checked - id else checked + id
    }

    // Preselected rows pinned first (in candidate order), then the
    // search-filtered remainder (cap 50 rendered). The pin set is the
    // OPEN-time preselection snapshot, deliberately NOT the live `checked`
    // set: re-sorting on every toggle teleported the tapped row out from
    // under the finger, which read as "issues are not selectable" (EXP-241).
    // Rows stay put; only the check indicator flips. Submitting still sends
    // the checked ids in candidate order via `checkedInOrder`.
    val pinnedIds = remember { preselectedIds intersect poolIds }
    val checkedInOrder = remember(issues, checked) { issues.filter { it.id in checked } }
    val pinnedRows = remember(issues, pinnedIds, query) {
        val q = query.trim()
        issues.filter {
            it.id in pinnedIds &&
                (
                    q.isEmpty() ||
                        it.identifier.contains(q, ignoreCase = true) ||
                        it.title.contains(q, ignoreCase = true)
                    )
        }
    }
    val otherRows = remember(issues, pinnedIds, query) {
        val q = query.trim()
        issues.asSequence()
            .filter { it.id !in pinnedIds }
            .filter {
                q.isEmpty() ||
                    it.identifier.contains(q, ignoreCase = true) ||
                    it.title.contains(q, ignoreCase = true)
            }
            .take(50)
            .toList()
    }

    val checkedCount = checkedInOrder.size
    val repoIds = remember(checkedInOrder) { checkedInOrder.map { it.repositoryId }.toSet() }
    // The settled device's synced ROW id: stamped on DB-derived rows, joined
    // through the synced devices by steer deviceId for poll-derived ones.
    val deviceRowId = device?.rowId
        ?: syncedDeviceRows.firstOrNull { it.deviceId == device?.deviceId }?.id
    val resumeCandidate = if (
        subjectTab == SubjectTab.Issues && checkedCount == 1 && device?.canResume == true
    ) {
        resumeWorktreeFor(
            deviceWorktrees,
            deviceRowId,
            checkedInOrder.firstOrNull()?.identifier,
            agent,
        )
    } else {
        null
    }
    val resumeOn = resume && resumeCandidate != null
    val multiRepo = checkedCount >= 1 && repoIds.size > 1
    val tooMany = checkedCount > MAX_BATCH_ISSUES
    val canStart = device != null && checkedCount in 1..MAX_BATCH_ISSUES && !multiRepo
    // Actions-tab launch gate: a settled capable desktop, a selection, no
    // unknown input types, and every required input filled.
    val requiredInputsFilled = selectedActionInputs
        .filter { it.required }
        .all { !inputValues[it.key].isNullOrBlank() }
    val canRunAction = device != null && selectedAction != null &&
        !hasUnknownInputType && requiredInputsFilled

    // Full-height sheet (EXP-208), iOS-chrome parity (EXP-211): no drag handle
    // (the sheet is inset below the status bar instead of colliding with it),
    // no sheet title, and an iOS-nav-bar-style pinned top row — Cancel on the
    // left, the Start button on the right — above the scrolling body.
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = null,
        modifier = Modifier.statusBarsPadding().testTag("start-coding-sheet"),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = onDismiss) { Text("Cancel") }
                Spacer(Modifier.weight(1f))
                if (createActionMode) {
                    // The sheet is deliberately title-less everywhere else
                    // (EXP-211 chrome) — create mode is the one host that
                    // needs to say what it is.
                    Text("New action", style = MaterialTheme.typography.titleSmall)
                    Spacer(Modifier.weight(1f))
                }
                Button(
                    onClick = {
                        val target = device ?: return@Button
                        val ids = checkedInOrder.map { it.id }
                        val action =
                            if (subjectTab == SubjectTab.Actions) selectedAction else null
                        if (subjectTab == SubjectTab.Actions) {
                            if (action == null) return@Button
                        } else if (ids.isEmpty()) {
                            return@Button
                        }
                        val options = SteerStartOptions(
                            model = model,
                            effort = effort,
                            // ultracode/plan are claude-only; skip-permissions
                            // applies to every guarded agent (i.e. not pi).
                            ultracode = if (agent == DEFAULT_AGENT) ultracode else null,
                            // A resume never re-enters plan mode (EXP-202) —
                            // clamped like the desktop dialog.
                            planMode = when {
                                resumeOn -> false
                                supportsPlanMode(agent) -> planMode
                                else -> null
                            },
                            agent = agent,
                            skipPermissions = if (agent == "pi") null else skipPermissions,
                            // Single-issue starts only — the batch/action
                            // inputs never carry the field.
                            resume = if (resumeOn && ids.size == 1 && action == null) true else null,
                        )
                        if (action != null) {
                            // Only filled values ride, keyed by the def key
                            // (repo/board values are the picked ids).
                            var payload = selectedActionInputs.mapNotNull { def ->
                                inputValues[def.key]?.trim()?.takeIf { it.isNotEmpty() }
                                    ?.let { def.key to it }
                            }.toMap()
                            // EXP-530: a configured automation rides the
                            // description as a machine-readable block the
                            // creator agent copies verbatim into
                            // exponential_actions_create's `trigger` field.
                            // EXP-574: it binds to the desktop that runs the
                            // creation — no second device picker.
                            if (createActionMode) {
                                automationDraftToTrigger(
                                    automation.copy(deviceId = target.deviceId),
                                )?.let { trigger ->
                                    payload["description"]?.let { description ->
                                        payload = payload +
                                            ("description" to description + triggerDescriptionBlock(trigger))
                                    }
                                }
                            }
                            onRunAction(target, action, options, payload)
                        } else {
                            onStart(target, ids, options)
                        }
                        onDismiss()
                    },
                    enabled = if (subjectTab == SubjectTab.Actions) canRunAction else canStart,
                ) {
                    Text(
                        when {
                            // The create builtin's run IS creation (desktop
                            // footer parity, EXP-431).
                            subjectTab == SubjectTab.Actions &&
                                selectedAction?.id == DomainContract.builtinCreateActionId -> "Create"
                            subjectTab == SubjectTab.Actions -> "Run action"
                            checkedCount >= 2 -> "Start coding ($checkedCount issues)"
                            else -> "Start coding"
                        },
                    )
                }
            }

            // ── Subject tabs (EXP-257): Issues | Actions ─────────────────────
            // Create mode is single-purpose (EXP-431) — the subject switch
            // would only lead out of the creation flow.
            if (!createActionMode) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 4.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
                ) {
                    SubjectTabPill(
                        label = "Issues",
                        selected = subjectTab == SubjectTab.Issues,
                        onClick = { subjectTab = SubjectTab.Issues },
                    )
                    SubjectTabPill(
                        label = "Actions",
                        selected = subjectTab == SubjectTab.Actions,
                        onClick = { subjectTab = SubjectTab.Actions },
                    )
                }
            }

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
                    .verticalScroll(rememberScrollState()),
            ) {
                if (subjectTab == SubjectTab.Issues) {
                    // ── Issues ───────────────────────────────────────────────────
                    SectionLabel("Issues")
                    // ONE grouped card for search + rows (EXP-211 — iOS Form
                    // parity): the search field is the first row of the glass
                    // container and hairlines separate the issue rows, instead of
                    // bare edge-to-edge rows on the sheet background.
                    OptionGroup {
                        TextField(
                            value = query,
                            onValueChange = { query = it },
                            modifier = Modifier.fillMaxWidth(),
                            placeholder = {
                                Text(
                                    "Search issues",
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                            },
                            leadingIcon = {
                                Icon(
                                    ExpIcons.navSearch,
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                            },
                            // iOS parity: a clear (×) affordance while searching.
                            trailingIcon = if (query.isEmpty()) {
                                null
                            } else {
                                {
                                    IconButton(onClick = { query = "" }) {
                                        Icon(
                                            ExpIcons.uiClose,
                                            contentDescription = "Clear search",
                                            modifier = Modifier.size(16.dp),
                                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                        )
                                    }
                                }
                            },
                            singleLine = true,
                            colors = TextFieldDefaults.colors(
                                focusedContainerColor = Color.Transparent,
                                unfocusedContainerColor = Color.Transparent,
                                disabledContainerColor = Color.Transparent,
                                focusedIndicatorColor = Color.Transparent,
                                unfocusedIndicatorColor = Color.Transparent,
                                disabledIndicatorColor = Color.Transparent,
                            ),
                        )
                        GroupDivider()
                        if (pinnedRows.isEmpty() && otherRows.isEmpty()) {
                            Text(
                                if (issues.isEmpty()) "No eligible issues" else "No matching issues",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                            )
                        } else {
                            // The issues scroll INSIDE this bounded area (EXP-173)
                            // so the Model/Effort/switch controls stay reachable.
                            // The heightIn(max) cap makes the lazy child's
                            // constraints finite, which is what legalizes nesting
                            // it in the outer scroll Column (~5.5 rows — the half
                            // row is the scroll affordance).
                            LazyColumn(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .heightIn(max = 264.dp),
                            ) {
                                itemsIndexed(
                                    pinnedRows + otherRows,
                                    key = { _, option -> option.id },
                                ) { index, option ->
                                    Column(modifier = Modifier.animateItem()) {
                                        if (index > 0) GroupDivider()
                                        IssueCheckRow(
                                            option = option,
                                            checked = option.id in checked,
                                            onToggle = { toggleIssue(option.id) },
                                        )
                                    }
                                }
                            }
                        }
                    }

                    // Validation captions (blocking) + the large-batch soft note.
                    val validationCaption = when {
                        multiRepo -> "Pick issues from a single repository per run."
                        tooMany -> "At most $MAX_BATCH_ISSUES issues per run. Split the batch."
                        else -> null
                    }
                    if (validationCaption != null) {
                        Spacer(Modifier.height(4.dp))
                        Text(
                            validationCaption,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(horizontal = 32.dp),
                        )
                    } else if (checkedCount > LARGE_BATCH_HINT_THRESHOLD) {
                        Spacer(Modifier.height(4.dp))
                        Text(
                            "Large batches are token-expensive.",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            modifier = Modifier.padding(horizontal = 32.dp),
                        )
                    }
                    Spacer(Modifier.height(4.dp))
                } else if (createActionMode) {
                    // ── EXP-431 create mode: no picker or intro copy — the
                    // create builtin's input fields below ARE the form
                    // (Description leads with the locked placeholder).
                    if (orderedActions == null) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            modifier = Modifier.padding(horizontal = 32.dp, vertical = 12.dp),
                        ) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(14.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Text(
                                "Loading actions…",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    }
                } else {
                    // ── Actions ──────────────────────────────────────────────
                    SectionLabel("Actions")
                    // Same grouped-card layout as the issue picker: search row
                    // + hairline-divided SINGLE-select action rows (builtin
                    // pinned first by its flag).
                    OptionGroup {
                        TextField(
                            value = actionQuery,
                            onValueChange = { actionQuery = it },
                            modifier = Modifier.fillMaxWidth(),
                            placeholder = {
                                Text(
                                    "Search actions",
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                            },
                            leadingIcon = {
                                Icon(
                                    ExpIcons.navSearch,
                                    contentDescription = null,
                                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                )
                            },
                            // iOS parity: a clear (×) affordance while searching.
                            trailingIcon = if (actionQuery.isEmpty()) {
                                null
                            } else {
                                {
                                    IconButton(onClick = { actionQuery = "" }) {
                                        Icon(
                                            ExpIcons.uiClose,
                                            contentDescription = "Clear search",
                                            modifier = Modifier.size(16.dp),
                                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                        )
                                    }
                                }
                            },
                            singleLine = true,
                            colors = TextFieldDefaults.colors(
                                focusedContainerColor = Color.Transparent,
                                unfocusedContainerColor = Color.Transparent,
                                disabledContainerColor = Color.Transparent,
                                focusedIndicatorColor = Color.Transparent,
                                unfocusedIndicatorColor = Color.Transparent,
                                disabledIndicatorColor = Color.Transparent,
                            ),
                        )
                        GroupDivider()
                        val actionRows = filteredActions
                        when {
                            actionRows == null && actionsState.error != null -> Text(
                                actionsState.error ?: "",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                            )
                            actionRows == null -> Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                            ) {
                                CircularProgressIndicator(
                                    modifier = Modifier.size(14.dp),
                                    strokeWidth = 2.dp,
                                    color = MaterialTheme.colorScheme.onSurface,
                                )
                                Text(
                                    "Loading actions…",
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                )
                            }
                            actionRows.isEmpty() -> Text(
                                if (orderedActions.isNullOrEmpty()) "No actions yet" else "No matching actions",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                            )
                            // Bounded like the issue list (EXP-173) so the
                            // Desktop/Model/Effort controls stay reachable.
                            else -> LazyColumn(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .heightIn(max = 264.dp),
                            ) {
                                itemsIndexed(
                                    actionRows,
                                    key = { _, action -> action.id },
                                ) { index, action ->
                                    Column {
                                        if (index > 0) GroupDivider()
                                        ActionSelectRow(
                                            action = action,
                                            selected = action.id == selectedActionId,
                                            onSelect = {
                                                if (selectedActionId != action.id) {
                                                    selectedActionId = action.id
                                                    inputValues = emptyMap()
                                                }
                                            },
                                        )
                                    }
                                }
                            }
                        }
                    }
                }

                // Typed input fields for the selected action (EXP-257) — in
                // create mode (EXP-431) they are the whole form.
                if (subjectTab == SubjectTab.Actions) {
                    if (selectedAction != null && selectedActionInputs.isNotEmpty()) {
                        Spacer(Modifier.height(8.dp))
                        SectionLabel("Inputs")
                        if (hasUnknownInputType) {
                            Text(
                                "This action needs a newer app version.",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.error,
                                modifier = Modifier.padding(horizontal = 32.dp),
                            )
                        } else {
                            selectedActionInputs.forEachIndexed { index, def ->
                                if (index > 0) Spacer(Modifier.height(8.dp))
                                ActionInputField(
                                    def = def,
                                    value = inputValues[def.key] ?: "",
                                    repos = teamRepos,
                                    boards = boardOptions,
                                    pullRequests = pullRequestOptions,
                                    onValueChange = { next ->
                                        inputValues = inputValues + (def.key to next)
                                    },
                                )
                            }
                        }
                    }
                    Spacer(Modifier.height(4.dp))

                    // ── Automation (EXP-530, create mode only) ───────────────
                    if (createActionMode) {
                        AutomationSection(
                            draft = automation,
                            boards = boardOptions,
                            labels = automationFilterLabels,
                            statuses = automationFilterStatuses,
                            onChange = { automation = it },
                        )
                    }
                }

                // ── Desktop ──────────────────────────────────────────────────
                if (subjectTab == SubjectTab.Actions && deviceCandidates.isEmpty()) {
                    // No desktop can take this run — none advertises `actions`,
                    // or the builtin/inputs/fix-conflicts run needs a newer
                    // desktop app.
                    OptionGroup {
                        Text(
                            if (needsFixConflictsCap) {
                                "No desktop can fix merge conflicts yet. Update the Exponential desktop app."
                            } else {
                                "No actions-capable desktop online. Open or update the Exponential desktop app."
                            },
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                        )
                    }
                    Spacer(Modifier.height(4.dp))
                } else if (deviceCandidates.size > 1) {
                    OptionGroup {
                        PickerRow(
                            label = "Desktop",
                            value = device?.let(::deviceLabel) ?: "",
                            options = deviceCandidates.map { it.deviceId },
                            selected = device?.deviceId,
                            optionLabel = { id ->
                                deviceCandidates.firstOrNull { it.deviceId == id }
                                    ?.let(::deviceLabel) ?: id
                            },
                            onSelect = { id ->
                                deviceId = id
                                // The new desktop may not run the current agent
                                // — fall back to its first available one.
                                val candidate = deviceCandidates.firstOrNull { it.deviceId == id }
                                val available = availableAgentsFor(candidate)
                                if (agent !in available) {
                                    selectAgent(available.firstOrNull() ?: DEFAULT_AGENT)
                                }
                            },
                        )
                    }
                    Spacer(Modifier.height(4.dp))
                }

                // Agent pill strip — hidden when the chosen desktop offers just
                // one; brand icons match the desktop IDE dialog's agent tabs.
                if (availableAgents.size > 1) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp, vertical = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
                    ) {
                        availableAgents.forEach { value ->
                            AgentTab(
                                value = value,
                                selected = agent == value,
                                onClick = { selectAgent(value) },
                            )
                        }
                    }
                    Spacer(Modifier.height(4.dp))
                }

                // ── Model / Effort ───────────────────────────────────────────
                OptionGroup {
                    val modelOptions = when (agent) {
                        "codex" -> listOf(CLI_DEFAULT_MODEL) + DomainContract.codexModelValues
                        "pi" -> listOf(CLI_DEFAULT_MODEL) + DomainContract.piModelValues
                        else -> DomainContract.codingModelValues
                    }
                    PickerRow(
                        label = "Model",
                        value = modelLabel(model),
                        options = modelOptions,
                        selected = model,
                        optionLabel = ::modelLabel,
                        onSelect = { model = it },
                    )
                    GroupDivider()
                    PickerRow(
                        label = when (agent) {
                            "codex" -> "Reasoning"
                            "pi" -> "Thinking"
                            else -> "Effort"
                        },
                        value = effortLabel(effort),
                        options = listOf(CLI_DEFAULT_EFFORT) + effortValuesFor(agent),
                        selected = effort,
                        optionLabel = ::effortLabel,
                        // Ultracode IS `--effort ultracode` — it owns the row.
                        enabled = !ultracode,
                        onSelect = { effort = it },
                    )
                }
                Spacer(Modifier.height(4.dp))

                // ── Resume (EXP-481) ─────────────────────────────────────────
                // Rendered only when a synced worktree row matches the single
                // checked issue + agent on a resume-capable machine; the
                // caption names the worktree so "why is this offered" is
                // answerable at a glance (desktop dialog copy).
                val resumeWorktree = resumeCandidate
                if (resumeWorktree != null) {
                    OptionGroup {
                        SwitchRow(
                            title = "Resume previous session",
                            checked = resume,
                            onCheckedChange = { resume = it },
                        )
                    }
                    Text(
                        "A worktree for ${checkedInOrder.firstOrNull()?.identifier} " +
                            "already exists (${resumeWorktree.branch}).",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
                    )
                    Spacer(Modifier.height(4.dp))
                }

                // ── Toggles ──────────────────────────────────────────────────
                // ONE group: claude gets Ultracode + Plan mode + Skip
                // permissions, codex just Skip permissions, pi just Plan mode
                // (EXP-441 — pi stays otherwise unguarded; EXP-208). pi's ONLY
                // toggle is plan mode, which resume hides — skip the empty
                // group shell then.
                if (agent != "pi" || !resumeOn) OptionGroup {
                    if (agent == DEFAULT_AGENT) {
                        SwitchRow(
                            title = "Ultracode",
                            checked = ultracode,
                            onCheckedChange = { ultracode = it },
                        )
                        GroupDivider()
                    }
                    // Hidden entirely while resuming — a resume never
                    // re-enters plan mode (EXP-202, desktop parity).
                    if (supportsPlanMode(agent) && !resumeOn) {
                        SwitchRow(
                            title = "Plan mode",
                            checked = planMode,
                            onCheckedChange = { planMode = it },
                        )
                        if (agent != "pi") {
                            GroupDivider()
                        }
                    }
                    if (agent != "pi") {
                        SwitchRow(
                            title = "Skip permissions",
                            checked = skipPermissions,
                            onCheckedChange = { skipPermissions = it },
                        )
                    }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

// One checkable issue, styled like the regular issue-list row (EXP-173):
// circle toggle icon (RepoRow's selection affordance — EXP-208), priority
// icon, mono identifier column, status icon, title.
@Composable
private fun IssueCheckRow(
    option: StartIssueOption,
    checked: Boolean,
    onToggle: () -> Unit,
) {
    val status = IssueStatus.fromWire(option.status)
    val priority = IssuePriority.fromWire(option.priority)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle)
            // Selected rows tint so the state is unmissable (EXP-241) —
            // the icon swap alone was easy to overlook.
            .background(
                if (checked) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else Color.Transparent,
            )
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (checked) ExpIcons.uiSelected else ExpIcons.uiUnselected,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = if (checked) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            },
        )
        Spacer(Modifier.width(10.dp))
        PriorityIcon(priority, size = 16.dp)
        Spacer(Modifier.width(10.dp))
        Text(
            option.identifier,
            style = MaterialTheme.typography.labelMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.widthIn(min = 60.dp),
        )
        Spacer(Modifier.width(10.dp))
        StatusIcon(status, size = 16.dp)
        Spacer(Modifier.width(10.dp))
        Text(
            option.title,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

// One subject tab in the top strip — the AgentTab pill styling without a
// brand icon (EXP-257: the Issues | Actions subject switch).
@Composable
private fun SubjectTabPill(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Text(
        label,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(
            alpha = if (selected) TextEmphasis.Primary else TextEmphasis.Secondary,
        ),
        modifier = Modifier
            .glassButton(active = selected)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    )
}

// One selectable action (single-select, IssueCheckRow's affordances): the
// circle/check indicator, a create (+) glyph for the builtin row / a bolt for
// regular ones, name (+ a small repo indicator when the action clones a
// repository), and the optional description.
@Composable
private fun ActionSelectRow(
    action: ActionDto,
    selected: Boolean,
    onSelect: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onSelect)
            .background(
                if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else Color.Transparent,
            )
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (selected) ExpIcons.uiSelected else ExpIcons.uiUnselected,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = if (selected) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            },
        )
        Spacer(Modifier.width(10.dp))
        Icon(
            if (action.isBuiltin) ExpIcons.actionCreate else ExpIcons.actionDefault,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    action.name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (action.repositoryId != null) {
                    Spacer(Modifier.width(6.dp))
                    Icon(
                        ExpIcons.actionRepository,
                        contentDescription = "Runs in a repository",
                        modifier = Modifier.size(12.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            }
            val description = action.description
            if (!description.isNullOrBlank()) {
                Text(
                    description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

// One typed input field (EXP-257): text → outlined field with the def's
// placeholder; repo/board/pr → a grouped picker row over the team registry /
// synced boards / the team's open pull requests; icon (EXP-273) → the curated
// swatch grid shared with the create-board form, whose value is a registry
// NAME rather than an id. The stored value is the raw text, the picked id or
// the glyph name; a cleared optional picker stores "" which the submit path
// drops.
@Composable
private fun ActionInputField(
    def: ActionInputDto,
    value: String,
    repos: List<TeamRepo>,
    boards: List<StartBoardOption>,
    pullRequests: List<StartPullRequestOption>,
    onValueChange: (String) -> Unit,
) {
    val label = if (def.required) def.label else "${def.label} (optional)"
    when (def.type) {
        "repo" -> OptionGroup {
            PickerRow(
                label = label,
                value = when {
                    value.isEmpty() && def.required -> "Select"
                    value.isEmpty() -> "None"
                    else -> repos.firstOrNull { it.id == value }?.fullName ?: value
                },
                options = (if (def.required) emptyList() else listOf("")) + repos.map { it.id },
                selected = value.takeIf { it.isNotEmpty() || !def.required },
                optionLabel = { id ->
                    if (id.isEmpty()) "None" else repos.firstOrNull { it.id == id }?.fullName ?: id
                },
                onSelect = onValueChange,
            )
        }
        "board" -> OptionGroup {
            PickerRow(
                label = label,
                value = when {
                    value.isEmpty() && def.required -> "Select"
                    value.isEmpty() -> "None"
                    else -> boards.firstOrNull { it.id == value }?.name ?: value
                },
                options = (if (def.required) emptyList() else listOf("")) + boards.map { it.id },
                selected = value.takeIf { it.isNotEmpty() || !def.required },
                optionLabel = { id ->
                    if (id.isEmpty()) "None" else boards.firstOrNull { it.id == id }?.name ?: id
                },
                onSelect = onValueChange,
            )
        }
        // EXP-259: the value is the REPRESENTATIVE issue id of an open
        // issue-linked PR (batch PRs dedupe by prUrl, so one row can list
        // several identifiers).
        "pr" -> OptionGroup {
            if (pullRequests.isEmpty()) {
                PickerRow(
                    label = label,
                    value = "No open pull requests",
                    options = emptyList(),
                    selected = null,
                    optionLabel = { it },
                    onSelect = {},
                )
            } else {
                PickerRow(
                    label = label,
                    value = when {
                        value.isEmpty() && def.required -> "Select"
                        value.isEmpty() -> "None"
                        else -> pullRequests.firstOrNull { it.issueId == value }?.label ?: value
                    },
                    options = (if (def.required) emptyList() else listOf("")) +
                        pullRequests.map { it.issueId },
                    selected = value.takeIf { it.isNotEmpty() || !def.required },
                    optionLabel = { id ->
                        if (id.isEmpty()) {
                            "None"
                        } else {
                            pullRequests.firstOrNull { it.issueId == id }?.label ?: id
                        }
                    },
                    onSelect = onValueChange,
                )
            }
        }
        // EXP-273: the curated icon set. Unlike the pickers above the value is
        // a glyph NAME, so there is nothing team-scoped to look up — the grid
        // is the same one the create-board form draws. Optional inputs start
        // with nothing highlighted and keep a "No icon" reset.
        "icon" -> OptionGroup {
            val picked = pickableIconName(value)
            Column(
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        label,
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                    )
                    if (picked != null && !def.required) {
                        TextButton(onClick = { onValueChange("") }) { Text("No icon") }
                    } else {
                        Text(
                            picked ?: "None",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Secondary,
                            ),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
                IconSwatchGrid(
                    selected = picked,
                    onSelect = onValueChange,
                    accentColor = AccentIndigo,
                )
            }
        }
        // EXP-530: multi-line prompt text — same cap as `text`, taller field.
        "textarea" -> GlassTextField(
            value = value,
            onValueChange = { onValueChange(it.take(DomainContract.actionInputTextMax)) },
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            placeholder = def.placeholder ?: label,
            minLines = 3,
        )
        // Only text remains — unknown types never render (the pane blocks the
        // run and shows the needs-a-newer-app caption instead).
        else -> GlassTextField(
            value = value,
            onValueChange = { onValueChange(it.take(DomainContract.actionInputTextMax)) },
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            placeholder = def.placeholder ?: label,
            singleLine = true,
        )
    }
}

/**
 * How a machine reads in the Desktop picker. EXP-432: a teammate's shared
 * server carries its owner ("buildbox — Danny"), so two similarly named boxes
 * stay tellable apart and the run's host is never a surprise.
 */
private fun deviceLabel(device: SteerDevice): String {
    val base = device.deviceLabel.ifBlank { device.deviceId }
    return device.owner?.let { "$base — ${it.name}" } ?: base
}

// The agents a desktop can launch, in contract order. No device settled yet
// means the pre-EXP-201 default (claude); a device answers for itself, and
// EXP-409 lets that answer be EMPTY — a machine whose agents are all signed
// out runs nothing, so it never becomes a candidate in the first place.
private fun availableAgentsFor(device: SteerDevice?): List<String> =
    device?.runnableAgents ?: listOf(DEFAULT_AGENT)

/** Every launch option for one agent, ready to drop into the sheet's state. */
private data class AgentSeed(
    val model: String,
    val effort: String,
    val ultracode: Boolean,
    val planMode: Boolean,
    val skipPermissions: Boolean,
)

/**
 * Which agent a machine starts on (EXP-437): the one it has configured as its
 * default, clamped to what it can actually run. A machine that advertises no
 * default (or an unrunnable one) falls back to claude, then to whatever it
 * runs first — the pre-EXP-437 behavior.
 */
private fun defaultAgentFor(device: SteerDevice?): String {
    val available = availableAgentsFor(device)
    return device?.launchDefaults?.defaultAgent?.takeIf { it in available }
        ?: DEFAULT_AGENT.takeIf { it in available }
        ?: available.firstOrNull()
        ?: DEFAULT_AGENT
}

// ── Automation (EXP-530) ─────────────────────────────────────────────────────

/**
 * The create sheet's automation editor state (the web `AutomationDraft`
 * mirror). A draft can represent states a valid [ActionTrigger] cannot (a
 * kind picked but no capable device to bind) — [automationDraftToTrigger]
 * converts at submit, and incomplete reads as null ("no automation"). The
 * filter picks are SINGLE-select with an "Any" default — the mobile
 * simplification of the web's multi-selects; the wire lists carry one-or-none
 * entries from here.
 */
internal data class AutomationDraft(
    /** `none` | `schedule` | `event`. */
    val kind: String = "none",
    /** Steer deviceId of the bound machine; null = nothing bindable yet. */
    val deviceId: String? = null,
    val enabled: Boolean = true,
    val interval: String = "daily",
    /** `HH:MM` wall-clock string, straight off the time field. */
    val time: String = "09:00",
    val weekday: Int = 1,
    val dayOfMonth: Int = 1,
    val event: String = "created",
    val boardId: String = "",
    val labelId: String = "",
    val priority: String = "",
    val toStatusId: String = "",
)

/** `HH:MM` → minutes past midnight; malformed/out-of-range reads 540 (09:00,
 * the web `timeToMinute` fallback). */
internal fun automationTimeToMinute(time: String): Int {
    val match = Regex("^(\\d{1,2}):(\\d{2})$").find(time.trim()) ?: return 540
    val minute = match.groupValues[1].toInt() * 60 + match.groupValues[2].toInt()
    return if (minute in 0..1439) minute else 540
}

/**
 * The draft's trigger, or null when it is "none" OR incomplete (no device
 * bound). Filters irrelevant to the picked event are dropped, matching the
 * server's strict write schema.
 */
internal fun automationDraftToTrigger(draft: AutomationDraft): ActionTrigger? {
    val deviceId = draft.deviceId ?: return null
    return when (draft.kind) {
        "schedule" -> ActionTrigger.Schedule(
            deviceId = deviceId,
            enabled = draft.enabled,
            interval = draft.interval,
            minuteOfDay = automationTimeToMinute(draft.time),
            weekday = draft.weekday.takeIf { draft.interval == "weekly" },
            dayOfMonth = draft.dayOfMonth.takeIf { draft.interval == "monthly" },
        )
        "event" -> ActionTrigger.Event(
            deviceId = deviceId,
            enabled = draft.enabled,
            event = draft.event,
            filters = ActionTriggerFilters(
                boardIds = listOfNotNull(draft.boardId.takeIf { it.isNotEmpty() }),
                labelIds = listOfNotNull(
                    draft.labelId.takeIf { it.isNotEmpty() && draft.event == "label_added" },
                ),
                priorities = listOfNotNull(
                    draft.priority.takeIf {
                        it.isNotEmpty() &&
                            (draft.event == "created" || draft.event == "priority_changed")
                    },
                ),
                toStatusIds = listOfNotNull(
                    draft.toStatusId.takeIf { it.isNotEmpty() && draft.event == "status_changed" },
                ),
            ),
        )
        else -> null
    }
}

private fun intervalLabel(interval: String): String = when (interval) {
    "weekly" -> "Week"
    "monthly" -> "Month"
    else -> "Day"
}

// The create sheet's Automation section (EXP-530): segmented None · Schedule
// · On event (the shared GlassSegmentedControl — web/iOS segmented parity,
// EXP-574), the schedule pane (interval + contextual weekday/day pickers +
// HH:MM time) and the event pane (event picker + contextual filter pickers
// from the synced tables). The automation binds to the desktop that runs the
// creation (no second device picker) and starts enabled — owners flip it
// later on the Automations tab.
@Composable
private fun AutomationSection(
    draft: AutomationDraft,
    boards: List<StartBoardOption>,
    labels: List<StartFilterOption>,
    statuses: List<StartFilterOption>,
    onChange: (AutomationDraft) -> Unit,
) {
    Spacer(Modifier.height(8.dp))
    SectionLabel("Automation")
    GlassSegmentedControl(
        options = listOf("none", "schedule", "event"),
        selected = draft.kind,
        label = {
            when (it) {
                "schedule" -> "Schedule"
                "event" -> "On event"
                else -> "None"
            }
        },
        onSelect = { onChange(draft.copy(kind = it)) },
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
    )

    if (draft.kind == "schedule") {
        Spacer(Modifier.height(4.dp))
        OptionGroup {
            PickerRow(
                label = "Every",
                value = intervalLabel(draft.interval),
                options = DomainContract.actionScheduleIntervalValues,
                selected = draft.interval,
                optionLabel = ::intervalLabel,
                onSelect = { onChange(draft.copy(interval = it)) },
            )
            if (draft.interval == "weekly") {
                GroupDivider()
                PickerRow(
                    label = "Weekday",
                    value = triggerWeekdayName(draft.weekday),
                    options = (1..7).map(Int::toString),
                    selected = draft.weekday.toString(),
                    optionLabel = { triggerWeekdayName(it.toInt()) },
                    onSelect = { onChange(draft.copy(weekday = it.toInt())) },
                )
            }
            if (draft.interval == "monthly") {
                GroupDivider()
                PickerRow(
                    label = "Day of month",
                    value = "Day ${draft.dayOfMonth}",
                    options = (1..28).map(Int::toString),
                    selected = draft.dayOfMonth.toString(),
                    optionLabel = { "Day $it" },
                    onSelect = { onChange(draft.copy(dayOfMonth = it.toInt())) },
                )
            }
            GroupDivider()
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Time",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                GlassTextField(
                    value = draft.time,
                    onValueChange = { onChange(draft.copy(time = it.take(5))) },
                    modifier = Modifier.width(104.dp),
                    placeholder = "09:00",
                    singleLine = true,
                )
            }
        }
    } else if (draft.kind == "event") {
        Spacer(Modifier.height(4.dp))
        OptionGroup {
            PickerRow(
                label = "When",
                value = triggerEventLabel(draft.event),
                options = DomainContract.actionTriggerEventValues,
                selected = draft.event,
                optionLabel = ::triggerEventLabel,
                onSelect = { onChange(draft.copy(event = it)) },
            )
            GroupDivider()
            // Board filter applies to every event; the rest are contextual
            // (labels for label_added, priorities for created/priority
            // changes, target status for status_changed).
            AutomationFilterPicker(
                label = "Board",
                anyLabel = "Any board",
                options = boards.map { StartFilterOption(it.id, it.name) },
                selected = draft.boardId,
                onSelect = { onChange(draft.copy(boardId = it)) },
            )
            if (draft.event == "label_added") {
                GroupDivider()
                AutomationFilterPicker(
                    label = "Label",
                    anyLabel = "Any label",
                    options = labels,
                    selected = draft.labelId,
                    onSelect = { onChange(draft.copy(labelId = it)) },
                )
            }
            if (draft.event == "created" || draft.event == "priority_changed") {
                GroupDivider()
                AutomationFilterPicker(
                    label = "Priority",
                    anyLabel = "Any priority",
                    options = DomainContract.issuePriorityValues.map {
                        StartFilterOption(it, IssuePriority.fromWire(it).label)
                    },
                    selected = draft.priority,
                    onSelect = { onChange(draft.copy(priority = it)) },
                )
            }
            if (draft.event == "status_changed") {
                GroupDivider()
                AutomationFilterPicker(
                    label = "To status",
                    anyLabel = "Any status",
                    options = statuses,
                    selected = draft.toStatusId,
                    onSelect = { onChange(draft.copy(toStatusId = it)) },
                )
            }
        }
    }
    Spacer(Modifier.height(4.dp))
}

// One "Any"-defaulted single-select filter row: the empty value is the
// no-filter state and stays re-pickable as the first option.
@Composable
private fun AutomationFilterPicker(
    label: String,
    anyLabel: String,
    options: List<StartFilterOption>,
    selected: String,
    onSelect: (String) -> Unit,
) {
    PickerRow(
        label = label,
        value = options.firstOrNull { it.id == selected }?.name ?: anyLabel,
        options = listOf("") + options.map { it.id },
        selected = selected,
        optionLabel = { id ->
            if (id.isEmpty()) anyLabel else options.firstOrNull { it.id == id }?.name ?: id
        },
        onSelect = onSelect,
    )
}

/**
 * [agent]'s launch options as [device] has them configured (EXP-437), falling
 * back per field to the static contract defaults — an older desktop advertises
 * nothing, and the sheet must still open on something startable. Advertised
 * values are validated against this agent's vocabularies (an empty string is
 * the explicit "CLI default", which claude has no entry for) and the toggles
 * are capability-clamped, so a machine can never seed a combination the server
 * would reject.
 */
private fun agentSeed(device: SteerDevice?, agent: String): AgentSeed {
    val defaults = device?.launchDefaults?.agents?.get(agent)
        ?: return AgentSeed(defaultModelFor(agent), CLI_DEFAULT_EFFORT, false, false, false)
    val models = modelValuesFor(agent)
    return AgentSeed(
        model = defaults.model
            ?.takeIf {
                if (agent == DEFAULT_AGENT) it in models
                else it == CLI_DEFAULT_MODEL || it in models
            }
            ?: defaultModelFor(agent),
        effort = defaults.effort
            ?.takeIf { it == CLI_DEFAULT_EFFORT || it in effortValuesFor(agent) }
            ?: CLI_DEFAULT_EFFORT,
        ultracode = defaults.ultracode && agent == DEFAULT_AGENT,
        planMode = defaults.planMode && supportsPlanMode(agent),
        skipPermissions = defaults.skipPermissions && agent != "pi",
    )
}
