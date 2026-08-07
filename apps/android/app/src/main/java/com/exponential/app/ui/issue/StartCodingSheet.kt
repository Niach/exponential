package com.exponential.app.ui.issue

import android.content.Context
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.R
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.IconSwatchGrid
import com.exponential.app.ui.components.PriorityIcon
import com.exponential.app.ui.components.pickableIconName
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AccentIndigo
import com.exponential.app.ui.theme.GlassTokens
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
// a searchable single-select action list (the server-appended virtual builtin
// "Create action" pinned first by its flag) plus typed input fields for the
// selected action (text / repo / board / pr / icon), sharing the SAME desktop / agent /
// model / effort / toggle sections; devices there are filtered to the
// `actions` cap (+ `action-inputs` for builtin/inputs-carrying runs).
// Last-used options persist via SharedPreferences; stored values are validated
// against the contract on read so a stale entry can never send a value the
// server rejects.

/** Sentinel-free UI state: an empty effort means "CLI default" (omit --effort). */
private const val CLI_DEFAULT_EFFORT = ""

/** Same convention for codex/pi models: an empty model means "CLI default". */
private const val CLI_DEFAULT_MODEL = ""

private const val DEFAULT_AGENT = "claude"

private const val PREFS_NAME = "coding_start"

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
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE) }

    // Actions-tab data (EXP-257): the team's actions plus the lookup sources
    // the typed input fields render from.
    val actionsState by dataViewModel.actionsState.collectAsStateWithLifecycle()
    val teamRepos by dataViewModel.repos.collectAsStateWithLifecycle()
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val pullRequestOptions by dataViewModel.pullRequestOptions.collectAsStateWithLifecycle()

    // Stored per-mode defaults, read once on composition. ultracode/planMode are
    // the single-issue defaults; a 2+ batch overrides them (see below) without
    // ever writing back over these. model/effort are validated against the
    // STORED agent's option set — an agent switch invalidates them anyway.
    val storedAgent = remember {
        prefs.getString("agent", null)
            ?.takeIf { it in DomainContract.codingAgentValues }
            ?: DEFAULT_AGENT
    }
    val storedModel = remember {
        val valid = modelValuesFor(storedAgent)
        prefs.getString("model", null)
            ?.takeIf {
                if (storedAgent == DEFAULT_AGENT) it in valid
                else it == CLI_DEFAULT_MODEL || it in valid
            }
            ?: defaultModelFor(storedAgent)
    }
    val storedEffort = remember {
        prefs.getString("effort", null)
            ?.takeIf { it == CLI_DEFAULT_EFFORT || it in effortValuesFor(storedAgent) }
            ?: CLI_DEFAULT_EFFORT
    }
    val storedUltracode = remember { prefs.getBoolean("ultracode", false) }
    val storedPlanMode = remember { prefs.getBoolean("planMode", false) }
    val storedSkipPermissions = remember { prefs.getBoolean("skipPermissions", false) }

    // The set of queue-able issue ids (the pool). ALL derived state operates on
    // the intersection of `checked` with this — a preselected id that isn't in
    // the pool (e.g. a repo-less current issue) must never be counted, or it
    // corrupts the 1↔2+ batch seeding, the button, the validation and submit.
    val poolIds = remember(issues) { issues.mapTo(HashSet()) { it.id } }
    val initialInPoolCount = remember { preselectedIds.count { it in poolIds } }

    // Only ONLINE machines can take a start — the relay refuses the rest with
    // device_offline. The Agents tab feeds this sheet the whole EXP-403
    // registry (offline rows included), so the filter lives here, once, for
    // every host rather than at each call site. EXP-409: a machine whose every
    // installed agent is signed out is just as unstartable — it drops out here
    // too, and the My machines list carries the "sign in" reason.
    val startable = remember(devices) { devices.filter { it.online && it.hasRunnableAgent } }

    // The initially selected desktop decides which agents are on offer before
    // any state exists — a stored agent the device can't run falls back to the
    // device's first available agent, with that agent's model/effort defaults.
    val initialAgent = remember {
        val initialDevice = startable.firstOrNull { it.deviceId == preferredDeviceId }
            ?: startable.firstOrNull()
        storedAgent.takeIf { it in availableAgentsFor(initialDevice) }
            ?: availableAgentsFor(initialDevice).firstOrNull() ?: DEFAULT_AGENT
    }

    var agent by remember { mutableStateOf(initialAgent) }
    var model by remember {
        mutableStateOf(if (initialAgent == storedAgent) storedModel else defaultModelFor(initialAgent))
    }
    var effort by remember {
        mutableStateOf(if (initialAgent == storedAgent) storedEffort else CLI_DEFAULT_EFFORT)
    }
    // A run seeded with 2+ in-pool issues starts as a batch (ultracode ON, plan
    // OFF) until the user touches a toggle; ≤1 uses the stored single defaults.
    // Batch seeding is a claude-only concept — other agents have no ultracode.
    var ultracode by remember {
        mutableStateOf(
            if (initialAgent == DEFAULT_AGENT && initialInPoolCount >= 2) true else storedUltracode,
        )
    }
    var planMode by remember {
        mutableStateOf(
            if (initialAgent == DEFAULT_AGENT && initialInPoolCount >= 2) false else storedPlanMode,
        )
    }
    var skipPermissions by remember { mutableStateOf(storedSkipPermissions) }
    // Seed only with in-pool preselected ids — never carry a phantom id.
    var checked by remember { mutableStateOf(preselectedIds intersect poolIds) }
    // Set by any Model/Effort/ultracode/plan interaction: once the user takes
    // control, crossing the 1↔2+ boundary stops auto-seeding ultracode/plan.
    var touchedToggles by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }

    // ── Actions-tab state (EXP-257) ──────────────────────────────────────────
    var subjectTab by remember {
        mutableStateOf(if (preselectedActionId != null) SubjectTab.Actions else SubjectTab.Issues)
    }
    var selectedActionId by remember { mutableStateOf(preselectedActionId) }
    var actionQuery by remember { mutableStateOf("") }
    var inputValues by remember { mutableStateOf<Map<String, String>>(emptyMap()) }

    // Builtin rows pin FIRST by the flag (never by sort order; stable sort
    // keeps the server order otherwise), then the search filter applies.
    val orderedActions = remember(actionsState.actions) {
        actionsState.actions?.sortedByDescending { it.isBuiltin }
    }
    val filteredActions = remember(orderedActions, actionQuery) {
        val q = actionQuery.trim()
        orderedActions?.filter {
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

    // Switching agent invalidates the per-agent model/effort vocabularies:
    // reset both to the new agent's defaults and clamp the claude-only toggles.
    fun selectAgent(next: String) {
        if (next == agent) return
        agent = next
        model = defaultModelFor(next)
        effort = CLI_DEFAULT_EFFORT
        if (next != DEFAULT_AGENT) {
            ultracode = false
            planMode = false
        }
    }

    // The settled device can change without an explicit pick (tab switch or a
    // stricter Actions-tab candidate filter) — clamp the agent to one the new
    // device can actually run.
    LaunchedEffect(device?.deviceId) {
        if (device != null && agent !in availableAgentsFor(device)) {
            selectAgent(availableAgentsFor(device).firstOrNull() ?: DEFAULT_AGENT)
        }
    }

    fun toggleIssue(id: String) {
        // Count only in-pool ids so a lingering phantom (a checked id that fell
        // out of the pool mid-session) can't skew the 1↔2+ crossing.
        val before = checked.count { it in poolIds }
        checked = if (id in checked) checked - id else checked + id
        val after = checked.count { it in poolIds }
        // Batch defaults (ultracode ON, plan OFF) only exist for claude.
        if (!touchedToggles && agent == DEFAULT_AGENT) {
            if (before <= 1 && after >= 2) {
                ultracode = true
                planMode = false
            } else if (before >= 2 && after <= 1) {
                ultracode = storedUltracode
                planMode = storedPlanMode
            }
        }
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
                        // agent/model/effort/skipPermissions persist on every
                        // submit; ultracode/plan only on a claude non-batch
                        // start, so batch seeding (and agent clamping) never
                        // leaks into the stored single-issue defaults.
                        prefs.edit().apply {
                            putString("agent", agent)
                            putString("model", model)
                            putString("effort", effort)
                            putBoolean("skipPermissions", skipPermissions)
                            if (agent == DEFAULT_AGENT && (action != null || ids.size <= 1)) {
                                putBoolean("ultracode", ultracode)
                                putBoolean("planMode", planMode)
                            }
                            apply()
                        }
                        val options = SteerStartOptions(
                            model = model,
                            effort = effort,
                            // ultracode/plan are claude-only; skip-permissions
                            // applies to every guarded agent (i.e. not pi).
                            ultracode = if (agent == DEFAULT_AGENT) ultracode else null,
                            planMode = if (agent == DEFAULT_AGENT) planMode else null,
                            agent = agent,
                            skipPermissions = if (agent == "pi") null else skipPermissions,
                        )
                        if (action != null) {
                            // Only filled values ride, keyed by the def key
                            // (repo/board values are the picked ids).
                            val payload = selectedActionInputs.mapNotNull { def ->
                                inputValues[def.key]?.trim()?.takeIf { it.isNotEmpty() }
                                    ?.let { def.key to it }
                            }.toMap()
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
                            subjectTab == SubjectTab.Actions -> "Run action"
                            checkedCount >= 2 -> "Start coding ($checkedCount issues)"
                            else -> "Start coding"
                        },
                    )
                }
            }

            // ── Subject tabs (EXP-257): Issues | Actions ─────────────────────
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

                    // Typed input fields for the selected action (EXP-257).
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
                        onSelect = {
                            model = it
                            touchedToggles = true
                        },
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
                        onSelect = {
                            effort = it
                            touchedToggles = true
                        },
                    )
                }
                Spacer(Modifier.height(4.dp))

                // ── Toggles ──────────────────────────────────────────────────
                // ONE group: claude gets Ultracode + Plan mode + Skip
                // permissions, codex just Skip permissions, pi nothing at all
                // (it is always unguarded — no row, no notice; EXP-208).
                if (agent != "pi") {
                    OptionGroup {
                        if (agent == DEFAULT_AGENT) {
                            SwitchRow(
                                title = "Ultracode",
                                checked = ultracode,
                                onCheckedChange = {
                                    ultracode = it
                                    touchedToggles = true
                                },
                            )
                            GroupDivider()
                            SwitchRow(
                                title = "Plan mode",
                                checked = planMode,
                                onCheckedChange = {
                                    planMode = it
                                    touchedToggles = true
                                },
                            )
                            GroupDivider()
                        }
                        SwitchRow(
                            title = "Skip permissions",
                            checked = skipPermissions,
                            onCheckedChange = {
                                skipPermissions = it
                                touchedToggles = true
                            },
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
        // Only text remains — unknown types never render (the pane blocks the
        // run and shows the needs-a-newer-app caption instead).
        else -> OutlinedTextField(
            value = value,
            onValueChange = { onValueChange(it.take(DomainContract.actionInputTextMax)) },
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            label = { Text(label) },
            placeholder = def.placeholder?.let { hint -> { Text(hint) } },
        )
    }
}

@Composable
private fun SectionLabel(text: String) {
    // Aligned with the grouped cards' inner content edge (16dp card inset +
    // 16dp row padding), the way iOS section headers line up with their card.
    Text(
        text = text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
    )
}

// iOS-inset-grouped-section analog (EXP-208): a rounded glass container that
// wraps a group of rows, separated inside by [GroupDivider] hairlines.
@Composable
private fun OptionGroup(content: @Composable () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(GlassTokens.RowFill, RoundedCornerShape(12.dp)),
    ) {
        content()
    }
}

