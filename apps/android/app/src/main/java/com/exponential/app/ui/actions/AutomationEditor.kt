package com.exponential.app.ui.actions

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SYSTEM_PROFILE_ID
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.AutomationTriggerFilters
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.triggerEventLabel
import com.exponential.app.domain.triggerWeekdayName
import com.exponential.app.ui.components.AccountPickerPill
import com.exponential.app.ui.components.CLI_DEFAULT_EFFORT
import com.exponential.app.ui.components.CLI_DEFAULT_MODEL
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.LaunchOptionsSection
import com.exponential.app.ui.components.LaunchOptionsVariant
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.accountOptionsFor
import com.exponential.app.ui.components.ambientAccountOptions
import com.exponential.app.ui.components.availableAgentsFor
import com.exponential.app.ui.components.defaultAgentFor
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.agent.StartBoardOption
import com.exponential.app.ui.agent.StartFilterOption
import com.exponential.app.ui.theme.TextEmphasis

// The shared automation editor (EXP-583). TWO hosts render these fields: the
// Automations tab's "New automation" / "Edit automation" sheet (where a
// trigger is mandatory) and the create-action sheet's Automation detail, which
// starts on "None" and, once set, becomes the note the creator agent copies
// into `exponential_automations_create` — built from this same draft. Keeping
// one draft + one set of rows is what stops the two from drifting.

/** The default schedule time, matching the web draft (09:00). */
private const val DEFAULT_MINUTE_OF_DAY = 540

/** Kind sentinel for "no automation" — only the create-action host offers it. */
internal const val AUTOMATION_KIND_NONE = "none"
internal const val AUTOMATION_KIND_SCHEDULE = "schedule"
internal const val AUTOMATION_KIND_EVENT = "event"

/**
 * The editor state. A draft can represent things a valid trigger cannot (an
 * event kind with no machine bound yet), so [automationDraftToTrigger]
 * converts at submit and incomplete reads as null. The filter picks are
 * SINGLE-select with an "Any" default — the mobile simplification of the web's
 * multi-selects; the wire lists carry one-or-none entries from here.
 * [agent] is seeded from the bound machine's default (EXP-615 — empty only
 * before one is bound); EXP-995: [account] is the agent profile the run
 * spends, picked WITH its agent off the account row ("" = the machine's
 * default login, saved as NULL); an empty [model]/[effort] is the "CLI
 * default" that saves as NULL.
 */
internal data class AutomationDraft(
    val kind: String = AUTOMATION_KIND_SCHEDULE,
    /** Steer deviceId of the bound machine; null = nothing picked yet. */
    val deviceId: String? = null,
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
    val agent: String = "",
    val account: String = "",
    val model: String = CLI_DEFAULT_MODEL,
    val effort: String = CLI_DEFAULT_EFFORT,
)

/** `HH:MM` → minutes past midnight; malformed/out-of-range reads 09:00 (the
 * web `timeToMinute` fallback). */
internal fun automationTimeToMinute(time: String): Int {
    val match = Regex("^(\\d{1,2}):(\\d{2})$").find(time.trim()) ?: return DEFAULT_MINUTE_OF_DAY
    val minute = match.groupValues[1].toInt() * 60 + match.groupValues[2].toInt()
    return if (minute in 0..1439) minute else DEFAULT_MINUTE_OF_DAY
}

/** Minutes past midnight → the `HH:MM` the time field renders. */
internal fun automationMinuteToTime(minuteOfDay: Int): String =
    String.format(java.util.Locale.ROOT, "%02d:%02d", minuteOfDay / 60, minuteOfDay % 60)

/**
 * A draft seeded from an existing/suggested [trigger] (the suggestion host);
 * null seeds the plain schedule default. [deviceId] pre-picks the machine.
 */
internal fun automationDraftFor(
    trigger: AutomationTrigger?,
    deviceId: String? = null,
): AutomationDraft = when (trigger) {
    null -> AutomationDraft(deviceId = deviceId)
    is AutomationTrigger.Schedule -> AutomationDraft(
        kind = AUTOMATION_KIND_SCHEDULE,
        deviceId = deviceId,
        interval = trigger.interval,
        time = automationMinuteToTime(trigger.minuteOfDay),
        weekday = trigger.weekday ?: 1,
        dayOfMonth = trigger.dayOfMonth ?: 1,
    )
    is AutomationTrigger.Event -> AutomationDraft(
        kind = AUTOMATION_KIND_EVENT,
        deviceId = deviceId,
        event = trigger.event,
        boardId = trigger.filters.boardIds.firstOrNull().orEmpty(),
        labelId = trigger.filters.labelIds.firstOrNull().orEmpty(),
        priority = trigger.filters.priorities.firstOrNull().orEmpty(),
        toStatusId = trigger.filters.toStatusIds.firstOrNull().orEmpty(),
    )
}

/**
 * The draft's when-part trigger, or null when the kind is "none". Filters
 * irrelevant to the picked event are dropped, matching the server's strict
 * write schema.
 */
