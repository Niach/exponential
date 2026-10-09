package com.exponential.app.ui.agent

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceWorktreeEntity
import com.exponential.app.domain.AccountOption
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GlassSwitchSize
import com.exponential.app.ui.components.picker.AccountPicker
import com.exponential.app.ui.components.picker.AccountPickerPillTrigger
import com.exponential.app.ui.components.toPickerAccount
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.deviceIcon
import com.exponential.app.ui.components.picker.DevicePicker
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.components.deviceOptionLabel
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.components.modelOptionsFor
import com.exponential.app.ui.components.supportsPlanMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-825: the launch options as ONE muted inline line under the composer
 * card (Danny's variant B): Device, Account, Model, a Plan switch and the
 * Resume switch inline while a worktree makes it offerable (EXP-481).
 * EXP-1249: the `⋯` overflow is gone — Effort, Subagents, Ultracode, MCP
 * servers and Computer use live in the composer's "+" menu.
 * Every pill is a menu or a toggle — no disabled controls; the caption under
 * the row explains what cannot start. Horizontally scrolling: a phone cannot
 * fit six pills, and wrapping would push the sessions list around.
 *
 * EXP-872: there is no agent pill any more — the ACCOUNT picker carries the
 * brand mark, and picking a login implies its agent. EXP-993: and no
 * repository pill: mobile never showed the chat's optional anchor well, so a
 * chat simply takes the team's first repository.
 */
@Composable
internal fun AgentOptionsRow(
    devices: List<SteerDevice>,
    device: SteerDevice?,
    onDeviceChange: (String) -> Unit,
    launch: LaunchDraft,
    /** EXP-872: the settled machine's logins, last used first. */
    accountOptions: List<AccountOption>,
    /** A picked login sets the agent AND the account in one go. */
    onAccountChange: (AccountOption) -> Unit,
    onModelChange: (String) -> Unit,
    onPlanModeChange: (Boolean) -> Unit,
    resumeCandidate: DeviceWorktreeEntity?,
    resume: Boolean,
    onResumeChange: (Boolean) -> Unit,
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
            // EXP-1030: the shared device picker — the machine glyph and its
            // owner's name are `devicePickerItems`' job now (EXP-924: the
            // owner's pick, else the kind default), and the pill is its
            // trigger.
            DevicePicker(
                devices = devices.map { row ->
                    DevicePickerDevice(
                        id = row.deviceId,
                        name = deviceOptionLabel(row),
                        icon = row.icon,
                        isServer = row.isServer,
                    )
                },
                value = device.deviceId,
                onChange = onDeviceChange,
                trigger = { open ->
                    OptionPill(
                        text = deviceOptionLabel(device),
                        contentDescription = "Device",
                        icon = deviceIcon(device),
                        // A lone machine is a statement, not a choice.
                        enabled = devices.size > 1,
                        onOpen = open,
                        modifier = Modifier.testTag("agent-device-pill"),
                    )
                },
            )
        }
        // EXP-872: the ACCOUNT — the ONE picker every launch surface renders.
        // The brand mark plus the login's email; picking one implies its
        // agent, so there is no agent pill beside it.
        // EXP-642: the store slide's pop-out rect used to be measured off the
        // segmented strip this replaced, so the testTag stays on this pill.
        val account = accountOptions.firstOrNull { it.key == launch.accountKey } ?: accountOptions.firstOrNull()
        if (account != null) {
            AccountPicker(
                options = accountOptions.map { it.toPickerAccount() },
                value = account.key,
                onChange = { key -> accountOptions.firstOrNull { it.key == key }?.let(onAccountChange) },
                trigger = { open ->
                    AccountPickerPillTrigger(
                        option = account.toPickerAccount(),
                        onOpen = if (accountOptions.size > 1) open else null,
                        modifier = Modifier.testTag("start-coding-agent-picker"),
                    )
                },
            )
        }
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
                contentDescription = "Resume previous run",
            )
        }
    }
}

/**
 * EXP-827: the Plan switch on the row's capsule. The whole pill is the tap
 * target and reads as ONE switch to TalkBack; the M3 switch inside is a pure
 * indicator ([GlassSwitch]), scaled to sit in the 28dp capsule.
 */
@Composable
private fun PlanSwitchPill(
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    GlassPill(
        "Plan",
        onClick = { onCheckedChange(!checked) },
        trailing = { GlassSwitch(checked = checked, onCheckedChange = null, size = GlassSwitchSize.Pill) },
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
        OptionPill(
            text = text,
            contentDescription = contentDescription,
            icon = icon,
            enabled = enabled,
            onOpen = { open = true },
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
 * The capsule every option pill wears: a glyph, the picked value and the
 * chevron that says it opens something. [enabled] false renders the label
 * alone — a single value is a statement, not a choice. Shared by the pills
 * that open a MENU ([OptionMenuPill]) and by the device pill, whose list is
 * the shared picker's sheet.
 */
@Composable
private fun OptionPill(
    text: String,
    contentDescription: String,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    enabled: Boolean = true,
) {
    GlassPill(
        text,
        onClick = if (enabled) onOpen else null,
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
        modifier = modifier,
    )
}
