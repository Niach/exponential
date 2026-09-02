package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
import com.exponential.app.data.api.builtinChatAction
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.resumeWorktreeFor
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.LaunchOptionsSection
import com.exponential.app.ui.components.LaunchOptionsVariant
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.agentSeed
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.components.defaultAgentFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

// The unified remote Start-coding sheet (EXP-156) — the Android twin of the
// desktop IDE's ONE Start-coding dialog, restyled to mirror the iOS sheet
// (EXP-208, EXP-211): a full-height sheet inset below the status bar whose
// body scrolls under an iOS-style top bar (Cancel left, Start right, no
// title), the issue picker carded like an iOS Form section (search row +
// hairline-divided rows), grouped picker rows for Device / Model / Effort, a
// brand-icon agent capsule (EXP-201: claude / codex / pi, shown only when the
// chosen desktop offers more than one), the claude-only ultracode switch
// (it IS `--effort ultracode`, so it disables the Effort row) and plan-mode
// switch. Exactly 1 checked issue launches a
// plain single session; 2+ launch a BATCH session (one agent on one
// `exp/batch-<id8>` branch spanning every issue, all from one repository).
// EXP-257 adds a top-level subject switch, since EXP-615 one segmented capsule
// of Issues | Actions | Chat: the Actions tab is a searchable single-select
// action list (the "Fix merge conflicts" builtin pinned first by its flag;
// "Create action" is not offered — creation lives in its own
// [com.exponential.app.ui.actions.CreateActionSheet]) plus typed input fields
// for the selected action (text / repo / board / pr / icon), and the Chat tab
// (EXP-615) is a free prompt on a repository, riding the hidden
// [builtinChatAction] over the same action rails. All three share the SAME
// device / agent / model / effort / toggle block (LaunchOptionsSection), and
// since EXP-672 the SAME device pool: online with a runnable agent, no
// per-subject capability filters.
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

/** The sheet's top-level subject switch (EXP-257, Chat since EXP-615): what a
 * run launches on. */
enum class SubjectTab { Issues, Actions, Chat }

/**
 * The styleguide capture suite addresses the subject tabs by testTag —
 * "Actions" and "Chat" also read as ordinary nodes elsewhere in the sheet, so
 * the label alone is not a handle (EXP-642). Byte-identical to the iOS
 * accessibility identifiers on `StartCodingSheet.SubjectTab`.
 */
internal fun subjectTabTestTag(tab: SubjectTab): String = when (tab) {
    SubjectTab.Issues -> "start-coding-tab-issues"
    SubjectTab.Actions -> "start-coding-tab-actions"
    SubjectTab.Chat -> "start-coding-tab-chat"
}