/** Hairline between rows in an [OptionGroup] (TeamSettingsScreen's idiom). */
@Composable
private fun GroupDivider() {
    HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
}

// iOS-Form-style picker row: label left, selected value + chevron right; tap
// opens a glass menu of the options. Disabled = dimmed + non-interactive.
@Composable
private fun PickerRow(
    label: String,
    value: String,
    options: List<String>,
    selected: String?,
    optionLabel: (String) -> String,
    onSelect: (String) -> Unit,
    enabled: Boolean = true,
) {
    var expanded by remember { mutableStateOf(false) }
    val contentAlpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary
    Box(Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(enabled = enabled) { expanded = true }
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                label,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = contentAlpha),
                modifier = Modifier.weight(1f),
            )
            Text(
                value,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (enabled) TextEmphasis.Secondary else TextEmphasis.Quaternary,
                ),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Icon(
                ExpIcons.uiChevronDown,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (enabled) TextEmphasis.Tertiary else TextEmphasis.Quaternary,
                ),
            )
        }
        GlassDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            options.forEach { option ->
                GlassMenuItem(
                    text = { Text(optionLabel(option)) },
                    trailingIcon = if (option == selected) {
                        {
                            // Indigo checkmark — GlassSheetRow's selection idiom.
                            Icon(
                                ExpIcons.uiCheck,
                                contentDescription = "Selected",
                                modifier = Modifier.size(16.dp),
                                tint = AccentIndigo,
                            )
                        }
                    } else {
                        null
                    },
                    onClick = {
                        expanded = false
                        onSelect(option)
                    },
                )
            }
        }
    }
}

