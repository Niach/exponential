package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.ui.theme.TextEmphasis

// The ONE device/agent/model/effort block every launch surface renders
// (EXP-615): the Start-coding sheet's Issues / Actions / Chat tabs, the
// create-action sheet, the automation editor and (EXP-694) the device-settings
// sheet all used to grow their own copy, which is how the dialogs drifted apart
// across the clients. Three variants:
//
//  * [LaunchOptionsVariant.Launch] — a run starting NOW: the machine picker,
//    the agent tabs, model/effort and the launch toggles.
//  * [LaunchOptionsVariant.Automation] — a binding that runs LATER: the same
//    rows minus the toggles. EXP-615 dropped its old "Device default" agent
//    option: the strip seeds to the bound machine's own default launch agent,
//    so every variant renders the SAME three-segment strip and model/effort
//    fall back to the launch "CLI default" sentinel.
//  * [LaunchOptionsVariant.Device] — one machine's stored per-agent defaults:
//    no machine picker (the sheet IS the machine), plus the [accountSlot] with
//    that agent's sign-in and usage.
//
// EXP-694 folds all of it into ONE grouped card: the agent tabs are the card's
// first row (embedded — no capsule of their own), then Model / Effort /
// (Resume) / Ultracode / Plan mode / (account), hairline-divided. The device
// picker keeps its own group above, and groups sit 8dp apart everywhere.

enum class LaunchOptionsVariant { Launch, Automation, Device }

/**
 * The agent strip (brand icon + label per agent) — web/iOS/desktop parity.
 * [embedded] renders it as a grouped card's first row instead of a
 * free-floating capsule (EXP-694).
 */
@Composable
internal fun AgentSegmentedTabs(
    agents: List<String>,
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    embedded: Boolean = false,
) {
    if (agents.isEmpty()) return
    GlassSegmentedControl(
        options = agents,
        selected = selected,
        label = ::agentLabel,
        onSelect = onSelect,
        // EXP-627: the store slide's pop-out rect is measured off the agent
        // strip (`PopRects`), iOS parity.
        modifier = modifier.testTag("start-coding-agent-picker"),
        leadingIcon = { value ->
            Icon(
                painterResource(agentIconRes(value)),
                contentDescription = null,
                modifier = Modifier.size(14.dp),
            )
        },
        embedded = embedded,
    )
}

/**
 * How a machine reads in a picker. EXP-432: a teammate's shared server carries
 * its owner ("buildbox — Danny"), so two similarly named boxes stay tellable
 * apart and the run's host is never a surprise.
 */
internal fun deviceOptionLabel(device: SteerDevice): String {
    val base = device.deviceLabel.ifBlank { device.deviceId }
    return device.owner?.let { "$base — ${it.name}" } ?: base
}

/**
 * The shared options block. [devices] are the candidates already filtered by
 * the host (per-tab caps for a launch, the `automations` cap for a binding);
 * [noDeviceNote] renders instead of the picker when none qualifies. The Device
 * variant renders no machine row at all — the sheet already names it.
 *
 * [resumeSlot] (Launch) and [accountSlot] (Device) are ROWS of the one group,
 * not groups of their own: they are rendered after their own [GroupDivider], so
 * a caller passes bare rows and never its own [OptionGroup].
 */
