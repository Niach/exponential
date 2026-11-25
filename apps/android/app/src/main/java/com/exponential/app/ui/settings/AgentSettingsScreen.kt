package com.exponential.app.ui.settings

import androidx.compose.foundation.layout.statusBarsPadding
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PinnedScaffold
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
import com.exponential.app.data.api.AgentVoicePatch
import com.exponential.app.domain.AgentVoiceSection
import com.exponential.app.domain.AgentWake
import com.exponential.app.domain.MemberVoiceLike
import com.exponential.app.domain.Voice
import com.exponential.app.domain.agentVoiceOf
import com.exponential.app.ui.components.TextFieldRow
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.api.UpdateAgentPatch
import com.exponential.app.domain.AccountOptions
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.AgentTrigger
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.DestructiveButton
import com.exponential.app.ui.components.ColorPicker
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.theme.glassGroup
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.picker.AccountPicker
import com.exponential.app.ui.components.picker.AccountPickerOption
import com.exponential.app.ui.components.picker.DevicePicker
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.picker.SkillPicker
import com.exponential.app.ui.components.picker.McpServerPicker
import com.exponential.app.domain.agentMcpServersValue
import com.exponential.app.domain.Skills
import com.exponential.app.domain.agentSkillsValue
import com.exponential.app.ui.components.PickerValueRow
import com.exponential.app.ui.components.toPickerAccount
import com.exponential.app.ui.components.toPickerDevice
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.background
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// EXP-1296 (`agent-settings.json`): Team settings → Agents,
// managed EXACTLY like boards. The list = icon-in-colour + name rows under the
// "Agents" band, whose header carries the `New agent` pill (owner-only,
// NEW-40: the Prompts band's pattern ×4). An agent's
// page (#EXP-1846, ONE order for the Assistant and every team agent) = the
// board form's ONE identity field (icon picker + colour swatch + name), Runs
// on (Device, Account, Model, Skills, MCP servers), Triggers
// ([AgentTriggersSection]), every agent's Voice (#EXP-1847), a team agent's
// Instructions and the owner-only Delete. A NEW agent is a form with a Create button
// (`sessions.send({agent})`); an existing one autosaves like the board page:
// name and instructions on blur, everything else at once.

private const val CREATE_AGENT_LABEL = "Create agent"

/** The agent's glyph, tinted in its colour (the board row's [BoardIcon]). */
@Composable
internal fun AgentGlyph(icon: String, color: String, modifier: Modifier = Modifier) {
    Icon(
        ExpIcons.byName(icon) ?: ExpIcons.byName(SessionTree.ListCopy.AGENT_DEFAULT_ICON) ?: ExpIcons.settingsAgents,
        contentDescription = null,
        tint = remember(color) { parseColor(color) },
        modifier = modifier.size(16.dp),
    )
}

/**
 * Team settings → Agents, the LIST: one row per agent of the selected team
 * under the band whose header carries the `New agent` pill (owner-only). Embeddable (a settings section) — no
 * Scaffold of its own.
 */
@Composable
fun AgentsSettingsList(
    onOpenAgent: (agentId: String) -> Unit,
    onNewAgent: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: AgentSettingsViewModel = hiltViewModel(),
) {
    val agents by viewModel.agents.collectAsStateWithLifecycle()
    val isOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()
    val events by viewModel.events.collectAsStateWithLifecycle()
    val lead by viewModel.lead.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    // FEED-79: a refused lead pick = the failed-action toast.
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    LaunchedEffect(Unit) { viewModel.ensureAssistant() }
    // NEW-2 (web Settings → Agents): the agents band over the team's Events.
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(24.dp)) {
        // #EXP-1735: the Lead Assistant row heads the group where the
        // Coordinator row was (owners pick it).
        LeadAssistantRow(lead, enabled = isOwner && !busy && lead.known, onPick = viewModel::setLeadAssistant)
        AgentsSettingsListContent(
            // #EXP-1987 (settings-nav.json): the Agents group lists the team's
            // agents only; the Assistant is Settings › Assistant (Personal).
            agents = agents?.filter { !it.assistant },
            canCreate = isOwner,
            onOpenAgent = onOpenAgent,
            onNewAgent = onNewAgent,
        )
        AgentEventsBand(events)
    }
}

/**
 * #EXP-1735 (agent-settings.json `lead`, web `LeadAssistantRow`): THE
 * settings Picker row where the Coordinator row was (single-select, the
 * merge-mode row's look): value = the lead member's display name or `None`;
 * owners pick, members read it disabled.
 */
@Composable
internal fun LeadAssistantRow(
    state: AgentSettingsViewModel.LeadState,
    enabled: Boolean,
    onPick: (userId: String?) -> Unit,
    modifier: Modifier = Modifier,
) {
    val none = AgentSettings.Lead.NONE
    val items = state.options.map { (id, name) ->
        com.exponential.app.ui.components.picker.PickerItem(id ?: none, name)
    }
    val current = state.value ?: none
    Column(modifier.fillMaxWidth().glassGroup()) {
        com.exponential.app.ui.components.picker.Picker(
            items = items,
            mode = com.exponential.app.ui.components.picker.PickerMode.Single,
            value = setOf(current),
            onChange = { picked ->
                picked.firstOrNull()?.takeIf { it != current }?.let { id -> onPick(id.takeIf { it != none }) }
            },
            title = AgentSettings.Lead.LABEL,
            enabled = enabled,
        ) { open ->
            PickerValueRow(
                label = AgentSettings.Lead.LABEL,
                value = items.firstOrNull { it.value == current }?.label ?: AgentSettings.Lead.NONE_LABEL,
                onClick = open,
                enabled = enabled,
                modifier = Modifier.testTag(AgentSettings.Lead.TEST_ID_ROW),
            )
        }
    }
}