// One agent tab in the pill strip: brand icon + label; the selected tab is a
// filled pill, unselected tabs are subdued (desktop agent_tabs parity).
@Composable
private fun AgentTab(
    value: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val tint = MaterialTheme.colorScheme.onSurface.copy(
        alpha = if (selected) TextEmphasis.Primary else TextEmphasis.Secondary,
    )
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .glassButton(active = selected)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    ) {
        Icon(
            painterResource(agentIconRes(value)),
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = tint,
        )
        Spacer(Modifier.width(6.dp))
        Text(agentLabel(value), style = MaterialTheme.typography.labelLarge, color = tint)
    }
}

@Composable
private fun SwitchRow(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onCheckedChange)
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

private fun modelValuesFor(agent: String): List<String> = when (agent) {
    "codex" -> DomainContract.codexModelValues
    "pi" -> DomainContract.piModelValues
    else -> DomainContract.codingModelValues
}

private fun effortValuesFor(agent: String): List<String> = when (agent) {
    "codex" -> DomainContract.codexEffortValues
    "pi" -> DomainContract.piThinkingValues
    else -> DomainContract.codingEffortValues
}

/** claude has no CLI-default model entry; codex/pi default to the blank one. */
private fun defaultModelFor(agent: String): String =
    if (agent == DEFAULT_AGENT) DomainContract.codingModelValues.first() else CLI_DEFAULT_MODEL

private fun agentLabel(value: String): String = when (value) {
    "claude" -> "Claude Code"
    "codex" -> "Codex"
    "pi" -> "pi"
    else -> value
}

/** Monochrome brand marks derived from the desktop IDE's SVG icons (EXP-208). */
private fun agentIconRes(value: String): Int = when (value) {
    "codex" -> R.drawable.ic_agent_codex
    "pi" -> R.drawable.ic_agent_pi
    else -> R.drawable.ic_agent_claude
}

private fun modelLabel(value: String): String = when (value) {
    CLI_DEFAULT_MODEL -> "CLI default"
    "gpt-5.6-sol" -> "GPT-5.6 Sol"
    "gpt-5.6-terra" -> "GPT-5.6 Terra"
    "gpt-5.6-luna" -> "GPT-5.6 Luna"
    "grok-4.5" -> "Grok 4.5"
    else -> value.replaceFirstChar { it.uppercaseChar() }
}

private fun effortLabel(value: String): String = when (value) {
    CLI_DEFAULT_EFFORT -> "CLI default"
    "xhigh" -> "XHigh"
    else -> value.replaceFirstChar { it.uppercaseChar() }
}