@Composable
internal fun LaunchOptionsSection(
    variant: LaunchOptionsVariant,
    devices: List<SteerDevice>,
    device: SteerDevice?,
    onDeviceChange: (String) -> Unit,
    agent: String,
    availableAgents: List<String>,
    onAgentChange: (String) -> Unit,
    model: String,
    onModelChange: (String) -> Unit,
    effort: String,
    onEffortChange: (String) -> Unit,
    noDeviceNote: String? = null,
    ultracode: Boolean = false,
    onUltracodeChange: (Boolean) -> Unit = {},
    planMode: Boolean = false,
    onPlanModeChange: (Boolean) -> Unit = {},
    planModeHidden: Boolean = false,
    resumeSlot: (@Composable () -> Unit)? = null,
    accountSlot: (@Composable () -> Unit)? = null,
) {
    val automation = variant == LaunchOptionsVariant.Automation
    val deviceVariant = variant == LaunchOptionsVariant.Device

    // ── Device ───────────────────────────────────────────────────────────────
    // Its own group above the agent card (every client).
    if (!deviceVariant) {
        if (devices.isEmpty()) {
            if (noDeviceNote != null) {
                OptionGroup {
                    Text(
                        noDeviceNote,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Secondary,
                        ),
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                }
                Spacer(Modifier.height(8.dp))
            }
        } else if (automation || devices.size > 1) {
            // A single launch candidate needs no picker; a binding always names
            // the machine it will fire on.
            OptionGroup {
                PickerRow(
                    // A binding row says where it fires; a launch picks the
                    // device — same wording as web/iOS/desktop on both.
                    label = if (automation) "Runs on" else "Device",
                    value = device?.let(::deviceOptionLabel) ?: "Select",
                    options = devices.map { it.deviceId },
                    selected = device?.deviceId,
                    optionLabel = { id ->
                        devices.firstOrNull { it.deviceId == id }?.let(::deviceOptionLabel) ?: id
                    },
                    onSelect = onDeviceChange,
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }

    // ── The agent card ───────────────────────────────────────────────────────
    // EXP-694: tabs + model/effort + toggles + account are ONE inset-grouped
    // card on all four clients.
    val showsPlanMode = !automation && supportsPlanMode(agent) && !planModeHidden
    OptionGroup {
        // A lone option is not a choice — every variant hides the strip then.
        if (availableAgents.size > 1) {
            AgentSegmentedTabs(
                agents = availableAgents,
                selected = agent,
                onSelect = onAgentChange,
                modifier = Modifier.padding(8.dp),
                embedded = true,
            )
            GroupDivider()
        }

        // Every variant speaks the launch "CLI default" sentinel (EXP-615); a
        // binding offers it for EVERY agent, because a blank pin is what stores
        // NULL on the row and lets the machine decide.
        PickerRow(
            label = "Model",
            value = modelLabel(model),
            options = if (automation) {
                listOf(CLI_DEFAULT_MODEL) + modelValuesFor(agent)
            } else {
                modelOptionsFor(agent)
            },
            selected = model,
            optionLabel = ::modelLabel,
            onSelect = onModelChange,
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
            enabled = automation || !ultracode,
            onSelect = onEffortChange,
        )

        // ── Toggles ──────────────────────────────────────────────────────────
        // claude gets Ultracode + Plan mode, pi just Plan mode (EXP-441 — pi
        // stays otherwise unguarded), codex neither; a binding gets none. Plan
        // mode is hidden entirely while resuming — a resume never re-enters it
        // (EXP-202, desktop parity).
        if (!automation) {
            if (resumeSlot != null) {
                GroupDivider()
                resumeSlot()
            }
            if (agent == DEFAULT_AGENT) {
                GroupDivider()
                SwitchRow(
                    title = "Ultracode",
                    checked = ultracode,
                    onCheckedChange = onUltracodeChange,
                )
            }
            if (showsPlanMode) {
                GroupDivider()
                SwitchRow(
                    title = "Plan mode",
                    checked = planMode,
                    onCheckedChange = onPlanModeChange,
                )
            }
        }

        // EXP-688/EXP-694: the machine's sign-in and usage for THIS agent are
        // the card's last rows — a standalone "Agents" section repeated the
        // agent list a second time.
        if (accountSlot != null) {
            GroupDivider()
            accountSlot()
        }
    }
}

// The agents a desktop can launch, in contract order. No device settled yet
// means the pre-EXP-201 default (claude); a device answers for itself, and
// EXP-409 lets that answer be EMPTY — a machine whose agents are all signed
// out runs nothing, so it never becomes a candidate in the first place.
internal fun availableAgentsFor(device: SteerDevice?): List<String> =
    device?.runnableAgents ?: listOf(DEFAULT_AGENT)

/** Every launch option for one agent, ready to drop into the sheet's state. */
internal data class AgentSeed(
    val model: String,
    val effort: String,
    val ultracode: Boolean,
    val planMode: Boolean,
)

/**
 * Which agent a machine starts on (EXP-437): the one it has configured as its
 * default, clamped to what it can actually run. A machine that advertises no
 * default (or an unrunnable one) falls back to claude, then to whatever it
 * runs first — the pre-EXP-437 behavior.
 */
internal fun defaultAgentFor(device: SteerDevice?): String {
    val available = availableAgentsFor(device)
    return device?.launchDefaults?.defaultAgent?.takeIf { it in available }
        ?: DEFAULT_AGENT.takeIf { it in available }
        ?: available.firstOrNull()
        ?: DEFAULT_AGENT
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
internal fun agentSeed(device: SteerDevice?, agent: String): AgentSeed {
    val defaults = device?.launchDefaults?.agents?.get(agent)
        ?: return AgentSeed(defaultModelFor(agent), CLI_DEFAULT_EFFORT, false, false)
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
    )
}
