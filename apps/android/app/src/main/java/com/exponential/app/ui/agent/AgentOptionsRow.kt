package com.exponential.app.ui.agent

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.AgentAccountProfile
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.db.DeviceWorktreeEntity
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.SwitchThumb
import com.exponential.app.ui.components.agentIconPainter
import com.exponential.app.ui.components.agentLabel
import com.exponential.app.ui.components.deviceOptionLabel
import com.exponential.app.ui.components.effortLabel
import com.exponential.app.ui.components.effortValuesFor
import com.exponential.app.ui.components.glassSwitchColors
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelOptionsFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-825: the launch options as ONE muted inline line under the composer
 * card (Danny's variant B): Device, Agent, Model, a Plan switch, the Resume
 * switch inline while a worktree makes it offerable (EXP-481), the Repository
 * pick only while there is no subject (a chat's optional anchor, EXP-739), and
 * a `⋯` pill for the rest ([AgentOptionsSheet]: Effort, Ultracode, Account).
 * Every pill is a menu or a toggle — no disabled controls; the caption under
 * the row explains what cannot start. Horizontally scrolling: a phone cannot
 * fit six pills, and wrapping would push the sessions list around.
 */
@Composable
internal fun AgentOptionsRow(
    devices: List<SteerDevice>,
    device: SteerDevice?,
    onDeviceChange: (String) -> Unit,
    launch: LaunchDraft,
    availableAgents: List<String>,
    onAgentChange: (String) -> Unit,
    onModelChange: (String) -> Unit,
    onPlanModeChange: (Boolean) -> Unit,
    resumeCandidate: DeviceWorktreeEntity?,
    resume: Boolean,
    onResumeChange: (Boolean) -> Unit,
    showRepository: Boolean,
    repos: List<TeamRepo>,
    chatRepoId: String,
    onChatRepoChange: (String) -> Unit,
    /** EXP-862: the account pill's pick — `""` = the machine's active login. */
    onAccountChange: (String) -> Unit,
    onMore: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // A resume never re-enters plan mode (EXP-202) — the switch hides while
    // one is on, like the machine's own clamp.
    val resumeActive = resume && resumeCandidate != null
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .testTag("agent-options-row"),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // The machine the run lands on — always named once one resolves (the
        // desktop and web say it too); a menu only while there is a choice,
        // a lone machine reads as a plain label like a lone agent does.
        if (device != null) {
            OptionMenuPill(
                icon = if (device.isServer) ExpIcons.uiServer else ExpIcons.uiDevice,
                text = deviceOptionLabel(device),
                contentDescription = "Device",
                options = devices.map { it.deviceId },
                optionLabel = { id -> devices.firstOrNull { it.deviceId == id }?.let(::deviceOptionLabel) ?: id },
                // EXP-862: a picker whose VALUE carries a glyph carries it on
                // the items too — the device kind, here.
                optionIcon = { id ->
                    val row = devices.firstOrNull { it.deviceId == id }
                    if (row?.isServer == true) ExpIcons.uiServer else ExpIcons.uiDevice
                },
                selected = device.deviceId,
                onSelect = onDeviceChange,
                enabled = devices.size > 1,
                modifier = Modifier.testTag("agent-device-pill"),
            )
        }
        // The agent — the ONE picker every surface renders (EXP-862): the
        // brand mark alone, its name only in the menu and the description.
        // EXP-642: the store slide's pop-out rect used to be measured off the
        // segmented strip this replaced, so the testTag stays on this pill.
        AgentPickerPill(
            agent = launch.agent,
            agents = availableAgents,
            onSelect = onAgentChange,
            modifier = Modifier.testTag("start-coding-agent-picker"),
        )
        OptionMenuPill(
            text = modelLabel(launch.model),
            contentDescription = "Model",
            options = modelOptionsFor(launch.agent),
            optionLabel = ::modelLabel,
            selected = launch.model,
            onSelect = onModelChange,
        )
        // Plan mode is claude-only since EXP-849. EXP-827: a slide switch on
        // every platform, not a lit select pill.
        if (supportsPlanMode(launch.agent) && !resumeActive) {
            PlanSwitchPill(
                checked = launch.planMode,
                onCheckedChange = onPlanModeChange,
            )
        }
        // EXP-481: offered only while the picked machine's synced worktree
        // inventory carries a row for the ONE checked issue.
        if (resumeCandidate != null) {
            GlassPill(
                "Resume",
                onClick = { onResumeChange(!resume) },
                mode = PillMode.Select,
                selected = resume,
                contentDescription = "Resume previous session",
            )
        }
        // The chat's OPTIONAL repository (EXP-615/739): with one the run gets
        // its own `exp/chat-<id8>` worktree, without one it runs in the
        // agent's scratch dir. Only while there is no subject — an issue
        // brings its board's repo, an action its own.
        if (showRepository) {
            if (repos.isEmpty()) {
                GlassPill("No repository", icon = ExpIcons.actionRepository)
            } else {
                OptionMenuPill(
                    icon = ExpIcons.actionRepository,
                    text = repos.firstOrNull { it.id == chatRepoId }?.fullName ?: "No repository",
                    contentDescription = "Repository",
                    options = listOf("") + repos.map { it.id },
                    optionLabel = { id ->
                        if (id.isEmpty()) "No repository" else repos.firstOrNull { it.id == id }?.fullName ?: id
                    },
                    selected = chatRepoId,
                    onSelect = onChatRepoChange,
                )
            }
        }
        // EXP-862: the account is a DECISION, not an overflow entry — the pill
        // rides the row itself the moment the picked machine reports two or
        // more logins for the picked agent (web/desktop/iOS parity).
        val profiles: List<AgentAccountProfile> =
            device?.agentAccounts?.get(launch.agent)?.profiles.orEmpty()
        if (profiles.size >= 2) {
            OptionMenuPill(
                icon = ExpIcons.uiSignIn,
                text = accountPillLabel(profiles, launch.account),
                contentDescription = "Account",
                options = listOf("") + profiles.map { it.id },
                optionLabel = { id -> accountOptionLabel(profiles, id) },
                selected = launch.account,
                onSelect = onAccountChange,
                modifier = Modifier.testTag("agent-account-pill"),
            )
        }
        GlassPill(
            "",
            icon = ExpIcons.uiMore,
            onClick = onMore,
            contentDescription = "More options",
        )
    }
}

/** The active login reads as itself, never as an id. */
private const val ACTIVE_LOGIN_LABEL = "Active login"

private fun accountOptionLabel(profiles: List<AgentAccountProfile>, id: String): String =
    if (id.isEmpty()) {
        ACTIVE_LOGIN_LABEL
    } else {
        profiles.firstOrNull { it.id == id }
            ?.let { it.email ?: it.label?.trim()?.takeIf { l -> l.isNotEmpty() } ?: it.id }
            ?: id
    }

private fun accountPillLabel(profiles: List<AgentAccountProfile>, account: String): String =
    accountOptionLabel(profiles, account)

/**
 * EXP-862: THE agent picker of this client — an icon-only trigger (the brand
 * mark plus the chevron) whose menu names the agents. "Claude Code" / "Codex"
 * appear in the menu and in the accessibility description only; the row itself
 * is a mark, like web's `AgentPicker`, the desktop's `coding_selects::agent_picker`
 * and iOS's `AgentPickerMenu`.
 *
 * A lone agent is not a choice: the pill states it and opens nothing.
 */
@Composable
internal fun AgentPickerPill(
    agent: String,
    agents: List<String>,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var open by remember { mutableStateOf(false) }
    val enabled = agents.size > 1
    Box(modifier = modifier) {
        GlassPill(
            "",
            onClick = if (enabled) ({ open = true }) else null,
            leading = {
                Icon(
                    agentIconPainter(agent),
                    contentDescription = null,
                    modifier = Modifier.size(13.dp),
                )
            },
            trailing = if (enabled) {
                {
                    Icon(
                        ExpIcons.uiChevronDown,
                        contentDescription = null,
                        modifier = Modifier.size(10.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            } else {
                null
            },
            contentDescription = agentLabel(agent),
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            AgentMenuItems(
                agents = agents,
                selected = agent,
                onSelect = {
                    open = false
                    onSelect(it)
                },
            )
        }
    }
}

/**
 * The agent rows of a menu a surface already owns (EXP-862): the brand mark,
 * the agent's name, a check on the current pick.
 */
@Composable
internal fun AgentMenuItems(
    agents: List<String>,
    selected: String?,
    onSelect: (String) -> Unit,
) {
    agents.forEach { option ->
        GlassMenuItem(
            text = { Text(agentLabel(option)) },
            leadingIcon = {
                Icon(
                    agentIconPainter(option),
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                )
            },
            trailingIcon = if (option == selected) {
                { Icon(ExpIcons.uiCheck, contentDescription = null, modifier = Modifier.size(16.dp)) }
            } else {
                null
            },
            onClick = { onSelect(option) },
        )
    }
}

/**
 * EXP-827: the Plan switch on the row's capsule. The whole pill is the tap
 * target and reads as ONE switch to TalkBack; the M3 switch inside is a pure
 * indicator ([glassSwitchColors] chrome), scaled to sit in the 28dp capsule.
 */
@Composable
private fun PlanSwitchPill(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    GlassPill(
        "Plan",
        onClick = { onCheckedChange(!checked) },
        trailing = {
            Box(
                modifier = Modifier.size(width = 36.dp, height = 22.dp),
                contentAlignment = Alignment.Center,
            ) {
                Switch(
                    checked = checked,
                    onCheckedChange = null,
                    colors = glassSwitchColors(),
                    thumbContent = SwitchThumb,
                    modifier = Modifier
                        .requiredSize(width = 52.dp, height = 32.dp)
                        .scale(0.7f),
                )
            }
        },
        contentDescription = "Plan mode",
        modifier = Modifier.semantics {
            role = Role.Switch
            toggleableState = if (checked) ToggleableState.On else ToggleableState.Off
        },
    )
}

/**
 * One option pill that opens a menu of its values: a glyph, the picked value
 * and a chevron, on the capsule every other pill wears. [enabled] false
 * renders the label alone — a single value is a statement, not a choice.
 *
 * EXP-862: [optionIcon] puts the value's own glyph on the MENU rows too — a
 * picker whose trigger shows an icon shows it on its items.
 */
@Composable
private fun OptionMenuPill(
    text: String,
    contentDescription: String,
    options: List<String>,
    optionLabel: (String) -> String,
    selected: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    optionIcon: ((String) -> ImageVector?)? = null,
    enabled: Boolean = true,
) {
    var open by remember { mutableStateOf(false) }
    Box(modifier = modifier) {
        GlassPill(
            text,
            onClick = if (enabled) ({ open = true }) else null,
            icon = icon,
            trailing = if (enabled) {
                {
                    Icon(
                        ExpIcons.uiChevronDown,
                        contentDescription = null,
                        modifier = Modifier.size(10.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
            } else {
                null
            },
            contentDescription = contentDescription,
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEach { option ->
                val glyph = optionIcon?.invoke(option)
                GlassMenuItem(
                    text = { Text(optionLabel(option)) },
                    leadingIcon = when {
                        glyph != null -> {
                            { Icon(glyph, contentDescription = null, modifier = Modifier.size(16.dp)) }
                        }
                        option == selected -> {
                            { Icon(ExpIcons.uiCheck, contentDescription = null, modifier = Modifier.size(16.dp)) }
                        }
                        else -> null
                    },
                    // With a glyph in the leading slot the pick is marked at
                    // the trailing edge instead, so both can show at once.
                    trailingIcon = if (glyph != null && option == selected) {
                        { Icon(ExpIcons.uiCheck, contentDescription = null, modifier = Modifier.size(16.dp)) }
                    } else {
                        null
                    },
                    onClick = {
                        open = false
                        onSelect(option)
                    },
                )
            }
        }
    }
}

/**
 * EXP-825: the `⋯` sheet — the options that did not earn a pill: Effort
 * (Reasoning / Thinking per agent) and Ultracode (claude only — it IS
 * `--effort ultracode`, so it disables the Effort row). No MCP-server picker:
 * mobile has none. EXP-862: the Account moved OUT of here onto the options row
 * itself — which login a run spends is a decision, not an overflow entry.
 */
@Composable
internal fun AgentOptionsSheet(
    launch: LaunchDraft,
    onEffortChange: (String) -> Unit,
    onUltracodeChange: (Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(
        title = "Options",
        onDismiss = onDismiss,
        modifier = Modifier.testTag("agent-options-sheet"),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            OptionGroup {
                PickerRow(
                    label = when (launch.agent) {
                        "codex" -> "Reasoning"
                        else -> "Effort"
                    },
                    value = effortLabel(launch.effort),
                    options = listOf(CLI_DEFAULT_EFFORT) + effortValuesFor(launch.agent),
                    selected = launch.effort,
                    optionLabel = ::effortLabel,
                    // Ultracode IS `--effort ultracode` — it owns the row.
                    enabled = !(launch.agent == DEFAULT_AGENT && launch.ultracode),
                    onSelect = onEffortChange,
                )
                if (launch.agent == DEFAULT_AGENT) {
                    GroupDivider()
                    SwitchRow(
                        title = "Ultracode",
                        checked = launch.ultracode,
                        onCheckedChange = onUltracodeChange,
                    )
                }
            }
            Spacer(Modifier.height(16.dp))
        }
    }
}
