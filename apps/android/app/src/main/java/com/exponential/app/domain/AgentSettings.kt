package com.exponential.app.domain

/**
 * EXP-1291 Threads v2: Team settings → Agents — the Android mirror of
 * `packages/domain-contract/src/agent-settings.ts` (desktop
 * `domain::agent_settings`, iOS `AgentSettings.swift`). Pure and byte-locked
 * by `fixtures/agent-settings.json` (`summaryCases`, `validationCases`),
 * which `AgentSettingsTest` replays; the copy below is the fixture's.
 *
 * The list = Settings rows (icon + color + name like a board row, then
 * `New agent`); an agent page = the board page layout, [FIELDS] top to
 * bottom. Save = `sessions.send({teamId, deviceId, account, agent})` for a
 * new agent, `sessions.updateAgent` after. Owner-only controls hide from
 * members.
 */
object AgentSettings {

    /** One page field: [key] identity | device | account | schedule |
     *  instructions | delete; [control] names the widget. */
    data class Field(
        val key: String,
        val label: String,
        val control: String,
        val required: Boolean,
        val ownerOnly: Boolean = false,
    )

    const val FIELD_IDENTITY = "identity"
    const val FIELD_DEVICE = "device"
    const val FIELD_ACCOUNT = "account"
    const val FIELD_SCHEDULE = "schedule"
    const val FIELD_INSTRUCTIONS = "instructions"
    const val FIELD_DELETE = "delete"

    val FIELDS: List<Field> = listOf(
        Field(FIELD_IDENTITY, "Name", "icon-color-name", required = true),
        Field(FIELD_DEVICE, "Device", "device-picker", required = true),
        Field(FIELD_ACCOUNT, "Account", "account-picker", required = false),
        Field(FIELD_SCHEDULE, "Schedule", "trigger-list", required = false),
        Field(FIELD_INSTRUCTIONS, "Instructions", "markdown", required = false),
        Field(FIELD_DELETE, "Delete agent", "danger", required = false, ownerOnly = true),
    )

    object Copy {
        const val TITLE = "Agents"
        const val NEW_AGENT = "New agent"
        const val NAME_PLACEHOLDER = "Agent name"
        const val INSTRUCTIONS_PLACEHOLDER = "What this agent takes care of…"
        const val ADD_TRIGGER = "Add"
        const val NO_SCHEDULE = "Waits for messages"
        const val PAUSED = "Paused"
        const val DELETE_CONFIRM = "Delete this agent? Its conversation is deleted with it."
        val WEEKDAYS: List<String> = listOf(
            "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
        )
        const val SCHEDULE_DAILY = "Daily at {time}"
        const val SCHEDULE_WEEKLY = "Weekly on {weekday} at {time}"
        const val SCHEDULE_MONTHLY = "Monthly on day {day} at {time}"
        /** Event summaries by contract `agentTriggerEventValues`. */
        val EVENTS: Map<String, String> = mapOf(
            "created" to "When an issue is created",
            "status_changed" to "When an issue's status changes",
            "assignee_changed" to "When an issue is assigned",
            "label_added" to "When a label is added",
            "priority_changed" to "When an issue's priority changes",
            "pr_opened" to "When a pull request opens",
            "pr_merged" to "When a pull request merges",
        )
        const val ERROR_NAME_REQUIRED = "Name the agent"
        const val ERROR_NAME_TOO_LONG = "Use at most 64 characters"
        const val ERROR_DEVICE_REQUIRED = "Pick a device"
        const val ERROR_INSTRUCTIONS_TOO_LONG = "Instructions are too long"
    }

    const val TEST_ID_LIST = "agents-settings-list"
    const val TEST_ID_PAGE = "agent-settings-page"

    /** The trigger fields [triggerSummary] reads (a stored agent trigger or
     *  the fixture's case). */
    data class TriggerLike(
        val kind: String,
        val enabled: Boolean? = null,
        val interval: String? = null,
        val minuteOfDay: Int? = null,
        val weekday: Int? = null,
        val dayOfMonth: Int? = null,
        val event: String? = null,
    )

    private fun clock(minuteOfDay: Int?): String {
        val minutes = (minuteOfDay ?: 0).coerceIn(0, 1439)
        fun pad(value: Int) = value.toString().padStart(2, '0')
        return "${pad(minutes / 60)}:${pad(minutes % 60)}"
    }

    /** One Schedule row's text. */
    fun triggerSummary(trigger: TriggerLike): String {
        val text = if (trigger.kind == "schedule") {
            val time = clock(trigger.minuteOfDay)
            when (trigger.interval) {
                "weekly" -> {
                    val index = Math.floorMod((trigger.weekday ?: 1) - 1 + 7, 7)
                    Copy.SCHEDULE_WEEKLY.replace("{weekday}", Copy.WEEKDAYS[index]).replace("{time}", time)
                }
                "monthly" -> Copy.SCHEDULE_MONTHLY
                    .replace("{day}", (trigger.dayOfMonth ?: 1).toString())
                    .replace("{time}", time)
                else -> Copy.SCHEDULE_DAILY.replace("{time}", time)
            }
        } else {
            Copy.EVENTS[trigger.event ?: ""] ?: trigger.event ?: ""
        }
        return if (trigger.enabled == false) "$text · ${Copy.PAUSED}" else text
    }

    /** A stored agent trigger as [triggerSummary] reads it. */
    fun triggerSummary(trigger: AgentTrigger): String = triggerSummary(
        when (val part = trigger.whenPart) {
            is TriggerWhen.Schedule -> TriggerLike(
                kind = "schedule",
                enabled = trigger.enabled,
                interval = part.interval,
                minuteOfDay = part.minuteOfDay,
                weekday = part.weekday,
                dayOfMonth = part.dayOfMonth,
            )
            is TriggerWhen.Event -> TriggerLike(kind = "event", enabled = trigger.enabled, event = part.event)
        },
    )

    data class Form(val name: String, val deviceId: String?, val instructions: String)

    data class FormError(val field: String, val message: String)

    /** Field errors in field order; empty = valid. */
    fun validateAgentForm(form: Form): List<FormError> {
        val errors = mutableListOf<FormError>()
        val name = form.name.trim()
        if (name.isEmpty()) {
            errors.add(FormError(FIELD_IDENTITY, Copy.ERROR_NAME_REQUIRED))
        } else if (name.codePointCount(0, name.length) > DomainContract.sessionsSendAgentNameMax) {
            errors.add(FormError(FIELD_IDENTITY, Copy.ERROR_NAME_TOO_LONG))
        }
        if (form.deviceId.isNullOrEmpty()) errors.add(FormError(FIELD_DEVICE, Copy.ERROR_DEVICE_REQUIRED))
        if (form.instructions.toByteArray(Charsets.UTF_8).size > DomainContract.sessionsSendAgentInstructionsMaxBytes) {
            errors.add(FormError(FIELD_INSTRUCTIONS, Copy.ERROR_INSTRUCTIONS_TOO_LONG))
        }
        return errors
    }
}