/**
 * NEW-2: the Events band (web `AgentEventsBand`): the team's event stream
 * newest first as flat rows — the trigger-event glyph, the name, who emitted
 * it, a relative time. null [events] = still loading (skeleton rows).
 */
@Composable
internal fun AgentEventsBand(events: List<AgentEventRow>?, modifier: Modifier = Modifier) {
    Column(modifier = modifier.testTag("agent-events")) {
        SectionHeader(AgentSettings.EVENTS_TITLE)
        when {
            events == null -> com.exponential.app.ui.components.SkeletonRows(modifier = Modifier.padding(vertical = 4.dp))
            events.isEmpty() -> com.exponential.app.ui.components.EmptyState(
                title = AgentSettings.NO_EVENTS,
                icon = ExpIcons.triggerEvent,
                size = at.exponential.ui.primitives.EmptyStateSize.List,
            )
            else -> {
                val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
                val now = System.currentTimeMillis()
                events.forEach { event ->
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("agent-event-${event.id}")
                            .flatRow()
                            .padding(horizontal = 12.dp, vertical = 10.dp),
                    ) {
                        Icon(ExpIcons.triggerEvent, contentDescription = null, tint = muted, modifier = Modifier.size(16.dp))
                        Text(
                            event.name,
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            event.emitter,
                            style = MaterialTheme.typography.bodyMedium,
                            color = muted,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            com.exponential.app.ui.issue.compactRelativeTime(event.createdAt, now),
                            style = MaterialTheme.typography.bodySmall,
                            color = muted,
                        )
                    }
                }
            }
        }
    }
}

/**
 * The stateless list. NEW-40: the Prompts band's pattern ×4 (web
 * `AgentsListBand`, iOS `TeamAgentsSection`): the `New agent` pill rides the
 * band header (owner-only), gapless rows, the list-size empty state.
 */
/** #EXP-1788: an agent row's height (10 dp padding ×2 + bodyLarge 24). */
private val AGENT_ROW_HEIGHT = 44.dp

