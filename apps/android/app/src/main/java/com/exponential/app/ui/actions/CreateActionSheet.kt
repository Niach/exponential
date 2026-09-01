package com.exponential.app.ui.actions

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.SteerStartOptions
import com.exponential.app.data.api.builtinCreateAction
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.formatAutomationBlock
import com.exponential.app.domain.triggerSummary
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.LaunchOptionsSection
import com.exponential.app.ui.components.LaunchOptionsVariant
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.agentSeed
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.components.defaultAgentFor
import com.exponential.app.ui.components.deviceOptionLabel
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.StartCodingSheetViewModel
import com.exponential.app.ui.theme.TextEmphasis

// The "New action" sheet (EXP-615, split out of the Start-coding sheet's old
// create mode): ONE form for authoring an action — icon + name, what it should
// do, the repository it clones, an always-visible Automation row, and the same
// device/agent/model/effort block every launch surface renders. Submitting
// RUNS the "Create action" builtin on the chosen machine: the agent writes the
// action for the team (and, when an automation is configured, copies the
// trailing note verbatim into `exponential_automations_create`).

@Composable
fun CreateActionSheet(
    teamId: String,
    devices: List<SteerDevice>,
    onRunAction: (SteerDevice, ActionDto, SteerStartOptions, Map<String, String>) -> Unit,
    onDismiss: () -> Unit,
    // EXP-530: a used suggestion's description + icon, keyed by input def key.
    prefilledInputs: Map<String, String>? = null,
    // EXP-583: the trigger an "Action + automation" suggestion proposes — it
    // seeds the Automation row, which is editable and clearable like any other.
    suggestionAutomation: AutomationTrigger? = null,
    dataViewModel: StartCodingSheetViewModel = hiltViewModel(),
) {
    val teamRepos by dataViewModel.repos.collectAsStateWithLifecycle()
    val boardOptions by dataViewModel.boardOptions.collectAsStateWithLifecycle()
    val labelOptions by dataViewModel.labelOptions.collectAsStateWithLifecycle()
    val statusOptions by dataViewModel.statusOptions.collectAsStateWithLifecycle()
    val automationDevices by dataViewModel.automationDevices.collectAsStateWithLifecycle()

    val createAction = remember(teamId) { builtinCreateAction(teamId) }

    // ── The form ─────────────────────────────────────────────────────────────
    var name by remember { mutableStateOf(prefilledInputs?.get("name").orEmpty()) }
    var description by remember { mutableStateOf(prefilledInputs?.get("description").orEmpty()) }
    var repoId by remember { mutableStateOf("") }
    var icon by remember { mutableStateOf(prefilledInputs?.get("icon").orEmpty()) }

    // ── The automation (EXP-583) ─────────────────────────────────────────────
    // Always offered now: the row reads "No automation" until one is set, and
    // the detail sheet's leading "None" segment clears it again. The machine
    // and the agent pins are the AUTOMATION's, independent of the desktop
    // running the creator run.
    var automation by remember {
        mutableStateOf(
            suggestionAutomation?.let { automationDraftFor(it) }
                ?: AutomationDraft(kind = AUTOMATION_KIND_NONE),
        )
    }
    var automationOpen by remember { mutableStateOf(false) }

    // Pre-pick the first automation-capable machine once the rows sync; a
    // manual re-pick sticks (the draft already carries a deviceId then).
    LaunchedEffect(automationDevices) {
        if (automation.deviceId != null) return@LaunchedEffect
        // EXP-622: the caller's default machine, else the first candidate.
        (automationDevices.firstOrNull { it.isDefault } ?: automationDevices.firstOrNull())
            ?.let { automation = automation.copy(deviceId = it.deviceId) }
    }

    // ── The creator run's machine + options (EXP-437 seeding) ────────────────
    // EXP-672: online with a runnable agent is the whole rule — every build
    // above the version floor runs the builtin.
    val candidates = remember(devices) {
        devices.filter { it.online && it.hasRunnableAgent }
    }
    val initialDevice = remember {
        // EXP-622: the caller's default machine, else the first candidate.
        candidates.firstOrNull { it.isDefault } ?: candidates.firstOrNull()
    }
    val initialAgent = remember { defaultAgentFor(initialDevice) }
    val initialSeed = remember { agentSeed(initialDevice, initialAgent) }

    var deviceId by remember { mutableStateOf(initialDevice?.deviceId) }
    var agent by remember { mutableStateOf(initialAgent) }
    var model by remember { mutableStateOf(initialSeed.model) }
    var effort by remember { mutableStateOf(initialSeed.effort) }
    var ultracode by remember { mutableStateOf(initialSeed.ultracode) }
    var planMode by remember { mutableStateOf(initialSeed.planMode) }

    val device = candidates.firstOrNull { it.deviceId == deviceId }
        ?: candidates.firstOrNull { it.isDefault }
        ?: candidates.firstOrNull()
    val availableAgents = availableAgentsFor(device)

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

    // A machine settling late (or a stricter candidate filter) re-seeds the
    // options once; the latch keeps a re-emit from stomping manual edits.
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

    val canCreate = device != null && description.isNotBlank()

    GlassSheet(
        title = "New action",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("create-action-sheet"),
        height = SheetHeight.Full,
        primaryAction = SheetPrimaryAction(
            label = "Create",
            enabled = canCreate,
            icon = ExpIcons.actionCreate,
            onClick = create@{
                val target = device ?: return@create
                val options = SteerStartOptions(
                    model = model,
                    effort = effort,
                    ultracode = if (agent == DEFAULT_AGENT) ultracode else null,
                    planMode = if (supportsPlanMode(agent)) planMode else null,
                    agent = agent,
                )
                // EXP-583: the configured automation rides the description as
                // a machine-readable note the creator agent copies verbatim
                // into exponential_automations_create.
                val automationDeviceId = automation.deviceId
                val trigger = automationDraftToTrigger(automation)
                val body = if (trigger != null && automationDeviceId != null) {
                    description + formatAutomationBlock(
                        trigger,
                        deviceId = automationDeviceId,
                        agent = automation.agent,
                        model = automation.model,
                        effort = automation.effort,
                    )
                } else {
                    description
                }
                val inputs = buildMap {
                    put("description", body)
                    if (name.isNotBlank()) put("name", name.trim())
                    if (repoId.isNotEmpty()) put("repo", repoId)
                    if (icon.isNotEmpty()) put("icon", icon)
                }
                onRunAction(target, createAction, options, inputs)
                onDismiss()
            },
        ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            // Icon + name on one row (web/desktop/iOS parity), then the
            // description that the creator agent actually works from.
            OptionGroup {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    IconPicker(selected = icon, onSelect = { icon = it }, allowsNone = true)
                    Spacer(Modifier.width(12.dp))
                    GlassTextField(
                        value = name,
                        onValueChange = {
                            name = it.take(DomainContract.actionInputTextMax)
                        },
                        modifier = Modifier.weight(1f),
                        placeholder = "Name (optional)",
                        singleLine = true,
                    )
                }
            }
            Spacer(Modifier.height(8.dp))

            // EXP-694 (S7): the brief sits INSIDE the card stack like the edit
            // sheet's description and prompt — the field's own fill/hairline
            // would double the group's chrome, so it drops both and the
            // placeholder carries the title.
            OptionGroup {
                GlassTextField(
                    value = description,
                    onValueChange = {
                        description = it.take(DomainContract.actionInputTextMax)
                    },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = "What should this action do?",
                    minLines = 4,
                    bordered = false,
                )
            }
            Spacer(Modifier.height(8.dp))

            OptionGroup {
                PickerRow(
                    label = "Repository",
                    value = when {
                        repoId.isEmpty() -> "None"
                        else -> teamRepos.firstOrNull { it.id == repoId }?.fullName ?: repoId
                    },
                    options = listOf("") + teamRepos.map { it.id },
                    selected = repoId,
                    optionLabel = { id ->
                        if (id.isEmpty()) {
                            "None"
                        } else {
                            teamRepos.firstOrNull { it.id == id }?.fullName ?: id
                        }
                    },
                    onSelect = { repoId = it },
                )
                GroupDivider()
                // The automation summary row — always here, "No automation"
                // until one is configured (web parity).
                AutomationSummaryRow(
                    draft = automation,
                    devices = automationDevices,
                    onClick = { automationOpen = true },
                )
            }
            Spacer(Modifier.height(8.dp))

            LaunchOptionsSection(
                variant = LaunchOptionsVariant.Launch,
                devices = candidates,
                device = device,
                onDeviceChange = { id ->
                    deviceId = id
                    val candidate = candidates.firstOrNull { it.deviceId == id }
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
                noDeviceNote = "No desktop online. Open the Exponential desktop app " +
                    "to start a run.",
                ultracode = ultracode,
                onUltracodeChange = { ultracode = it },
                planMode = planMode,
                onPlanModeChange = { planMode = it },
            )
            Spacer(Modifier.height(24.dp))
        }
    }

    // The automation detail, nested in its own glass sheet (EXP-607 pattern) —
    // the trigger's leading "None" segment IS the clear affordance.
    if (automationOpen) {
        GlassSheet(
            title = "Automation",
            onDismiss = { automationOpen = false },
            height = SheetHeight.Full,
            primaryAction = SheetPrimaryAction(label = "Done", onClick = { automationOpen = false }),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState()),
            ) {
                AutomationTriggerFields(
                    draft = automation,
                    boards = boardOptions,
                    labels = labelOptions,
                    statuses = statusOptions,
                    onChange = { automation = it },
                    allowNone = true,
                )
                if (automation.kind != AUTOMATION_KIND_NONE) {
                    Spacer(Modifier.height(8.dp))
                    AutomationBindingFields(
                        draft = automation,
                        devices = automationDevices,
                        onChange = { automation = it },
                    )
                }
                Spacer(Modifier.height(8.dp))
            }
        }
    }
}

/**
 * The always-visible automation row: the configured trigger's sentence plus
 * the machine it would run on, or "No automation" when there is none.
 */
@Composable
private fun AutomationSummaryRow(
    draft: AutomationDraft,
    devices: List<SteerDevice>,
    onClick: () -> Unit,
) {
    val trigger = automationDraftToTrigger(draft)
    val boundDevice = devices.firstOrNull { it.deviceId == draft.deviceId }
    val summary = when {
        trigger == null -> "No automation"
        boundDevice == null -> triggerSummary(trigger)
        else -> "${triggerSummary(trigger)} · ${deviceOptionLabel(boundDevice)}"
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "Automation",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        Text(
            summary,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}