internal fun automationDraftToTrigger(draft: AutomationDraft): AutomationTrigger? =
    when (draft.kind) {
        AUTOMATION_KIND_SCHEDULE -> AutomationTrigger.Schedule(
            interval = draft.interval,
            minuteOfDay = automationTimeToMinute(draft.time),
            weekday = draft.weekday.takeIf { draft.interval == "weekly" },
            dayOfMonth = draft.dayOfMonth.takeIf { draft.interval == "monthly" },
        )
        AUTOMATION_KIND_EVENT -> AutomationTrigger.Event(
            event = draft.event,
            filters = AutomationTriggerFilters(
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

private fun intervalLabel(interval: String): String = when (interval) {
    "weekly" -> "Week"
    "monthly" -> "Month"
    else -> "Day"
}

/**
 * The trigger half of the editor, as ONE grouped card (EXP-698): the segmented
 * kind switch (with a leading "None" only when [allowNone]) is its first row,
 * and under a hairline come the picked kind's rows — the schedule pane
 * (interval + contextual weekday/day pickers + the wall-clock row) or the
 * event pane (event picker + contextual filter pickers off the synced
 * tables). "None" leaves the strip alone in the card.
 */
@Composable
internal fun AutomationTriggerFields(
    draft: AutomationDraft,
    boards: List<StartBoardOption>,
    labels: List<StartFilterOption>,
    statuses: List<StartFilterOption>,
    onChange: (AutomationDraft) -> Unit,
    allowNone: Boolean = false,
) {
    // EXP-698: ONE card, exactly like the agent card in
    // [LaunchOptionsSection] — the kind strip is the card's FIRST ROW and the
    // picked kind's rows follow it under a hairline, instead of a loose
    // capsule floating above a second, separate group.
    OptionGroup {
        GlassSegmentedControl(
            options = if (allowNone) {
                listOf(AUTOMATION_KIND_NONE, AUTOMATION_KIND_SCHEDULE, AUTOMATION_KIND_EVENT)
            } else {
                listOf(AUTOMATION_KIND_SCHEDULE, AUTOMATION_KIND_EVENT)
            },
            selected = draft.kind,
            label = {
                when (it) {
                    AUTOMATION_KIND_SCHEDULE -> "Schedule"
                    AUTOMATION_KIND_EVENT -> "On event"
                    else -> "None"
                }
            },
            onSelect = { onChange(draft.copy(kind = it)) },
            modifier = Modifier.padding(8.dp),
            embedded = true,
        )

        if (draft.kind == AUTOMATION_KIND_SCHEDULE) {
            GroupDivider()
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
            AutomationTimeRow(
                time = draft.time,
                onChange = { onChange(draft.copy(time = it)) },
            )
        } else if (draft.kind == AUTOMATION_KIND_EVENT) {
            GroupDivider()
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
}

/**
 * Seed (and re-seed) the pin off the bound machine — what the editor SHOWS is
 * what Save STORES. Null = leave the draft alone.
 *
 *  - An unset pin (a row saved before EXP-615 carries a NULL agent) or one
 *    [bound] cannot run falls back to that machine's default ACCOUNT, which
 *    names the agent, clamped to what it advertises; model/effort reset to
 *    the "CLI default" blank since their vocabularies are per agent.
 *  - A runnable pin on the SAME machine is left alone, so a manual pick sticks.
 *  - EXP-995: a profile id is DEVICE-LOCAL, so on a switch ([deviceSwitched])
 *    a runnable pin moves onto the new machine's default login of the SAME
 *    agent (its stored default account, else that agent's active login —
 *    [accountOptionsFor] lists it first), keeping model/effort; an agent the
 *    new machine reports no login for falls back to its default account.
 */
internal fun seedAutomationPin(
    draft: AutomationDraft,
    bound: SteerDevice,
    deviceSwitched: Boolean,
): AutomationDraft? {
    val agents = availableAgentsFor(bound)
    val options = accountOptionsFor(bound, agents)
    val runnable = draft.agent.isNotEmpty() && draft.agent in agents
    if (runnable && !deviceSwitched) return null
    if (runnable) {
        val sameAgent = options.firstOrNull { it.agent == draft.agent }
        if (sameAgent != null) {
            return draft.copy(account = sameAgent.id.takeIf { it != SYSTEM_PROFILE_ID }.orEmpty())
        }
    }
    val fallback = options.firstOrNull { it.isDeviceDefault } ?: options.firstOrNull()
    return draft.copy(
        agent = fallback?.agent ?: defaultAgentFor(bound),
        account = fallback?.id?.takeIf { it != SYSTEM_PROFILE_ID }.orEmpty(),
        model = CLI_DEFAULT_MODEL,
        effort = CLI_DEFAULT_EFFORT,
    )
}

/**
 * The binding half: which machine runs the automation, and the account pin
 * with its model/effort. EXP-615 retired the "Device default" agent option;
 * EXP-995 retired the agent strip itself — the card's first row is THE
 * account picker ([AccountPickerPill], brand mark + email over the bound
 * machine's logins, its default first) and a pick names the agent too. The
 * pin seeds to the bound machine's DEFAULT ACCOUNT (the composer's seed) and
 * the row saves that concrete agent + profile. Model/Effort speak the launch
 * "CLI default" sentinel, which is what writes NULL.
 */
@Composable
internal fun AutomationBindingFields(
    draft: AutomationDraft,
    devices: List<SteerDevice>,
    onChange: (AutomationDraft) -> Unit,
) {
    val device = devices.firstOrNull { it.deviceId == draft.deviceId }
    // EXP-995: every signed-in login the bound machine reports, across
    // agents; a machine that reports none (or none bound yet) offers one
    // ambient row per runnable agent so the pin can be made before the
    // heartbeat lands (the workflow runner block's rule).
    val accountOptions = if (device == null) {
        ambientAccountOptions(DomainContract.codingAgentValues, draft.agent.takeIf { it.isNotEmpty() })
    } else {
        accountOptionsFor(device, availableAgentsFor(device))
    }
    // The machine the draft's pin was made on: an edited row's stored pin
    // belongs to its stored machine (never re-seeded on open), a fresh draft's
    // to nothing yet. Only a REAL switch away from it re-seeds a runnable pin —
    // the pool arriving late, or a heartbeat re-listing the same machine, is
    // not a switch.
    var pinDeviceId by remember { mutableStateOf(draft.deviceId) }
    LaunchedEffect(device?.deviceId, devices) {
        val bound = device ?: return@LaunchedEffect
        val switched = pinDeviceId != null && pinDeviceId != bound.deviceId
        pinDeviceId = bound.deviceId
        seedAutomationPin(draft, bound, deviceSwitched = switched)?.let(onChange)
    }
    // EXP-615: the same block the launch dialogs render, in its Automation
    // variant — no launch toggles; EXP-995: the account row leads it.
    LaunchOptionsSection(
        variant = LaunchOptionsVariant.Automation,
        devices = devices,
        device = device,
        // The re-seed above moves the pin onto the new machine (its own
        // profile for the same agent, else its default account).
        onDeviceChange = { id -> onChange(draft.copy(deviceId = id)) },
        agent = draft.agent,
        availableAgents = device?.runnableAgents.orEmpty(),
        onAgentChange = { next ->
            // A profile is ONE agent's, and the model/effort vocabularies are
            // per agent — a switch has to reset them, or a stale value hits a
            // server refusal.
            onChange(
                draft.copy(
                    agent = next,
                    account = "",
                    model = CLI_DEFAULT_MODEL,
                    effort = CLI_DEFAULT_EFFORT,
                ),
            )
        },
        accountRow = if (accountOptions.isEmpty()) {
            null
        } else {
            {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        "Account",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurface.copy(
                            alpha = TextEmphasis.Primary,
                        ),
                    )
                    Spacer(Modifier.weight(1f))
                    AccountPickerPill(
                        options = accountOptions,
                        selectedKey = "${draft.agent}:${draft.account.ifEmpty { SYSTEM_PROFILE_ID }}",
                        onSelect = { option ->
                            // Only an AGENT change invalidates the vocabularies
                            // below; another login of the same agent keeps
                            // model and effort exactly as picked. "" is the
                            // machine's ambient login, never an id on the wire.
                            val agentChanged = option.agent != draft.agent
                            onChange(
                                draft.copy(
                                    agent = option.agent,
                                    account = option.id.takeIf { it != SYSTEM_PROFILE_ID }.orEmpty(),
                                    model = if (agentChanged) CLI_DEFAULT_MODEL else draft.model,
                                    effort = if (agentChanged) CLI_DEFAULT_EFFORT else draft.effort,
                                ),
                            )
                        },
                        modifier = Modifier.testTag("automation-account-pill"),
                    )
                }
            }
        },
        model = draft.model,
        onModelChange = { onChange(draft.copy(model = it)) },
        effort = draft.effort,
        onEffortChange = { onChange(draft.copy(effort = it)) },
        // One wording with the web dialog (EXP-615).
        noDeviceNote = "No automation-capable device. Run the desktop app or the " +
            "exponential daemon and it will appear here.",
    )
}

/**
 * The schedule's wall-clock row: a PickerRow-shaped row that opens the
 * Material 3 time picker (EXP-615 — the free-text HH:MM field accepted
 * nonsense and read nothing like the iOS `DatePicker(.hourAndMinute)`). The
 * draft keeps its `"HH:MM"` string, so the wire format is unchanged.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AutomationTimeRow(time: String, onChange: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { open = true }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "Time",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        Text(
            time,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
    if (open) {
        val minuteOfDay = automationTimeToMinute(time)
        val state = rememberTimePickerState(
            initialHour = minuteOfDay / 60,
            initialMinute = minuteOfDay % 60,
            // The stored string is 24h — showing a 12h dial would disagree
            // with the row's own value.
            is24Hour = true,
        )
        AlertDialog(
            onDismissRequest = { open = false },
            text = { TimePicker(state = state) },
            confirmButton = {
                TextButton(
                    onClick = {
                        open = false
                        onChange(automationMinuteToTime(state.hour * 60 + state.minute))
                    },
                ) { Text("Done") }
            },
            dismissButton = {
                TextButton(onClick = { open = false }) { Text("Cancel") }
            },
        )
    }
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