@Composable
internal fun AgentsSettingsListContent(
    /** null = not read yet (#EXP-1788: skeleton rows in the rows' geometry). */
    agents: List<SessionTree.AgentRow>?,
    canCreate: Boolean,
    onOpenAgent: (agentId: String) -> Unit,
    onNewAgent: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.testTag(AgentSettings.TEST_ID_LIST)) {
        SectionHeader(AgentSettings.Copy.TITLE) {
            if (canCreate) {
                GlassPill(
                    AgentSettings.Copy.NEW_AGENT,
                    icon = ExpIcons.uiAdd,
                    size = PillSize.Sm,
                    onClick = onNewAgent,
                    modifier = Modifier.testTag("agents-settings-new"),
                )
            }
        }
        if (agents == null) {
            com.exponential.app.ui.components.SkeletonRows(modifier = Modifier.padding(vertical = 4.dp), rowHeight = AGENT_ROW_HEIGHT)
        } else if (agents.isEmpty()) {
            EmptyState(
                title = AgentSettings.Copy.NO_AGENTS,
                icon = ExpIcons.settingsAgents,
                size = EmptyStateSize.List,
            )
        }
        agents.orEmpty().forEach { row ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(SessionTree.ListCopy.TEST_ID_AGENT_ROW)
                    .flatRow()
                    .clickable { onOpenAgent(row.agentId) }
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                if (row.assistant) {
                    // S-3: the viewer's Assistant = the brand mark, first.
                    com.exponential.app.ui.components.ExponentialMark(size = 16.dp, tint = MaterialTheme.colorScheme.onSurface)
                } else {
                    AgentGlyph(row.icon, row.color)
                }
                Text(
                    row.name,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

/**
 * The Agents settings PAGE (`settings/agents`) — a pushed screen hosting
 * [AgentsSettingsList].
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentsSettingsScreen(
    onBack: () -> Unit,
    onOpenAgent: (agentId: String) -> Unit,
    onNewAgent: () -> Unit,
) {
    PinnedScaffold(
        topBar = {
            PageHeader(
                title = AgentSettings.Copy.TITLE,
                onBack = onBack,
                modifier = Modifier.statusBarsPadding(),
            )
        },
    ) { padding ->
        AgentsSettingsList(
            onOpenAgent = onOpenAgent,
            onNewAgent = onNewAgent,
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
        )
    }
}

/**
 * ONE agent's settings page (`settings/agent/{agentId}`; the
 * `settings/agent-new` route = `New agent`, the model's `agentId` null).
 * [onCreated] lands on a just-created agent's page; [onDeleted]
 * leaves the page.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AgentSettingsScreen(
    onBack: () -> Unit,
    onCreated: (agentId: String) -> Unit,
    onDeleted: () -> Unit,
    viewModel: AgentSettingsViewModel = hiltViewModel(),
) {
    val agent by viewModel.agent.collectAsStateWithLifecycle()
    val devices by viewModel.devices.collectAsStateWithLifecycle()
    val loadedInstructions by viewModel.instructions.collectAsStateWithLifecycle()
    val syncedTriggers by viewModel.triggers.collectAsStateWithLifecycle()
    val isOwner by viewModel.isTeamOwner.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val teamSkills by viewModel.skills.collectAsStateWithLifecycle()
    val teamMcpServers by viewModel.mcpServers.collectAsStateWithLifecycle()
    val teamPicks by viewModel.teamPicks.collectAsStateWithLifecycle()
    val clipBusy by viewModel.clipBusy.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    // #EXP-1847 (`voice.clipUpload`): the system file picker over WAV.
    val clipPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        uri?.let(viewModel::uploadVoiceClip)
    }

    val isNew = viewModel.agentId == null
    // The form's own copy of the row, seeded ONCE (an Electric echo must
    // never fight a person mid-edit).
    var name by remember { mutableStateOf("") }
    var icon by remember { mutableStateOf(SessionTree.ListCopy.AGENT_DEFAULT_ICON) }
    var color by remember { mutableStateOf(SessionTree.ListCopy.AGENT_DEFAULT_COLOR) }
    var deviceId by remember { mutableStateOf<String?>(null) }
    var accountKey by remember { mutableStateOf<String?>(null) }
    var instructions by remember { mutableStateOf("") }
    var instructionsSeeded by remember { mutableStateOf(false) }
    var seeded by remember { mutableStateOf(false) }
    // A new agent's triggers (and Model pick) live here until Create sends it.
    var draftTriggers by remember { mutableStateOf<List<AgentTrigger>>(emptyList()) }
    var draftModel by remember { mutableStateOf<String?>(null) }
    var showErrors by remember { mutableStateOf(false) }

    LaunchedEffect(agent) {
        val row = agent ?: return@LaunchedEffect
        if (seeded) return@LaunchedEffect
        seeded = true
        name = row.name
        icon = row.icon ?: SessionTree.ListCopy.AGENT_DEFAULT_ICON
        color = row.color ?: SessionTree.ListCopy.AGENT_DEFAULT_COLOR
        // #EXP-1730: the runner picks read `agents.defaults`.
        val picks = row.runnerDefaults
        deviceId = picks.deviceId
        accountKey = picks.account?.let { account -> picks.cli?.let { "$it:$account" } }
    }
    LaunchedEffect(loadedInstructions) {
        val text = loadedInstructions ?: return@LaunchedEffect
        if (instructionsSeeded) return@LaunchedEffect
        instructionsSeeded = true
        instructions = text
    }

    val deviceList = devices.orEmpty()
    val device = deviceList.firstOrNull { it.deviceId == deviceId }
    val accountOptions = remember(device) { accountOptionsFor(device, device?.acpAgents.orEmpty()) }
    // EXP-1306: the Account row's value = `agentAccountValue` over the pin
    // (`<cli>:<profile>` while editing, the synced row's cli otherwise).
    val syncedCli = agent?.runnerDefaults?.cli
    val accountValueKey = remember(accountOptions, accountKey, syncedCli) {
        val parsed = accountKey?.let(AccountOptions::parseKey)
        AgentSettings.agentAccountValue(
            options = accountOptions.map { AgentSettings.AccountValueOption(it.id, it.agent, it.email, it.isLastUsed) },
            account = parsed?.id,
            cli = parsed?.agent ?: syncedCli,
        )?.let { "${it.agent}:${it.id}" }
    }
    val triggers = if (isNew) draftTriggers else syncedTriggers
    // The Model row's CLI: the picked login's (a new agent), else the synced row's.
    val modelCli = accountKey?.let(AccountOptions::parseKey)?.agent?.takeIf { isNew } ?: syncedCli
    val errors = viewModel.validate(name, deviceId, instructions)
    // S-3: the viewer's Assistant = its own page (owner = the member).
    val me by viewModel.me.collectAsStateWithLifecycle()
    val assistant = agent?.isAssistant == true
    val editable = if (assistant) agent?.ownerId == me else isOwner
    // NEW-99 (`runsOnCases`): an Assistant's Runs on lists desktops only
    // (its device is the voice device); a bound non-desktop stays listed.
    // THE rows (`runsOnRowCases`, web `deviceRows`): an offline device stays
    // listed, `Offline` and disabled unless bound; an unmatched bound id leads.
    val pickerDevices = remember(deviceList, deviceId, assistant) {
        val byId = deviceList.associateBy { it.deviceId }
        AgentSettings.runsOnRows(
            deviceList.map { AgentSettings.RunsOnDevice(it.deviceId, it.kind) },
            { byId[it]?.online == true },
            deviceId,
            assistant,
        ).map { row ->
            byId[row.id]?.toPickerDevice(startGate = false)
                ?.copy(description = row.description, disabled = row.disabled)
                ?: DevicePickerDevice(id = row.id, name = row.id)
        }
    }

    var confirmDelete by remember { mutableStateOf(false) }

    fun patch(update: UpdateAgentPatch) {
        if (!isNew) viewModel.update(update)
    }

    PinnedScaffold(
        topBar = {
            PageHeader(
                title = if (isNew) AgentSettings.Copy.NEW_AGENT else agent?.name ?: name,
                onBack = onBack,
                modifier = Modifier.statusBarsPadding(),
            )
        },
    ) { padding ->
        AgentSettingsContent(
            isNew = isNew,
            editable = editable,
            name = name,
            onNameChange = { name = it },
            onNameCommit = {
                val trimmed = name.trim()
                if (errors.none { it.field == AgentSettings.FIELD_IDENTITY } && trimmed != agent?.name) {
                    patch(UpdateAgentPatch(name = trimmed))
                } else if (!isNew) {
                    showErrors = true
                }
            },
            icon = icon,
            onIconChange = {
                icon = it
                patch(UpdateAgentPatch(icon = it))
            },
            color = color,
            onColorChange = {
                color = it
                patch(UpdateAgentPatch(color = it))
            },
            devices = pickerDevices,
            deviceId = deviceId,
            onDeviceChange = {
                if (it != deviceId) {
                    deviceId = it
                    accountKey = null
                    patch(UpdateAgentPatch(deviceId = it))
                }
            },
            accounts = accountOptions.map { it.toPickerAccount() },
            accountValueKey = accountValueKey,
            onAccountChange = { key ->
                accountKey = key
                val parsed = AccountOptions.parseKey(key)
                if (parsed != null) {
                    patch(
                        if (parsed.id.isEmpty()) UpdateAgentPatch(clearAccount = true)
                        else UpdateAgentPatch(account = parsed.id),
                    )
                }
            },
            // #EXP-1846: THE trigger list (Check in + the listed triggers),
            // every agent's, the Assistant's too.
            triggersSection = {
                AgentTriggersSection(
                    agentId = viewModel.agentId,
                    editable = editable,
                    triggers = triggers,
                    busy = busy,
                    onWrite = { onDone, build ->
                        if (isNew) {
                            draftTriggers = build(draftTriggers)
                            onDone()
                        } else {
                            viewModel.writeTriggers(onDone, build)
                        }
                    },
                )
            },
            instructions = instructions,
            instructionsLoading = !isNew && loadedInstructions == null,
            onInstructionsChange = { instructions = it },
            onInstructionsCommit = {
                // #EXP-1439: an Assistant runs on the built-in prompt; never send its instructions.
                if (!isNew && !assistant && instructionsSeeded && instructions != loadedInstructions &&
                    errors.none { it.field == AgentSettings.FIELD_INSTRUCTIONS }
                ) {
                    patch(UpdateAgentPatch(instructions = instructions))
                }
            },
            errors = if (showErrors) errors else emptyList(),
            busy = busy,
            onCreate = if (isNew) {
                {
                    showErrors = true
                    val picked = deviceId
                    if (errors.isEmpty() && picked != null) {
                        val parsed = accountKey?.let(AccountOptions::parseKey)
                        viewModel.create(
                            name = name,
                            icon = icon,
                            color = color,
                            deviceId = picked,
                            account = parsed?.id?.takeIf { it.isNotEmpty() },
                            cli = parsed?.agent,
                            model = draftModel,
                            instructions = instructions,
                            triggers = draftTriggers,
                            onCreated = onCreated,
                        )
                    }
                }
            } else {
                null
            },
            // S-3: nobody deletes an Assistant.
            onDelete = if (!isNew && isOwner && !assistant) ({ confirmDelete = true }) else null,
            assistant = assistant,
            // #EXP-1846: the Model row is every agent's (`assistantModelValue`).
            model = if (isNew) draftModel else agent?.runnerDefaults?.model,
            modelOptions = com.exponential.app.ui.components.modelOptionsFor(modelCli ?: "claude"),
            modelCli = modelCli,
            onModelChange = {
                if (isNew) draftModel = it else patch(UpdateAgentPatch(model = it))
            },
            // #EXP-1847 (agent-settings.json `voice`): EVERY agent's own Voice
            // section (an existing agent: the clip uploads against its id);
            // the Assistant's member rows follow it for its member.
            voice = agent?.takeIf { !isNew }?.let { row ->
                {
                    AgentVoiceSectionBlock(
                        agent = row,
                        editable = editable,
                        clipBusy = clipBusy,
                        onSave = viewModel::saveVoice,
                        onPickClip = { clipPicker.launch(AgentVoiceSection.ClipUpload.ACCEPT.toTypedArray()) },
                        onRemoveClip = viewModel::removeVoiceClip,
                        memberRows = assistant && editable,
                    )
                }
            },
            // #EXP-1661: the agent's default skills (an existing agent only:
            // `New agent` picks them after Create). Owners edit, members read.
            // #EXP-1988: the Assistant's skills / MCP servers are per team
            // ([teamPicksSections]), never in its Runs on.
            skillsRow = agent?.takeIf { !isNew && !assistant }?.let { row ->
                { AgentSkillsRow(row, teamSkills, editable) { patch(it) } }
            },
            // #EXP-1793 (agent-settings.json `mcpRow`): the agent's MCP
            // servers, right under Skills; THE composer's MCP servers picker.
            // Shown while the team has a server or the agent names one.
            mcpRow = agent?.takeIf { !isNew && !assistant }
                ?.takeIf { AgentSettings.McpRow.visible(teamMcpServers.size, it.mcpServerIdList.size) }
                ?.let { row -> { AgentMcpRow(row, teamMcpServers, editable) { patch(it) } } },
            teamPicksSections = if (assistant && !isNew) {
                { AgentTeamPicksSections(teamPicks, editable, viewModel::updateTeamPick) }
            } else {
                null
            },
            modifier = Modifier
                .padding(padding)
                .consumeWindowInsets(padding)
                .imePadding()
                .fillMaxSize(),
        )
    }

    if (confirmDelete) {
        GlassAlert(
            title = AgentSettings.Copy.DELETE_CONFIRM,
            onDismiss = { confirmDelete = false },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { confirmDelete = false }),
                GlassAlertAction(
                    "Delete",
                    onClick = {
                        confirmDelete = false
                        viewModel.delete(onDeleted)
                    },
                    destructive = true,
                    testTag = "agent-settings-delete-confirm",
                ),
            ),
        )
    }
}

/**
 * The stateless agent page (#EXP-1846, agent-settings.json `page`): ONE
 * order for the Assistant and every team agent: identity, Runs on (Device,
 * Account, Model, Skills, MCP servers), Triggers ([triggersSection]), Voice
 * ([voice], every existing agent's), Instructions (team agents only), Delete (team
 * agents, owners). [onCreate] non-null = the `New agent` form's submit;
 * [onDelete] non-null = the owner's Delete.
 */
@Composable
internal fun AgentSettingsContent(
    isNew: Boolean,
    editable: Boolean,
    name: String,
    onNameChange: (String) -> Unit,
    onNameCommit: () -> Unit,
    icon: String,
    onIconChange: (String) -> Unit,
    color: String,
    onColorChange: (String) -> Unit,
    devices: List<DevicePickerDevice>,
    deviceId: String?,
    onDeviceChange: (String) -> Unit,
    accounts: List<AccountPickerOption>,
    /** The option the Account row reads ([AgentSettings.agentAccountValue]'s
     *  pick, as an option key); null = no login reported. */
    accountValueKey: String?,
    onAccountChange: (String) -> Unit,
    /** #EXP-1846: THE trigger list ([AgentTriggersSection]): its header with
     *  `+ Add`, the Check in row, the listed triggers. */
    triggersSection: @Composable () -> Unit,
    instructions: String,
    instructionsLoading: Boolean,
    onInstructionsChange: (String) -> Unit,
    onInstructionsCommit: () -> Unit,
    errors: List<AgentSettings.FormError>,
    busy: Boolean,
    onCreate: (() -> Unit)?,
    onDelete: (() -> Unit)?,
    modifier: Modifier = Modifier,
    /** S-3 (agent-settings.json `assistant`): THE ASSISTANT = the brand mark
     *  instead of the icon picker, its member voice rows; no Instructions
     *  (#EXP-1439: the prompt is built in), no Delete. */
    assistant: Boolean = false,
    model: String? = null,
    modelOptions: List<String> = emptyList(),
    /** The login's CLI the unpinned Model row defaults for (null = claude). */
    modelCli: String? = null,
    onModelChange: (String) -> Unit = {},
    /** #EXP-1847: the Voice section (its header + rows), after Triggers. */
    voice: (@Composable () -> Unit)? = null,
    /** #EXP-1661 (skills.json `agentRow`): the Skills row, after Model. */
    skillsRow: (@Composable () -> Unit)? = null,
    /** #EXP-1793 (agent-settings.json `mcpRow`): the MCP servers row, right
     *  under Skills, the last row of Runs on. */
    mcpRow: (@Composable () -> Unit)? = null,
    /** #EXP-1988 (`page.sections` › `team-picks`): the Assistant's one
     *  section per team, right after Runs on. */
    teamPicksSections: (@Composable () -> Unit)? = null,
) {
    fun errorOf(field: String) = errors.firstOrNull { it.field == field }?.message
    fun label(field: String) = AgentSettings.FIELDS.first { it.key == field }.label
    val device = devices.firstOrNull { it.id == deviceId }

    Column(
        modifier = modifier
            .testTag(AgentSettings.TEST_ID_PAGE)
            .verticalScroll(rememberScrollState())
            .padding(vertical = 8.dp),
    ) {
        // ── identity ─────────────────────────────────────────────────────
        OptionGroup {
            // The board form's ONE identity field: icon picker, colour
            // swatch and the name, all one row.
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth().padding(start = 12.dp, top = 6.dp, bottom = 6.dp),
            ) {
                if (assistant) {
                    // The Assistant wears the brand mark, never an icon pick.
                    com.exponential.app.ui.components.ExponentialMark(size = 36.dp, tint = MaterialTheme.colorScheme.onSurface)
                } else if (editable) {
                    IconPicker(selected = icon, onSelect = onIconChange, accentColor = parseColor(color))
                    ColorPicker(selected = color, onSelect = onColorChange)
                } else {
                    Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { AgentGlyph(icon, color) }
                }
                // No caption: the glyph + colour ARE the label (web/iOS/desktop).
                GlassTextField(
                    value = name,
                    onValueChange = onNameChange,
                    placeholder = AgentSettings.Copy.NAME_PLACEHOLDER,
                    enabled = editable,
                    singleLine = true,
                    bordered = false,
                    containerColor = Color.Transparent,
                    modifier = Modifier
                        .weight(1f)
                        .testTag("agent-settings-name")
                        .commitOnBlur { if (!isNew) onNameCommit() },
                )
            }
        }
        FieldErrors(listOfNotNull(errorOf(AgentSettings.FIELD_IDENTITY)))
        Spacer(Modifier.height(12.dp))

        // ── Runs on: Device, Account, Model, Skills, MCP servers ─────────
        SectionHeader(AgentSettings.Copy.RUNS_ON, modifier = Modifier.padding(horizontal = 16.dp))
        OptionGroup {
            AgentRunsOnRows(
                devices = devices,
                deviceId = deviceId,
                onDeviceChange = onDeviceChange,
                deviceValue = device?.name?.substringBefore(" — ") ?: "Select",
                accounts = accounts,
                accountValueKey = accountValueKey,
                onAccountChange = onAccountChange,
                editable = editable,
                model = model,
                modelOptions = modelOptions,
                modelCli = modelCli,
                onModelChange = onModelChange,
                skillsRow = skillsRow,
                mcpRow = mcpRow,
            )
        }
        FieldErrors(listOfNotNull(errorOf(AgentSettings.FIELD_DEVICE)))
        Spacer(Modifier.height(12.dp))

        // ── #EXP-1988: the Assistant's skills + MCP servers, per team ────
        teamPicksSections?.invoke()

        // ── Triggers: Check in, then the listed triggers ─────────────────
        triggersSection()

        // ── Voice (every agent, #EXP-1847) ───────────────────────────────
        if (voice != null) {
            Spacer(Modifier.height(12.dp))
            voice()
        }
        Spacer(Modifier.height(12.dp))

        // ── Instructions (markdown, server-only; never an Assistant's) ───
        if (!assistant) {
            OptionGroup {
                GlassTextField(
                    value = instructions,
                    onValueChange = onInstructionsChange,
                    placeholder = if (instructionsLoading) null else AgentSettings.Copy.INSTRUCTIONS_PLACEHOLDER,
                    minLines = 5,
                    enabled = editable && !instructionsLoading,
                    bordered = false,
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("agent-settings-instructions")
                        .commitOnBlur(onInstructionsCommit),
                )
            }
            Text(
                label(AgentSettings.FIELD_INSTRUCTIONS),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(start = 20.dp, top = 6.dp),
            )
            FieldErrors(listOfNotNull(errorOf(AgentSettings.FIELD_INSTRUCTIONS)))
            Spacer(Modifier.height(20.dp))
        }

        if (onCreate != null) {
            GlassSubmitButton(
                label = CREATE_AGENT_LABEL,
                enabled = editable && !busy,
                onClick = onCreate,
                modifier = Modifier.padding(horizontal = 16.dp).testTag("agent-settings-create"),
            )
        }
        if (onDelete != null) {
            // EXP-1554: THE destructive button, under the form, left-aligned.
            DestructiveButton(
                label(AgentSettings.FIELD_DELETE),
                onClick = onDelete,
                modifier = Modifier.padding(horizontal = 16.dp).testTag("agent-settings-delete"),
            )
        }
        Spacer(Modifier.height(24.dp))
    }
}

/**
 * #EXP-1846 (agent-settings.json `page` › `runs-on`): THE Runs on rows
 * (Device, Account, Model, Skills, MCP servers) inside the caller's group.
 * #EXP-1849 (apps.json `agentSlot`): the App settings' Agent group mounts
 * the same rows; [runner] false = no Account and Model (an empty slot).
 */
@Composable
internal fun AgentRunsOnRows(
    devices: List<DevicePickerDevice>,
    deviceId: String?,
    onDeviceChange: (String) -> Unit,
    deviceValue: String,
    accounts: List<AccountPickerOption>,
    accountValueKey: String?,
    onAccountChange: (String) -> Unit,
    editable: Boolean,
    model: String?,
    modelOptions: List<String>,
    modelCli: String?,
    onModelChange: (String) -> Unit,
    skillsRow: (@Composable () -> Unit)?,
    mcpRow: (@Composable () -> Unit)?,
    deviceLabel: String = AgentSettings.FIELDS.first { it.key == AgentSettings.FIELD_DEVICE }.label,
    /** The Device sheet's headline (#EXP-1849: the slot's `Runs on`). */
    deviceTitle: String = deviceLabel,
    runner: Boolean = true,
) {
    val account = accountValueKey?.let { key -> accounts.firstOrNull { it.key == key } }
    DevicePicker(
        devices = devices,
        value = deviceId,
        onChange = onDeviceChange,
        title = deviceTitle,
        trigger = { open ->
            PickerValueRow(
                label = deviceLabel,
                // EXP-1554: the value is the machine alone (×4); the
                // owner suffix only tells picker rows apart.
                value = deviceValue,
                onClick = open,
                enabled = editable,
                modifier = Modifier.testTag("agent-settings-device"),
            )
        },
    )
    if (runner) {
        GroupDivider()
        AccountPicker(
            options = accounts,
            value = account?.key,
            onChange = onAccountChange,
            trigger = { open ->
                // EXP-1306: the composer chip's value — the brand mark + the
                // email; no login reported = `Last used`, muted.
                PickerValueRow(
                    label = AgentSettings.FIELDS.first { it.key == AgentSettings.FIELD_ACCOUNT }.label,
                    value = account?.email ?: AgentSettings.Copy.ACCOUNT_LAST_USED,
                    valueAgent = account?.agent,
                    onClick = open,
                    enabled = editable && accounts.isNotEmpty(),
                    modifier = Modifier.testTag("agent-settings-account"),
                )
            },
        )
        GroupDivider()
        // agent-settings.json `modelValueCases`: unpinned = the default
        // for the login's CLI, any agent's.
        val shownModel = AgentSettings.Assistant.modelValue(model, modelCli)
        com.exponential.app.ui.components.picker.ModelPicker(
            options = modelOptions.map { com.exponential.app.ui.components.picker.ModelPickerOption(it, com.exponential.app.ui.components.modelLabel(it)) },
            value = shownModel,
            onChange = onModelChange,
            trigger = { open ->
                PickerValueRow(
                    label = "Model",
                    value = com.exponential.app.ui.components.modelLabel(shownModel),
                    onClick = open,
                    enabled = editable && modelOptions.isNotEmpty(),
                    modifier = Modifier.testTag("agent-settings-model"),
                )
            },
        )
    }
    if (skillsRow != null) {
        GroupDivider()
        skillsRow()
    }
    if (mcpRow != null) {
        GroupDivider()
        mcpRow()
    }
}

/**
 * #EXP-1988 (agent-settings.json `assistant.teamPicks`, `page.sections` ›
 * `team-picks`): ONE section per synced team of the Assistant's member, in
 * the team switcher's order: THE section header (the team's name) over one
 * grouped card holding THE Skills row and THE MCP servers row over THAT
 * team's rows. A pick writes the section agent's team pick only
 * (`agents.update {id, pickTeamId, skillIds | mcpServerIds}`).
 */
@Composable
internal fun AgentTeamPicksSections(
    sections: List<AgentSettingsViewModel.TeamPickSection>,
    editable: Boolean,
    onPick: (agentId: String, patch: UpdateAgentPatch) -> Unit,
) {
    for (section in sections) {
        val row = section.agent ?: continue
        SectionHeader(section.teamName, modifier = Modifier.padding(horizontal = 16.dp).testTag("agent-settings-team-${section.teamId}"))
        OptionGroup {
            AgentSkillsRow(row, section.skills, editable) { onPick(row.id, it.copy(pickTeamId = section.teamId)) }
            if (AgentSettings.McpRow.visible(section.mcpServers.size, row.mcpServerIdList.count { id -> section.mcpServers.any { it.id == id } })) {
                GroupDivider()
                AgentMcpRow(row, section.mcpServers, editable) { onPick(row.id, it.copy(pickTeamId = section.teamId)) }
            }
        }
        Spacer(Modifier.height(12.dp))
    }
}

/** #EXP-1661 (skills.json `agentRow`): the agent's default skills (owners
 *  edit, members read), clamped to skills that still exist. */
@Composable
internal fun AgentSkillsRow(
    row: com.exponential.app.data.db.AgentDefinitionEntity,
    teamSkills: List<com.exponential.app.domain.SkillOption>,
    editable: Boolean,
    patch: (UpdateAgentPatch) -> Unit,
) {
    val known = teamSkills.mapTo(HashSet()) { it.id }
    val picked = row.skillIdList.filter { it in known }
    SkillPicker(
        skills = teamSkills,
        value = picked,
        onToggle = { id ->
            val next = if (id in picked) picked - id else picked + id
            patch(UpdateAgentPatch(skillIds = next))
        },
        trigger = { open ->
            PickerValueRow(
                label = Skills.AGENT_ROW_LABEL,
                // The picked names, the one multi summary; `None` at zero.
                value = agentSkillsValue(teamSkills.filter { it.id in picked }.map { it.name }),
                onClick = open,
                enabled = editable && teamSkills.isNotEmpty(),
                modifier = Modifier.testTag("agent-settings-skills"),
            )
        },
    )
}

/** #EXP-1793 (agent-settings.json `mcpRow`): the agent's MCP servers, in
 *  the team list's order, clamped to servers that still exist. */
@Composable
internal fun AgentMcpRow(
    row: com.exponential.app.data.db.AgentDefinitionEntity,
    teamMcpServers: List<com.exponential.app.domain.McpServerOption>,
    editable: Boolean,
    patch: (UpdateAgentPatch) -> Unit,
) {
    val saved = row.mcpServerIdList.toSet()
    val picked = teamMcpServers.filter { it.id in saved }.map { it.id }
    McpServerPicker(
        servers = teamMcpServers,
        value = picked,
        onToggle = { id ->
            val next = if (id in picked) picked - id else picked + id
            patch(UpdateAgentPatch(mcpServerIds = next))
        },
        title = AgentSettings.McpRow.LABEL,
        trigger = { open ->
            PickerValueRow(
                label = AgentSettings.McpRow.LABEL,
                // The picked names, the one multi summary; `None` at zero.
                value = agentMcpServersValue(teamMcpServers.filter { it.id in saved }.map { it.name }),
                onClick = open,
                enabled = editable && teamMcpServers.isNotEmpty(),
                modifier = Modifier.testTag(AgentSettings.McpRow.TEST_ID),
            )
        },
    )
}

/** [onBlur] fires when focus LEAVES the field (never on the first,
 *  unfocused report), the board page's save-on-blur. */
@Composable
private fun Modifier.commitOnBlur(onBlur: () -> Unit): Modifier {
    var hadFocus by remember { mutableStateOf(false) }
    return onFocusChanged { state ->
        if (hadFocus && !state.hasFocus) onBlur()
        hadFocus = state.hasFocus
    }
}

@Composable
private fun FieldErrors(messages: List<String>) {
    messages.forEach { FieldError(it, modifier = Modifier.padding(start = 20.dp, top = 6.dp)) }
}

/**
 * #EXP-1847 (agent-settings.json `voice`): an agent's own Voice section under
 * header `copy.label`: Language (THE single-select picker row over the voice
 * languages), Voice (the clip row: `clipLabel`, a press opens the file picker
 * and uploads, Remove clip while it has one), Transcript and Wake phrase (THE
 * input rows, saved on blur / Done, the wake phrase's error inline). Values
 * resolve through [agentVoiceOf] against the member's `voice.get`. With
 * [memberRows] the Assistant's member rows follow (push to talk, wake word,
 * meetings), on the same voice view model.
 */
@Composable
private fun AgentVoiceSectionBlock(
    agent: com.exponential.app.data.db.AgentDefinitionEntity,
    editable: Boolean,
    clipBusy: Boolean,
    onSave: (AgentVoicePatch, String) -> Unit,
    onPickClip: () -> Unit,
    onRemoveClip: () -> Unit,
    memberRows: Boolean,
    voiceModel: VoiceSettingsViewModel = hiltViewModel(),
) {
    val vocabulary by voiceModel.vocabulary.collectAsStateWithLifecycle()
    val settings = voiceModel.settings
    val member = (settings ?: Voice.defaults).let {
        MemberVoiceLike(it.language, it.voiceFile, it.voiceTranscript, it.wakePhrase)
    }
    val own = agent.agentVoice
    val resolved = agentVoiceOf(agent.role, agent.name, own, member)
    val focus = LocalFocusManager.current
    // The rows show the RESOLVED values (web/iOS): re-seeded whenever the
    // resolution changes (the member's voice.get landing, a save's echo).
    var transcript by remember(agent.id, resolved.transcript) { mutableStateOf(resolved.transcript) }
    var wakePhrase by remember(agent.id, resolved.wakePhrase) { mutableStateOf(resolved.wakePhrase) }
    var wakePhraseError by remember(agent.id) { mutableStateOf<String?>(null) }
    // #EXP-1859: a TEAM agent's Wake word row (`wakeRow`; the Assistant's
    // page keeps the member row below).
    val teamAgent = !AgentSettings.Assistant.isAssistant(agent.role)
    val wakeAgents by voiceModel.wakeAgents.collectAsStateWithLifecycle()
    val wakeInput = settings?.let { voiceModel.wakeInput(it, wakeAgents) }
    val wakeRow = wakeInput?.let { AgentWake.row(it, agent.id) } ?: AgentWake.Row(on = false, error = null)
    val wakeError = voiceModel.wakeRefusals[agent.id] ?: wakeRow.error
    val listening = wakeInput != null && AgentWake.listeners(wakeInput).any { it.agentId == agent.id }
    // While it listens: the Assistant's Wake word detail, read the same way.
    val wakeDetail = if (!listening || settings == null) null else settings.voiceDeviceId?.let { deviceId ->
        Voice.wakeRowDetail(
            vocabulary.doctors[deviceId],
            Voice.WakeDetailContext(
                wakeWord = true,
                agent = settings.agentId != null,
                online = deviceId in vocabulary.online,
                device = vocabulary.deviceNames[deviceId] ?: Voice.Copy.ELLIPSIS,
                state = null,
            ),
        )
    }

    // A save equal to the resolved value sends nothing; a cleared row sends
    // null (the agent falls back to the member's / none).
    fun commitTranscript() {
        val trimmed = transcript.trim()
        if (!editable || trimmed == resolved.transcript) return
        onSave(AgentVoicePatch(transcript = trimmed), "the transcript")
    }

    fun commitWakePhrase() {
        val trimmed = wakePhrase.trim()
        if (!editable) return
        val error = Voice.wakePhraseError(trimmed)
        if (error != null) {
            wakePhraseError = error
            return
        }
        if (trimmed == resolved.wakePhrase) return
        onSave(AgentVoicePatch(wakePhrase = trimmed), "the wake phrase")
    }

    Column(Modifier.fillMaxWidth().testTag(AgentVoiceSection.TestIds.SECTION)) {
        SectionHeader(AgentVoiceSection.Copy.LABEL, modifier = Modifier.padding(horizontal = 16.dp))
        OptionGroup {
            // The plain language name, as web, iOS and desktop (the GPU
            // note stays on the member's Voice page, #NEW-74).
            val languages = Voice.languages.map { it.value }
            com.exponential.app.ui.components.picker.Picker(
                items = languages.map {
                    com.exponential.app.ui.components.picker.PickerItem(value = it, label = Voice.languageLabel(it, null))
                },
                mode = com.exponential.app.ui.components.picker.PickerMode.Single,
                value = setOf(resolved.language),
                onChange = { picked ->
                    picked.firstOrNull()?.takeIf { it != resolved.language }?.let {
                        onSave(AgentVoicePatch(language = it), "the language")
                    }
                },
                title = AgentVoiceSection.Copy.LANGUAGE,
                enabled = editable,
            ) { open ->
                PickerValueRow(
                    label = AgentVoiceSection.Copy.LANGUAGE,
                    value = Voice.languageLabel(resolved.language, null),
                    onClick = open,
                    enabled = editable,
                    modifier = Modifier.testTag(AgentVoiceSection.TestIds.LANGUAGE),
                )
            }
            GroupDivider()
            // The clip row: `clipLabel` (`Built-in` with none, the row's usual
            // value colour like any value, never muted), a press
            // uploads a WAV; a set clip offers Remove clip (the team image
            // row's trailing remove).
            Row(verticalAlignment = Alignment.CenterVertically) {
                PickerValueRow(
                    label = AgentVoiceSection.Copy.CLIP,
                    value = resolved.clipLabel,
                    onClick = onPickClip,
                    enabled = editable && !clipBusy,
                    modifier = Modifier.weight(1f).testTag(AgentVoiceSection.TestIds.CLIP),
                )
                if (editable && !own.clipId.isNullOrEmpty()) {
                    com.exponential.app.ui.components.CircleIconButton(
                        ExpIcons.uiDelete,
                        contentDescription = AgentVoiceSection.Copy.CLIP_REMOVE,
                        onClick = onRemoveClip,
                        enabled = !clipBusy,
                        glyphSize = 16.dp,
                        borderless = true,
                        size = 28.dp,
                        modifier = Modifier.padding(end = 8.dp).testTag(AgentVoiceSection.TestIds.CLIP_REMOVE),
                    )
                }
            }
            GroupDivider()
            TextFieldRow(
                label = AgentVoiceSection.Copy.TRANSCRIPT,
                value = transcript,
                onValueChange = { transcript = Voice.textInput(it, AgentVoiceSection.Limits.TRANSCRIPT) },
                placeholder = AgentVoiceSection.Copy.TRANSCRIPT_PLACEHOLDER,
                enabled = editable,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                onFocusLost = ::commitTranscript,
                modifier = Modifier.testTag(AgentVoiceSection.TestIds.TRANSCRIPT),
            )
            GroupDivider()
            TextFieldRow(
                label = AgentVoiceSection.Copy.WAKE_PHRASE,
                value = wakePhrase,
                onValueChange = {
                    wakePhraseError = null
                    wakePhrase = Voice.textInput(it, AgentVoiceSection.Limits.WAKE_PHRASE)
                },
                placeholder = AgentVoiceSection.wakePhrasePlaceholder(agent.name),
                enabled = editable,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                onFocusLost = ::commitWakePhrase,
                modifier = Modifier.testTag(AgentVoiceSection.TestIds.WAKE_PHRASE),
            )
            if (teamAgent) {
                // #EXP-1859 (`voice.wake`): THE Wake word switch row (the
                // Assistant's), the MEMBER's own setting: every member flips it.
                GroupDivider()
                SwitchRow(
                    title = AgentWake.Copy.WAKE_WORD,
                    checked = wakeRow.on,
                    onCheckedChange = { voiceModel.wakeAgent(agent.id, it) },
                    enabled = settings != null,
                    description = wakeDetail,
                    modifier = Modifier.testTag(AgentVoiceSection.TestIds.WAKE_WORD),
                )
            }
        }
        // #NEW-56: invalid input, inline under the field (never a toast).
        FieldError(wakePhraseError, modifier = Modifier.padding(start = 20.dp, top = 6.dp))
        // #EXP-1859: why a switched-on agent does not listen, or a refused flip.
        if (teamAgent) FieldError(wakeError, modifier = Modifier.padding(start = 20.dp, top = 6.dp))
        if (memberRows && settings != null) {
            // S-3: the voice.json member rows, below the agent's own rows.
            VoiceSettingsForm(
                settings = settings,
                vocabulary = vocabulary,
                wakePhrase = voiceModel.wakePhrase,
                wakePhraseError = voiceModel.wakePhraseError,
                onChange = voiceModel::save,
                onMeeting = voiceModel::setMeeting,
                onWakePhraseChange = voiceModel::onWakePhraseChange,
                onWakePhraseDone = voiceModel::saveWakePhrase,
                voiceFile = voiceModel.voiceFile,
                onVoiceFileChange = voiceModel::onVoiceFileChange,
                onVoiceFileDone = voiceModel::saveVoiceFile,
                voiceTranscript = voiceModel.voiceTranscript,
                onVoiceTranscriptChange = voiceModel::onVoiceTranscriptChange,
                onVoiceTranscriptDone = voiceModel::saveVoiceTranscript,
                assistantBlock = true,
            )
        }
    }
    FailedActionToast(voiceModel.failure, onShown = { voiceModel.failure = null })
}
