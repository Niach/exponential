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
 * bottom. Save = `agents.create` for a new agent, `agents.update` after
 * (NEW-2: an agent is a definition row). Owner-only controls hide from
 * members.
 */
object AgentSettings {

    /** One page field: [key] identity | device | account | triggers |
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
    const val FIELD_TRIGGERS = "triggers"
    const val FIELD_INSTRUCTIONS = "instructions"
    const val FIELD_DELETE = "delete"

    val FIELDS: List<Field> = listOf(
        Field(FIELD_IDENTITY, "Name", "icon-color-name", required = true),
        Field(FIELD_DEVICE, "Device", "device-picker", required = true),
        Field(FIELD_ACCOUNT, "Account", "account-picker", required = false),
        Field(FIELD_TRIGGERS, "Triggers", "trigger-list", required = false),
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
        const val SCHEDULE_CRON = "Cron {cron}"
        const val SCHEDULE_CRON_TZ = "Cron {cron} ({tz})"
        /** Event summaries by contract `eventBuiltinNames`. */
        val EVENTS: Map<String, String> = mapOf(
            "issue.created" to "When an issue is created",
            "issue.status_changed" to "When an issue's status changes",
            "pr.opened" to "When a pull request opens",
            "pr.merged" to "When a pull request merges",
            "session.ended" to "When a session ends",
        )
        /** Any other (custom) event name. */
        const val EVENT = "On {name}"
        const val ERROR_NAME_REQUIRED = "Name the agent"
        const val ERROR_NAME_TOO_LONG = "Use at most 64 characters"
        const val ERROR_DEVICE_REQUIRED = "Pick a device"
        const val ERROR_INSTRUCTIONS_TOO_LONG = "Instructions are too long"
        /** EXP-1306: the Account row with no login reported (muted). */
        const val ACCOUNT_LAST_USED = "Last used"
    }

    const val TEST_ID_LIST = "agents-settings-list"
    const val TEST_ID_PAGE = "agent-settings-page"

    /** The trigger fields [triggerSummary] reads (a stored agent trigger or
     *  the fixture's case). */
    data class TriggerLike(
        val kind: String,
        val enabled: Boolean? = null,
        /** schedule: the 5-field cron and its IANA zone. */
        val cron: String? = null,
        val tz: String? = null,
        /** event: the events row name it listens on. */
        val name: String? = null,
    )

    /** One Triggers row's text. */
    fun triggerSummary(trigger: TriggerLike): String {
        val text = if (trigger.kind == "schedule") {
            val cron = trigger.cron ?: ""
            val tz = trigger.tz
            if (!tz.isNullOrEmpty()) {
                Copy.SCHEDULE_CRON_TZ.replace("{cron}", cron).replace("{tz}", tz)
            } else {
                Copy.SCHEDULE_CRON.replace("{cron}", cron)
            }
        } else {
            val name = trigger.name ?: ""
            Copy.EVENTS[name] ?: Copy.EVENT.replace("{name}", name)
        }
        return if (trigger.enabled == false) "$text · ${Copy.PAUSED}" else text
    }

    /** A stored agent trigger as [triggerSummary] reads it. */
    fun triggerSummary(trigger: AgentTrigger): String = triggerSummary(
        when (val part = trigger.whenPart) {
            is TriggerWhen.Schedule -> TriggerLike(
                kind = "schedule",
                enabled = trigger.enabled,
                cron = part.cron,
                tz = part.tz,
            )
            is TriggerWhen.Event -> TriggerLike(kind = "event", enabled = trigger.enabled, name = part.name)
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

    /** One login the Account row may read ([agentAccountValue]'s input). */
    data class AccountValueOption(val id: String, val agent: String, val email: String, val isLastUsed: Boolean)

    /**
     * EXP-1306: what the Account row reads — the picked login's agent mark +
     * its email, the composer chip's value: the option whose id == the pinned
     * [account] (and whose agent == [cli] when set), else [cli]'s first, else
     * the last used one, else the first; null = no login reported (the row
     * reads [Copy.ACCOUNT_LAST_USED]). Replays `accountValueCases`.
     */
    fun agentAccountValue(options: List<AccountValueOption>, account: String?, cli: String?): AccountValueOption? {
        val pin = account?.takeIf { it.isNotEmpty() }
        val agent = cli?.takeIf { it.isNotEmpty() }
        return pin?.let { pinned -> options.firstOrNull { it.id == pinned && (agent == null || it.agent == agent) } }
            ?: agent?.let { picked -> options.firstOrNull { it.agent == picked } }
            ?: options.firstOrNull { it.isLastUsed }
            ?: options.firstOrNull()
    }
}
