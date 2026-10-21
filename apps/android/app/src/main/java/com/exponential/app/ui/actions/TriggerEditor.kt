package com.exponential.app.ui.actions

import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.TriggerWhen
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.TextFieldRow

// The trigger editor's fields (EXP-583; EXP-1291: triggers live on the team
// AGENT, whose device/account/model are the runner): ONE draft and its
// when-part card (Schedule | On event), rendered by the agent page's
// Triggers "Add" / row tap sheet ([TriggerFormSheet]). NEW-2 (web
// `trigger-fields.tsx`): a schedule = ONE 5-field cron + its IANA zone, an
// event = ONE event name (the contract builtins offered, any custom name
// allowed). The server fires them; no filters.

internal const val TRIGGER_KIND_SCHEDULE = "schedule"
internal const val TRIGGER_KIND_EVENT = "event"

/** The cron field's placeholder (web `CRON_PLACEHOLDER`). */
internal const val CRON_PLACEHOLDER = "0 9 * * 1-5"

/** The editor state; [triggerDraftToWhen] converts at submit (an invalid or
 *  unknown draft reads as null). */
internal data class TriggerDraft(
    val kind: String = TRIGGER_KIND_SCHEDULE,
    /** `m h dom mon dow`. */
    val cron: String = "",
    /** IANA zone; blank = UTC. */
    val tz: String = java.util.TimeZone.getDefault().id,
    /** The events row name. */
    val name: String = "",
)

private val CRON_SHAPE = Regex("^(\\S+\\s+){4}\\S+$")
private val EVENT_NAME_SHAPE = Regex("^[A-Za-z0-9][A-Za-z0-9._:/-]*$")

/** A draft seeded from an existing [trigger]; null seeds an empty schedule. */
internal fun triggerDraftFor(trigger: TriggerWhen?): TriggerDraft = when (trigger) {
    null -> TriggerDraft()
    is TriggerWhen.Schedule -> TriggerDraft(kind = TRIGGER_KIND_SCHEDULE, cron = trigger.cron, tz = trigger.tz.orEmpty())
    is TriggerWhen.Event -> TriggerDraft(kind = TRIGGER_KIND_EVENT, name = trigger.name)
}

/** Whether [tz] names a zone this JVM knows (blank = UTC, valid). */
private fun validTimeZone(tz: String): Boolean =
    tz.isEmpty() || (tz.length <= DomainContract.agentTriggerTzMax && runCatching { java.time.ZoneId.of(tz) }.isSuccess)

/**
 * The draft's when-part, or null while it does not satisfy the server's
 * strict write schema (`cronSchema`, `timeZoneSchema`, `eventNameSchema`).
 */
internal fun triggerDraftToWhen(draft: TriggerDraft): TriggerWhen? =
    when (draft.kind) {
        TRIGGER_KIND_SCHEDULE -> {
            val cron = draft.cron.trim().replace(Regex("\\s+"), " ")
            val tz = draft.tz.trim()
            if (cron.length !in 9..DomainContract.agentTriggerCronMax || !CRON_SHAPE.matches(cron) || !validTimeZone(tz)) {
                null
            } else {
                TriggerWhen.Schedule(cron = cron, tz = tz.ifEmpty { null })
            }
        }
        TRIGGER_KIND_EVENT -> {
            val name = draft.name.trim()
            if (name.isEmpty() || name.length > DomainContract.eventNameMax || !EVENT_NAME_SHAPE.matches(name)) {
                null
            } else {
                TriggerWhen.Event(name)
            }
        }
        else -> null
    }

/** The label a builtin event name reads as (else the name itself). */
private fun eventLabel(name: String): String = AgentSettings.Copy.EVENTS[name] ?: name

/**
 * The when-part half of the editor, as ONE grouped card (EXP-698): the
 * segmented kind switch is its first row, and under a hairline come the picked
 * kind's rows — the schedule pane (cron + time zone) or the event pane (the
 * builtin picker + the free-text name).
 */
@Composable
internal fun TriggerWhenFields(
    draft: TriggerDraft,
    onChange: (TriggerDraft) -> Unit,
) {
    // EXP-698: ONE card, exactly like the agent card in
    // [LaunchOptionsSection] — the kind strip is the card's FIRST ROW and the
    // picked kind's rows follow it under a hairline.
    OptionGroup {
        GlassSegmentedControl(
            options = listOf(TRIGGER_KIND_SCHEDULE, TRIGGER_KIND_EVENT),
            selected = draft.kind,
            label = { if (it == TRIGGER_KIND_EVENT) "On event" else "Schedule" },
            onSelect = { onChange(draft.copy(kind = it)) },
            modifier = Modifier.padding(8.dp),
            embedded = true,
        )

        if (draft.kind == TRIGGER_KIND_SCHEDULE) {
            GroupDivider()
            TextFieldRow(
                label = "Cron",
                value = draft.cron,
                onValueChange = { onChange(draft.copy(cron = it.take(DomainContract.agentTriggerCronMax))) },
                placeholder = CRON_PLACEHOLDER,
                modifier = Modifier.testTag("trigger-cron"),
            )
            GroupDivider()
            TextFieldRow(
                label = "Time zone",
                value = draft.tz,
                onValueChange = { onChange(draft.copy(tz = it.take(DomainContract.agentTriggerTzMax))) },
                placeholder = "UTC",
                modifier = Modifier.testTag("trigger-tz"),
            )
        } else if (draft.kind == TRIGGER_KIND_EVENT) {
            GroupDivider()
            val names = buildList {
                addAll(DomainContract.eventBuiltinNames)
                val typed = draft.name.trim()
                if (typed.isNotEmpty() && typed !in DomainContract.eventBuiltinNames) add(0, typed)
            }
            PickerRow(
                label = "When",
                value = draft.name.trim().takeIf { it.isNotEmpty() }?.let(::eventLabel) ?: "Pick an event",
                options = names,
                selected = draft.name.trim(),
                optionLabel = ::eventLabel,
                onSelect = { onChange(draft.copy(name = it)) },
            )
            GroupDivider()
            TextFieldRow(
                label = "Event name",
                value = draft.name,
                onValueChange = { onChange(draft.copy(name = it.take(DomainContract.eventNameMax))) },
                placeholder = "deploy:prod",
                modifier = Modifier.testTag("trigger-event-name"),
            )
        }
    }
}