@Composable
fun StartCodingSheet(
    devices: List<SteerDevice>,
    issues: List<StartIssueOption>,
    preselectedIds: Set<String>,
    preferredDeviceId: String? = null,
    // Non-null opens the sheet on the Actions tab with this action selected.
    preselectedActionId: String? = null,
    // EXP-615: which subject the sheet opens on. Null keeps the derivation
    // from [preselectedActionId] (an action means the Actions tab).
    initialTab: SubjectTab? = null,
    // Non-null pre-picks the selected action's `pr` input (EXP-323 — the
    // conflict-recovery entry points hand over the issue their surface acts
    // on; ANY issue linked to the PR resolves, see [optionForIssue]).
    preselectedPrIssueId: String? = null,
    onStart: (SteerDevice, List<String>, SteerStartOptions) -> Unit,
    onRunAction: (SteerDevice, ActionDto, SteerStartOptions, Map<String, String>) -> Unit,
    onDismiss: () -> Unit,
    dataViewModel: StartCodingSheetViewModel = hiltViewModel(),
) {

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
        startable.firstOrNull { it.deviceId == preferredDeviceId }
            // EXP-622: the caller's default machine, when it is still startable.
            ?: startable.firstOrNull { it.isDefault }
            ?: startable.firstOrNull()
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
    // Seed only with in-pool preselected ids — never carry a phantom id.
    var checked by remember { mutableStateOf(preselectedIds intersect poolIds) }
    var query by remember { mutableStateOf("") }

    // ── Actions-tab state (EXP-257) ──────────────────────────────────────────
    var subjectTab by remember {
        mutableStateOf(
            initialTab
                ?: if (preselectedActionId != null) SubjectTab.Actions else SubjectTab.Issues,
        )
    }
    var selectedActionId by remember { mutableStateOf(preselectedActionId) }
    var actionQuery by remember { mutableStateOf("") }
    var inputValues by remember { mutableStateOf<Map<String, String>>(emptyMap()) }

    // ── Chat-tab state (EXP-615) ─────────────────────────────────────────────
    // The chat builtin is HIDDEN: no list carries it, so the tab constructs
    // the row itself and fills its two inputs from these fields.
    val selectedTeamId by dataViewModel.teamId.collectAsStateWithLifecycle()
    var chatPrompt by remember { mutableStateOf("") }
    var chatRepoId by remember { mutableStateOf("") }

    // A team with exactly one repository never asks which one (web parity).
    LaunchedEffect(teamRepos) {
        if (chatRepoId.isEmpty() && teamRepos.size == 1) chatRepoId = teamRepos.first().id
    }

    // Builtin rows pin FIRST by the flag (never by sort order; stable sort
    // keeps the server order otherwise), then the search filter applies. The
    // create builtin never renders as a picker row (it lives in its own
    // create sheet), and neither does the hidden chat row.
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
    // An input type this build doesn't know blocks the run (the desktop would
    // render the prompt without it otherwise).
    val hasUnknownInputType =
        selectedActionInputs.any { it.type !in DomainContract.actionInputTypeValues }

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
                ?: startable.firstOrNull { it.isDefault }?.deviceId
                ?: startable.firstOrNull()?.deviceId,
        )
    }
    // EXP-672: every subject shares ONE device pool — online with a runnable
    // agent. The per-subject capability filters (`actions`, `action-inputs`,
    // `fix-conflicts`, `chat`) are gone: every build above the version floor
    // advertises them all, and the server refuses only on the agent.
    val device = startable.firstOrNull { it.deviceId == deviceId }
        ?: startable.firstOrNull { it.isDefault }
        ?: startable.firstOrNull()
    val availableAgents = availableAgentsFor(device)

    // ── Remote resume (EXP-481) ─────────────────────────────────────────────
    // Offerable iff exactly ONE issue is checked and a synced worktree row
    // matches this issue + agent — EXP-672 dropped the `resume` cap check: a
    // machine that reports a worktree for the issue honors the flag by
    // construction (the inventory and the launcher ship together). The state
    // starts ON and a manual flip persists for the sheet's lifetime; a stale
    // row degrades device-side (fresh session seeded with a resume prompt).
    var resume by remember { mutableStateOf(true) }

    // Every option follows the agent: the per-agent model/effort vocabularies
    // differ, and so do the settled machine's advertised defaults (EXP-437).
    // The seed is already capability-clamped (no ultracode/plan off claude),
    // so this needs no clamping of its own.
    fun applyAgentSeed(next: String) {
        agent = next
        val seed = agentSeed(device, next)
        model = seed.model
        effort = seed.effort
        ultracode = seed.ultracode
        planMode = seed.planMode
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
        subjectTab == SubjectTab.Issues && checkedCount == 1 && device != null
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
    // Actions-tab launch gate: a settled desktop, a selection, no unknown
    // input types, and every required input filled.
    val requiredInputsFilled = selectedActionInputs
        .filter { it.required }
        .all { !inputValues[it.key].isNullOrBlank() }
    val canRunAction = device != null && selectedAction != null &&
        !hasUnknownInputType && requiredInputsFilled
    // Chat needs both of its required inputs and a team to hang the hidden
    // builtin row on (every builtin start carries its teamId).
    val chatAction = selectedTeamId?.let { builtinChatAction(it) }
    val canChat = device != null && chatAction != null &&
        chatPrompt.isNotBlank() && chatRepoId.isNotEmpty()

    // Full-height sheet (EXP-208), one-shell chrome (EXP-687): a drag handle,
    // no title, and ONE pinned bottom button — the Cancel pill and the
    // top-right Start button are gone, the swipe/back gesture cancels.
    GlassSheet(
        title = null,
        onDismiss = onDismiss,
        modifier = Modifier.testTag("start-coding-sheet"),
        height = SheetHeight.Full,
        primaryAction = SheetPrimaryAction(
            label = when {
                subjectTab == SubjectTab.Actions -> "Run action"
                subjectTab == SubjectTab.Chat -> "Start chat"
                checkedCount >= 2 -> "Start coding ($checkedCount issues)"
                else -> "Start coding"
            },
            enabled = when (subjectTab) {
                SubjectTab.Actions -> canRunAction
                SubjectTab.Chat -> canChat
                SubjectTab.Issues -> canStart
            },
            onClick = start@{
                val target = device ?: return@start
                val ids = checkedInOrder.map { it.id }
                val action = when (subjectTab) {
                    SubjectTab.Actions -> selectedAction ?: return@start
                    SubjectTab.Chat -> chatAction ?: return@start
                    SubjectTab.Issues -> null
                }
                if (action == null && ids.isEmpty()) return@start
                val options = SteerStartOptions(
                    model = model,
                    effort = effort,
                    // ultracode/plan are claude-only.
                    ultracode = if (agent == DEFAULT_AGENT) ultracode else null,
                    // A resume never re-enters plan mode (EXP-202) —
                    // clamped like the desktop dialog.
                    planMode = when {
                        resumeOn -> false
                        supportsPlanMode(agent) -> planMode
                        else -> null
                    },
                    agent = agent,
                    // Single-issue starts only — the batch/action
                    // inputs never carry the field.
                    resume = if (resumeOn && ids.size == 1 && action == null) true else null,
                )
                if (action != null) {
                    val payload = if (subjectTab == SubjectTab.Chat) {
                        mapOf(
                            "prompt" to chatPrompt.trim(),
                            "repo" to chatRepoId,
                        )
                    } else {
                        // Only filled values ride, keyed by the def key
                        // (repo/board values are the picked ids).
                        selectedActionInputs.mapNotNull { def ->
                            inputValues[def.key]?.trim()?.takeIf { it.isNotEmpty() }
                                ?.let { def.key to it }
                        }.toMap()
                    }
                    onRunAction(target, action, options, payload)
                } else {
                    onStart(target, ids, options)
                }
                onDismiss()
            },
        ),
    ) {
        // ── Subject tabs (EXP-257): Issues | Actions | Chat ──────────────
        // ONE segmented capsule (EXP-615, web/iOS parity) — the loose
        // pills read as filters rather than as a subject switch.
        GlassSegmentedControl(
            options = listOf(SubjectTab.Issues, SubjectTab.Actions, SubjectTab.Chat),
            selected = subjectTab,
            label = {
                when (it) {
                    SubjectTab.Issues -> "Issues"
                    SubjectTab.Actions -> "Actions"
                    SubjectTab.Chat -> "Chat"
                }
            },
            onSelect = { subjectTab = it },
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            testTag = ::subjectTabTestTag,
        )

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            if (subjectTab == SubjectTab.Issues) {
                // ── Issues ───────────────────────────────────────────────
                SectionHeader("Issues", modifier = Modifier.padding(horizontal = 12.dp))
                // ONE grouped card for search + rows (EXP-211 — iOS Form
                // parity): the search field is the first row of the glass
                // container and hairlines separate the issue rows, instead of
                // bare edge-to-edge rows on the sheet background.
                OptionGroup {
                    // EXP-698: the shared field, chrome-less inside the
                    // group (the group owns the fill and the hairlines).
                    GlassTextField(
                        value = query,
                        onValueChange = { query = it },
                        modifier = Modifier.fillMaxWidth(),
                        placeholder = "Search issues",
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
                        bordered = false,
                    )
                    GroupDivider()
                    if (pinnedRows.isEmpty() && otherRows.isEmpty()) {
                        Text(
                            // One wording per state across the clients
                            // (EXP-615, web launch-dialog reference).
                            if (query.isBlank()) {
                                "No codeable issues in repo-backed boards."
                            } else {
                                "No issues match \"$query\""
                            },
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
            } else if (subjectTab == SubjectTab.Chat) {
                // ── Chat (EXP-615) ───────────────────────────────────────
                // A free prompt on one repository's trunk clone — no issue,
                // no branch, no worktree. The two fields ARE the hidden
                // builtin's two inputs, labelled exactly as it declares
                // them.
                SectionHeader("Prompt", modifier = Modifier.padding(horizontal = 12.dp))
                // EXP-698: inside the grouped card like every other field on
                // this sheet, instead of a second chromed box beside them.
                OptionGroup {
                    GlassTextField(
                        value = chatPrompt,
                        onValueChange = {
                            chatPrompt = it.take(DomainContract.actionInputTextMax)
                        },
                        modifier = Modifier.fillMaxWidth(),
                        placeholder = "What should the agent do?",
                        minLines = 4,
                        bordered = false,
                    )
                }
                Spacer(Modifier.height(8.dp))
                OptionGroup {
                    PickerRow(
                        label = "Repository",
                        value = teamRepos.firstOrNull { it.id == chatRepoId }?.fullName
                            ?: "Select",
                        options = teamRepos.map { it.id },
                        selected = chatRepoId.takeIf { it.isNotEmpty() },
                        optionLabel = { id ->
                            teamRepos.firstOrNull { it.id == id }?.fullName ?: id
                        },
                        onSelect = { chatRepoId = it },
                    )
                }
                if (teamRepos.isEmpty()) {
                    Spacer(Modifier.height(4.dp))
                    Text(
                        "Connect a repository to this team to chat.",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        modifier = Modifier.padding(horizontal = 32.dp),
                    )
                }
                Spacer(Modifier.height(4.dp))
            } else {
                // ── Actions ──────────────────────────────────────────────
                SectionHeader("Actions", modifier = Modifier.padding(horizontal = 12.dp))
                // Same grouped-card layout as the issue picker: search row
                // + hairline-divided SINGLE-select action rows (builtin
                // pinned first by its flag).
                OptionGroup {
                    // EXP-698: the shared field, chrome-less inside the
                    // group (the group owns the fill and the hairlines).
                    GlassTextField(
                        value = actionQuery,
                        onValueChange = { actionQuery = it },
                        modifier = Modifier.fillMaxWidth(),
                        placeholder = "Search actions",
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
                        bordered = false,
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
                            if (actionQuery.isBlank()) {
                                "No actions yet."
                            } else {
                                "No actions match \"$actionQuery\""
                            },
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

            // Typed input fields for the selected action (EXP-257).
            if (subjectTab == SubjectTab.Actions) {
                // EXP-583: no "Inputs" heading — the fields speak for
                // themselves and the create flow reads as one form.
                if (selectedAction != null && selectedActionInputs.isNotEmpty()) {
                    Spacer(Modifier.height(8.dp))
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
            }

            // ── Device / agent / model / effort / toggles ─────────────────
            // ONE shared block for every tab (EXP-615) — the per-tab
            // difference is only which machines qualify and what to say
            // when none does (web launch-dialog wording).
            LaunchOptionsSection(
                variant = LaunchOptionsVariant.Launch,
                devices = startable,
                device = device,
                onDeviceChange = { id ->
                    deviceId = id
                    // The new desktop may not run the current agent —
                    // fall back to its first available one.
                    val candidate = startable.firstOrNull { it.deviceId == id }
                    val available = availableAgentsFor(candidate)
                    if (agent !in available) {
                        selectAgent(available.firstOrNull() ?: DEFAULT_AGENT)
                    }
                },
                agent = agent,
                availableAgents = availableAgents,
                onAgentChange = ::selectAgent,
                model = model,
                onModelChange = { model = it },
                effort = effort,
                onEffortChange = { effort = it },
                // EXP-672: the capability wordings are gone — with the cap
                // filters removed an empty pool only ever means "no machine
                // online", so "update the desktop app" named the wrong problem.
                noDeviceNote = when (subjectTab) {
                    SubjectTab.Issues ->
                        "No desktop online. Open the Exponential desktop app to start coding."
                    SubjectTab.Actions, SubjectTab.Chat ->
                        "No desktop online. Open the Exponential desktop app to start a run."
                },
                ultracode = ultracode,
                onUltracodeChange = { ultracode = it },
                planMode = planMode,
                onPlanModeChange = { planMode = it },
                planModeHidden = resumeOn,
                // ── Resume (EXP-481) ─────────────────────────────────────
                // Offered only when a synced worktree row matches the
                // single checked issue + agent on the settled machine; the
                // caption names the worktree so "why is this
                // offered" is answerable at a glance (desktop copy).
                // EXP-694: bare rows — the section renders them inside the
                // agent card, so this must not open a group of its own.
                resumeSlot = resumeCandidate?.let { worktree ->
                    {
                        SwitchRow(
                            title = "Resume previous session",
                            checked = resume,
                            onCheckedChange = { resume = it },
                        )
                        Text(
                            "A worktree for ${checkedInOrder.firstOrNull()?.identifier} " +
                                "already exists (${worktree.branch}).",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = TextEmphasis.Tertiary,
                            ),
                            modifier = Modifier.padding(
                                start = 16.dp,
                                end = 16.dp,
                                bottom = 8.dp,
                            ),
                        )
                    }
                },
            )
            Spacer(Modifier.height(24.dp))
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
        // a glyph NAME, so there is nothing team-scoped to look up — the picker
        // is the same one the create-board form draws (EXP-575). Optional
        // inputs start unset and keep a "No icon" reset inside the sheet.
        "icon" -> OptionGroup {
            Row(
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    label,
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
                IconPicker(
                    selected = value,
                    onSelect = onValueChange,
                    allowsNone = !def.required,
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
