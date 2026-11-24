package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * EXP-1291: Team settings → Agents — the Android mirror of
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

    /** NEW-2: the Events band under the agents list (web `AgentEventsBand`):
     *  the team's events newest first, at most this many. */
    const val EVENTS_CAP = 50
    const val EVENTS_TITLE = "Events"
    const val NO_EVENTS = "No events yet"

    /**
     * Who emitted an event, as its row's caption (web `eventEmitterLabel`):
     * the emitting session's title, the person's name, or the server.
     */
    fun eventEmitterLabel(
        emittedBy: String,
        sessionId: String?,
        userId: String?,
        sessionTitle: (String) -> String?,
        userName: (String) -> String,
    ): String = when (emittedBy) {
        "session" -> sessionId?.let(sessionTitle)?.takeIf { it.isNotEmpty() } ?: "Session"
        "user" -> userId?.let(userName) ?: "Someone"
        else -> "Exponential"
    }

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
        /** NEW-40: the list's empty state (web `AgentsListBand`, no fixture key). */
        const val NO_AGENTS = "No agents yet"
        const val NAME_PLACEHOLDER = "Agent name"
        const val INSTRUCTIONS_PLACEHOLDER = "What this agent takes care of…"
        const val ADD_TRIGGER = "Add"
        const val NO_SCHEDULE = "Waits for messages"
        const val PAUSED = "Paused"
        const val DELETE_CONFIRM = "Delete this agent? Its conversation is deleted with it."
        /** Event summaries by contract `eventBuiltinNames`. */
        val EVENTS: Map<String, String> = mapOf(
            "issue.created" to "When an issue is created",
            "issue.status_changed" to "When an issue's status changes",
            "issue.assigned" to "When an issue is assigned",
            "issue.deleted" to "When an issue is deleted",
            "issue.restored" to "When an issue is restored",
            "issue.purged" to "When a deleted issue is purged",
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
        /** #EXP-1846 (`page`): the agent page's section headers. */
        const val TRIGGERS = "Triggers"
        const val RUNS_ON = "Runs on"
        const val CHECK_IN = "Check in"
        /** #EXP-1850: an app agent's record / webhook trigger, a `run`
         *  trigger's second line. */
        const val RECORD = "When a {record} is {op}"
        val RECORD_OPS: Map<String, String> = linkedMapOf(
            "create" to "created",
            "update" to "updated",
            "delete" to "deleted",
        )
        const val WEBHOOK = "When webhook {name} is called"
        const val RUNS = "Runs {tool}"
        const val HANDS_OFF = "Hands off to {agent}"
    }

    /** #EXP-1846 (agent-settings.json `page.sections`): ONE agent page ×4
     *  for the Assistant and every team agent, top to bottom. */
    const val SECTION_IDENTITY = "identity"
    const val SECTION_RUNS_ON = "runs-on"
    /** #EXP-1988 (`assistant.teamPicks`): the Assistant's page only, ONE
     *  per synced team of its member, right after Runs on. */
    const val SECTION_TEAM_PICKS = "team-picks"
    const val SECTION_TRIGGERS = "triggers"
    const val SECTION_VOICE = "voice"
    const val SECTION_INSTRUCTIONS = "instructions"
    const val SECTION_DELETE = "delete"
    val PAGE_SECTIONS = listOf(
        SECTION_IDENTITY, SECTION_RUNS_ON, SECTION_TEAM_PICKS, SECTION_TRIGGERS, SECTION_VOICE, SECTION_INSTRUCTIONS, SECTION_DELETE,
    )
    /** The Runs on rows, in order. */
    val RUNS_ON_ROWS = listOf("device", "account", "model", "skills", McpRow.ID)
    /** #EXP-1988: a team section's rows (the Assistant's Runs on drops them). */
    val TEAM_PICKS_ROWS = listOf("skills", McpRow.ID)

    // #EXP-1846 (agent-settings.json `checkIn`): THE Check in = the ONE
    // schedule trigger carrying `preset`, any agent's; the mirror of TS
    // `checkInOf`, `withCheckIn`, `listedTriggers` (replayed by
    // `AgentSettingsTest`).

    private fun presetCron(id: String?): String? = Assistant.SCHEDULES.firstOrNull { it.id == id }?.cron

    private fun isCheckIn(trigger: AgentTrigger): Boolean =
        trigger.whenPart is TriggerWhen.Schedule && !trigger.preset.isNullOrEmpty()

    /** The Check in row's value (`checkInCases`): the preset trigger's id,
     *  `off` when there is none or it is paused. */
    fun checkInOf(triggers: List<AgentTrigger>): String {
        val trigger = triggers.firstOrNull(::isCheckIn) ?: return "off"
        if (!trigger.enabled || presetCron(trigger.preset) == null) return "off"
        return trigger.preset!!
    }

    /** The triggers after a Check in pick (`withCheckInCases`): the preset
     *  trigger replaced IN FRONT (keeping its id and stored zone; a new one
     *  takes [tz]) or removed for `off`; the others unchanged, in order. */
    fun withCheckIn(triggers: List<AgentTrigger>, id: String, tz: String?): List<AgentTrigger> {
        val current = triggers.firstOrNull(::isCheckIn)
        val rest = if (current == null) triggers else triggers.filter { it !== current }
        val cron = presetCron(id) ?: return rest
        val zone = if (current != null) (current.whenPart as TriggerWhen.Schedule).tz else tz
        val next = AgentTrigger(
            id = current?.id,
            enabled = true,
            whenPart = TriggerWhen.Schedule(cron, zone?.takeIf { it.isNotEmpty() }),
            preset = id,
        )
        return listOf(next) + rest
    }

    /** The rows under Check in (`listedCases`): every trigger without `preset`. */
    fun listedTriggers(triggers: List<AgentTrigger>): List<AgentTrigger> =
        triggers.filter { it.preset.isNullOrEmpty() }

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
        /** event: the events row name it listens on; webhook: the hook. */
        val name: String? = null,
        /** #EXP-1850 record: the record kind and the op it listens on. */
        val record: String? = null,
        val op: String? = null,
        /** #EXP-1850: an app binding the trigger runs instead of a wake. */
        val run: JsonObject? = null,
    )

    /** One Triggers row's text. */
    fun triggerSummary(trigger: TriggerLike): String {
        val text = if (trigger.kind == "schedule") {
            // NEW-2: nobody speaks cron — THE schedule picker's sentence.
            SchedulePicker.scheduleSummary(trigger.cron ?: "", trigger.tz)
        } else if (trigger.kind == "record") {
            // #EXP-1850: an app agent's record trigger.
            val op = Copy.RECORD_OPS[trigger.op ?: ""] ?: trigger.op ?: ""
            Copy.RECORD.replace("{record}", trigger.record ?: "").replace("{op}", op)
        } else if (trigger.kind == "webhook") {
            Copy.WEBHOOK.replace("{name}", trigger.name ?: "")
        } else {
            val name = trigger.name ?: ""
            Copy.EVENTS[name] ?: Copy.EVENT.replace("{name}", name)
        }
        return if (trigger.enabled == false) "$text · ${Copy.PAUSED}" else text
    }

    /** A stored agent trigger as [triggerSummary] reads it. */
    fun triggerSummary(trigger: AgentTrigger): String = triggerSummary(triggerLike(trigger))

    /** A stored agent trigger as [triggerSummary] / [triggerDetail] read it. */
    fun triggerLike(trigger: AgentTrigger): TriggerLike = when (val part = trigger.whenPart) {
        is TriggerWhen.Schedule -> TriggerLike(
            kind = "schedule",
            enabled = trigger.enabled,
            cron = part.cron,
            tz = part.tz,
            run = trigger.run,
        )
        is TriggerWhen.Event -> TriggerLike(kind = "event", enabled = trigger.enabled, name = part.name, run = trigger.run)
        is TriggerWhen.Record -> TriggerLike(
            kind = "record",
            enabled = trigger.enabled,
            record = part.record,
            op = part.op,
            run = trigger.run,
        )
        is TriggerWhen.Webhook -> TriggerLike(kind = "webhook", enabled = trigger.enabled, name = part.name, run = trigger.run)
    }

    /** #EXP-1850 (`detailCases`): an app binding in a few words
     *  (`records.<op> <kind>`, the action, `<upstream>/<tool>`). */
    fun runLabel(run: JsonObject): String {
        fun JsonObject.text(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
        val records = run["records"] as? JsonObject
        if (records != null) return "records.${records.text("op") ?: ""} ${records.text("kind") ?: ""}"
        run.text("action")?.let { return it }
        run.text("upstream")?.let { upstream -> return "$upstream/${run.text("tool") ?: ""}" }
        return ""
    }

    /** #EXP-1850 (`detailCases`): a Triggers row's second line = what a
     *  `run` trigger runs; null for a wake. */
    fun triggerDetail(trigger: TriggerLike): String? {
        val run = trigger.run ?: return null
        (run["agent"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.let { agent ->
            return Copy.HANDS_OFF.replace("{agent}", agent)
        }
        return Copy.RUNS.replace("{tool}", runLabel(run))
    }

    /** [triggerDetail] of a stored agent trigger. */
    fun triggerDetail(trigger: AgentTrigger): String? = triggerDetail(triggerLike(trigger))

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

    /** #EXP-1383: a synced agent row as [issueActor] / [assigneeOptions] read it. */
    data class AgentRowLike(
        val id: String,
        val name: String,
        val icon: String?,
        val color: String?,
        val sortOrder: Double = 0.0,
    )

    /** #EXP-1383: who an issue event / comment / issue creation reads as. */
    sealed interface IssueActor {
        data class Agent(val id: String, val name: String, val icon: String?, val color: String?) : IssueActor
        data class User(val id: String) : IssueActor
    }

    /**
     * #EXP-1383 (`actor.actorCases`): a synced agent first, else the member,
     * else null (the server / a widget reporter). An unsynced agent falls back
     * to the member.
     */
    fun issueActor(actorAgentId: String?, actorUserId: String?, agents: List<AgentRowLike>): IssueActor? {
        val agent = actorAgentId?.takeIf { it.isNotEmpty() }?.let { id -> agents.firstOrNull { it.id == id } }
        if (agent != null) return IssueActor.Agent(agent.id, agent.name, agent.icon, agent.color)
        if (!actorUserId.isNullOrEmpty()) return IssueActor.User(actorUserId)
        return null
    }

    /** #EXP-1383: the assignee option id prefixes (`assignee.optionCases`). */
    const val ASSIGNEE_UNASSIGNED = "unassigned"
    const val ASSIGNEE_USER_PREFIX = "user:"
    const val ASSIGNEE_AGENT_PREFIX = "agent:"

    /** #EXP-1383: `assignee.copy.unassigned`. */
    const val ASSIGNEE_UNASSIGNED_LABEL = "Unassigned"

    /** The team's agents in assignee-picker order: sortOrder, then name, then id. */
    fun <T> sortedAssigneeAgents(agents: List<T>, row: (T) -> AgentRowLike): List<T> =
        agents.sortedWith(
            compareBy<T> { row(it).sortOrder }
                .thenComparator { a, b -> compareNames(row(a).name, row(b).name) }
                .thenBy { row(it).id },
        )

    /** `localeCompare`-like: case-insensitive first, then exact. */
    private fun compareNames(a: String, b: String): Int {
        val folded = a.compareTo(b, ignoreCase = true)
        return if (folded != 0) folded else a.compareTo(b)
    }

    /**
     * #EXP-1383: the assignee picker's option ids ×4: unassigned, the members
     * in the given order, then the team's agents by sortOrder then name.
     */
    fun assigneeOptions(memberIds: List<String>, agents: List<AgentRowLike>): List<String> =
        listOf(ASSIGNEE_UNASSIGNED) +
            memberIds.map { "$ASSIGNEE_USER_PREFIX$it" } +
            sortedAssigneeAgents(agents) { it }.map { "$ASSIGNEE_AGENT_PREFIX${it.id}" }

    /**
     * #EXP-1383: an issue's assignee = ONE of two exclusive columns. Every
     * write sends BOTH, the cleared one as an explicit null: an agent wins and
     * clears the person.
     */
    data class Assignee(val userId: String?, val agentId: String?) {
        val isEmpty: Boolean get() = userId.isNullOrEmpty() && agentId.isNullOrEmpty()

        /** The two-column write. */
        fun write(): Assignee = if (!agentId.isNullOrEmpty()) Assignee(null, agentId) else Assignee(userId?.takeIf { it.isNotEmpty() }, null)

        companion object {
            val NONE = Assignee(null, null)
        }
    }

    /**
     * #EXP-1383: a solo team hides its assignee control only while it has no
     * agents (an agent is someone to assign to); a set agent assignee keeps it.
     */
    fun hidesAssigneeControl(memberCount: Int, agentCount: Int, assigneeAgentId: String? = null): Boolean =
        memberCount == 1 && agentCount == 0 && assigneeAgentId.isNullOrEmpty()

    // S-3 (owner 2026-10-28): THE ASSISTANT (`assistant` in the fixture;
    // contract `agentRole`), the mirror of TS `ASSISTANT*`,
    // `assistantScheduleOf`, `visibleAgent`, `orderedAgents`; replayed by
    // `AgentSettingsTest` (`scheduleCases`, `visibleCases`, `orderCases`).
    /** A Runs on candidate as [runsOnDeviceIds] reads it. */
    data class RunsOnDevice(val deviceId: String, val kind: String?)

    /**
     * NEW-99 (`assistant.runsOnCases`, TS `runsOnDeviceIds`): the Runs on
     * device picker's devices, in the given order. An Assistant's device is
     * the member's voice device, so it lists desktops only (a bound
     * non-desktop stays, naming itself); a team agent keeps every device.
     */
    fun runsOnDeviceIds(devices: List<RunsOnDevice>, boundId: String?, assistant: Boolean): List<String> =
        devices.filter { !assistant || it.kind == "desktop" || it.deviceId == boundId }.map { it.deviceId }

    /** A Runs on picker row (`runsOnRowCases`): [description] = the secondary
     *  line (devices-list `copy.offline` while offline), [disabled] its state. */
    data class RunsOnRow(val id: String, val description: String?, val disabled: Boolean)

    /**
     * THE Runs on rows ×4 (`runsOnRowCases`, TS `runsOnRows`, web
     * `deviceRows`): the devices of [runsOnDeviceIds], online ones pickable,
     * an offline one listed with `Offline` and disabled unless bound; a bound
     * id no device matches leads by its raw id. [online] is per device id.
     */
    fun runsOnRows(devices: List<RunsOnDevice>, online: (String) -> Boolean, boundId: String?, assistant: Boolean): List<RunsOnRow> {
        val kept = runsOnDeviceIds(devices, boundId, assistant).toSet()
        val rows = devices.filter { it.deviceId in kept }.map {
            val up = online(it.deviceId)
            RunsOnRow(it.deviceId, if (up) null else DevicesList.Copy.OFFLINE, !up && it.deviceId != boundId)
        }
        val unmatched = boundId?.takeIf { id -> id.isNotEmpty() && devices.none { it.deviceId == id } }
        return if (unmatched != null) listOf(RunsOnRow(unmatched, null, false)) + rows else rows
    }

    /**
     * #EXP-1793 (agent-settings.json `mcpRow`): the agent page's MCP servers
     * row, right under Skills (the Assistant: after Skills, the last row of
     * Runs on); THE composer's MCP servers multi picker; owners edit
     * (`agents.update {mcpServerIds}`), members read. Shown while the team
     * has a server or the agent already names one.
     */
    object McpRow {
        const val ID = "mcp-servers"
        const val LABEL = "MCP servers"
        const val AFTER = "skills"
        const val NONE = "None"
        const val MAX = 20
        const val TEST_ID = "agent-settings-mcp-servers"

        fun visible(teamServerCount: Int, agentServerCount: Int): Boolean = teamServerCount > 0 || agentServerCount > 0
    }

    /**
     * #EXP-1717 (agent-settings.json `coordinator`): THE team Coordinator, a
     * ROLE on one team agent (`agents.role = coordinator`, at most one per
     * team). Owners pick it in Team settings › Agents (ONE settings Picker
     * row above the agents list; members read it); `agents.setCoordinator`.
     */
    object Coordinator {
        const val ROLE: String = DomainContract.agentRoleCoordinator
        /** The option id of "no coordinator". */
        const val NONE = "none"
        const val LABEL = "Coordinator"
        const val NONE_LABEL = "None"
        const val REFUSED_OWNER = "Only the team's owners set its coordinator"
        const val REFUSED_AGENT = "Only a team agent of this team can be its coordinator"
        const val TEST_ID_ROW = "settings-agents-coordinator"

        data class OptionLike(val id: String, val name: String, val role: String?, val sortOrder: Double)

        fun isCoordinator(role: String?): Boolean = role == ROLE

        /** `optionCases`: null (= [NONE]) first, then the team agents (never
         *  an Assistant) by sortOrder, then name. */
        fun <T> coordinatorOptions(agents: List<T>, row: (T) -> OptionLike): List<T?> =
            listOf<T?>(null) + agents
                .filter { !Assistant.isAssistant(row(it).role) }
                .sortedWith(
                    compareBy<T> { row(it).sortOrder }
                        .thenComparator { a, b -> compareNames(row(a).name, row(b).name) },
                )

        /** `valueCases`: the team's coordinator id, or null. */
        fun <T> coordinatorOf(agents: List<T>, teamId: String, id: (T) -> String, team: (T) -> String, role: (T) -> String?): String? =
            agents.firstOrNull { team(it) == teamId && isCoordinator(role(it)) }?.let(id)
    }

    /**
     * #EXP-1735 (agent-settings.json `lead`): THE team's lead Assistant, one
     * member's Assistant every member's Assistant reports to. Owners pick it
     * in Team settings › Agents (ONE settings Picker row where the
     * Coordinator row was; members read it); `agents.setLeadAssistant`.
     */
    object Lead {
        /** The option id of "no lead Assistant". */
        const val NONE = "none"
        const val LABEL = "Lead Assistant"
        const val NONE_LABEL = "None"
        const val REFUSED_OWNER = "Only the team's owners set its lead Assistant"
        const val REFUSED_MEMBER = "Only a member of this team can lead it"
        const val NO_ASSISTANT = "That member has no device to run an Assistant on yet"
        const val TEST_ID_ROW = "settings-agents-lead"

        data class MemberLike(val userId: String, val name: String)

        /** `optionCases`: null (= [NONE]) first, then the team's members by
         *  display name, then id. */
        fun <T> leadOptions(members: List<T>, member: (T) -> MemberLike): List<T?> =
            listOf<T?>(null) + members.sortedWith(
                Comparator<T> { a, b -> compareNames(member(a).name, member(b).name) }
                    .thenBy { member(it).userId },
            )
    }

    object Assistant {
        const val ROLE = "assistant"
        const val TITLE = "Assistant"
        const val NAME = "Assistant"
        const val RUNS_ON = "Runs on"
        const val CHECK_IN = "Check in"
        const val VOICE = "Voice"
        const val NO_DEVICE = "Connect a desktop to set up your Assistant"
        const val DELETE_REFUSED = "The Assistant can't be deleted"
        const val SCHEDULE_REFUSED = "The Assistant checks in on a schedule only"
        /** #EXP-1439: `agents.update` refuses an Assistant's instructions. */
        const val PROMPT_FIXED = "The Assistant's prompt is built in"
        const val LEGACY_INSTRUCTIONS = "Your Assistant now runs on Exponential's built-in prompt, so its instructions field is gone. Your old instructions, to move into a team agent if you still need them:"
        const val DEFAULT_SCHEDULE = "1h"
        /** No Instructions (#EXP-1439: the prompt is built in), no Delete. */
        val SECTIONS = listOf("identity", "runs-on", "check-in", "voice")

        data class Schedule(val id: String, val label: String, val cron: String?)

        val SCHEDULES = listOf(
            Schedule("off", "Off", null),
            Schedule("15m", "Every 15 minutes", "*/15 * * * *"),
            Schedule("30m", "Every 30 minutes", "*/30 * * * *"),
            Schedule("1h", "Every hour", "0 * * * *"),
            Schedule("daily", "Every day", "0 9 * * *"),
        )

        /** The Model row's value (`modelValueCases`): the pinned model, else
         *  the contract default for the login's CLI (never `Last used`). */
        fun modelValue(model: String?, cli: String?): String = when {
            !model.isNullOrBlank() -> model
            cli == "codex" -> DomainContract.deviceAgentDefaultsCodexModel
            else -> DomainContract.deviceAgentDefaultsModel
        }

        /** The preset stored triggers stand for; null = not an Assistant's
         *  (an event, a custom cron, more than one). */
        fun scheduleOf(triggers: List<TriggerLike>): String? {
            if (triggers.isEmpty()) return "off"
            if (triggers.size > 1) return null
            val trigger = triggers.single()
            if (trigger.kind != "schedule") return null
            val preset = SCHEDULES.firstOrNull { it.cron != null && it.cron == trigger.cron } ?: return null
            return if (trigger.enabled == false) "off" else preset.id
        }

        /** [scheduleOf] over the parsed stored triggers. */
        fun scheduleOfStored(triggers: List<AgentTrigger>): String? = scheduleOf(
            triggers.map { trigger ->
                when (val part = trigger.whenPart) {
                    is TriggerWhen.Schedule -> TriggerLike("schedule", trigger.enabled, part.cron, part.tz)
                    is TriggerWhen.Event -> TriggerLike("event", trigger.enabled, name = part.name)
                    is TriggerWhen.Record -> TriggerLike("record", trigger.enabled, record = part.record, op = part.op)
                    is TriggerWhen.Webhook -> TriggerLike("webhook", trigger.enabled, name = part.name)
                }
            },
        )

        /** The triggers a preset saves as (`off` = none), tagged as the
         *  Check in (#EXP-1846). */
        fun scheduleTriggers(id: String, tz: String? = null): List<AgentTrigger> {
            val cron = SCHEDULES.firstOrNull { it.id == id }?.cron ?: return emptyList()
            return listOf(
                AgentTrigger(id = null, enabled = true, whenPart = TriggerWhen.Schedule(cron, tz?.takeIf { it.isNotEmpty() }), preset = id),
            )
        }

        fun isAssistant(role: String?): Boolean = role == ROLE

        /** Another member's Assistant is never listed (settings, composer, chats). */
        fun visibleAgent(role: String?, ownerId: String?, me: String?): Boolean =
            role != ROLE || ownerId == me

        data class OrderLike(val role: String?, val ownerId: String?, val name: String, val sortOrder: Int)

        /** The agents a member sees, in order: their Assistant first, then the
         *  team agents by sortOrder, then name. */
        fun <T> orderedAgents(agents: List<T>, me: String?, row: (T) -> OrderLike): List<T> =
            agents.filter { row(it).let { r -> visibleAgent(r.role, r.ownerId, me) } }
                .sortedWith(
                    compareBy<T> { if (isAssistant(row(it).role)) 0 else 1 }
                        .thenBy { row(it).sortOrder }
                        .thenComparator { a, b -> compareNames(row(a).name, row(b).name) },
                )

        data class TeamAgentLike(val teamId: String, val role: String?, val ownerId: String?)

        /** #EXP-1714 (`teamAgentsCases`): the agents a team lists = its own rows
         *  (input order) + the viewer's global Assistant (its team is none of the
         *  synced `teamIds`, i.e. their personal team) while the team holds no
         *  Assistant row of theirs. */
        fun <T> agentsInTeam(agents: List<T>, teamId: String, teamIds: Collection<String>, me: String?, row: (T) -> TeamAgentLike): List<T> {
            val own = agents.filter { row(it).teamId == teamId }
            if (own.any { row(it).let { r -> isAssistant(r.role) && r.ownerId == me } }) return own
            val global = agents.firstOrNull { row(it).let { r -> isAssistant(r.role) && r.ownerId == me && r.teamId !in teamIds } }
            return if (global != null) listOf(global) + own else own
        }

        /** #EXP-1988 (`teamPicks.pickAgentCases`, TS `assistantPickAgent`):
         *  the agent a team section of the Assistant's page edits = the
         *  viewer's Assistant [agentsInTeam] lists in [teamId] (switch off:
         *  that team's own; on: the global one), else the page's. */
        fun <T> pickAgent(
            agents: List<T>,
            teamId: String,
            teamIds: Collection<String>,
            me: String?,
            pageAgentId: String?,
            id: (T) -> String,
            row: (T) -> TeamAgentLike,
        ): String? = agentsInTeam(agents, teamId, teamIds, me, row)
            .firstOrNull { row(it).let { r -> isAssistant(r.role) && r.ownerId == me } }
            ?.let(id) ?: pageAgentId

        /** #EXP-1988 (`teamPickCases`, TS `teamPick`): the picks for [teamId]
         *  in a flat list = the listed ids whose row is that team's, pick order. */
        fun teamPick(ids: List<String>, rowTeams: Map<String, String>, teamId: String): List<String> =
            ids.distinct().filter { rowTeams[it] == teamId }

        /** #EXP-2079 (`chatTeams.cases`, TS `assistantChatTeams`): the teams
         *  the lists of a chat read (`#` refs, `@` members, the MCP pick,
         *  prompts, Link / New issue): the own team while it is synced, else
         *  the turn's teams (the Teams pick ∩ the synced teams, else all of
         *  them), in the synced order ([syncedTeamIds] is given in the
         *  cross-team order). Uploads and sends keep the own team. */
        fun chatTeams(ownTeamId: String?, syncedTeamIds: List<String>, pickedTeamIds: Collection<String>?): List<String> {
            if (!ownTeamId.isNullOrEmpty() && ownTeamId in syncedTeamIds) return listOf(ownTeamId)
            if (syncedTeamIds.isEmpty()) return listOfNotNull(ownTeamId?.takeIf { it.isNotEmpty() })
            val picked = pickedTeamIds.orEmpty().toSet()
            val turn = syncedTeamIds.filter { it in picked }
            return turn.ifEmpty { syncedTeamIds }
        }

        /** #EXP-2079 (`chatTeams.deviceCases`, TS `assistantOwnDevices`): the
         *  device list of a session in a team the viewer does not sync, that
         *  they started (the Assistant's conversation) = their own machines,
         *  no team rule. */
        fun ownDevices(sessionTeamSynced: Boolean, starterIsViewer: Boolean): Boolean =
            !sessionTeamSynced && starterIsViewer

        /** #EXP-1714: a team-scoped session list also keeps the viewer's own
         *  sessions that sit in no synced team (their personal team). */
        fun inTeamOrPersonal(sessionTeamId: String, userId: String?, teamId: String, teamIds: Collection<String>, me: String?): Boolean =
            sessionTeamId == teamId || (me != null && userId == me && teamIds.isNotEmpty() && sessionTeamId !in teamIds)

        /** #EXP-1987: THE ASSISTANT'S CHAT IS THE MEMBER'S OWN: a chat whose
         *  team is none of the synced teams (their personal team after the
         *  move) and whose starter is the viewer gives the viewer a member's
         *  rights in it. No synced teams yet = not decided (false). */
        fun ownChat(chatTeamId: String?, starterId: String?, teamIds: Collection<String>, me: String?): Boolean =
            !me.isNullOrEmpty() && starterId == me && !chatTeamId.isNullOrEmpty() &&
                teamIds.isNotEmpty() && chatTeamId !in teamIds
    }

    /** #EXP-1990 (agent-settings.json `groupChat`): a session as
     *  [agentChatOf] reads it. */
    data class ChatSessionLike(val id: String, val parentId: String?, val agentId: String?, val userId: String?)

    /** #EXP-1990 (`groupChat.chatCases`, TS `agentChatOf`): THE chat of an
     *  agent for the viewer: its group chat ([groupChatId]) when that root is
     *  synced, none when it names a root the viewer does not sync; with no
     *  group chat the root with its [agentId] the viewer started (the
     *  Assistant; a team agent before the switch). */
    fun <T> agentChatOf(agentId: String, groupChatId: String?, sessions: List<T>, me: String?, row: (T) -> ChatSessionLike): T? {
        if (!groupChatId.isNullOrEmpty()) {
            return sessions.firstOrNull { row(it).let { s -> s.id == groupChatId && s.parentId == null } }
        }
        if (me.isNullOrEmpty()) return null
        return sessions.firstOrNull { row(it).let { s -> s.parentId == null && s.agentId == agentId && s.userId == me } }
    }
}

/** #EXP-1793 (`mcpValueCases`, TS `agentMcpServersValue`): the MCP servers
 *  row's value, the picked names (team list order) as the one picker summary
 *  (`None` / `a` / `a, b` / `a, b +N`). */
fun agentMcpServersValue(names: List<String>): String = when {
    names.isEmpty() -> AgentSettings.McpRow.NONE
    names.size <= 2 -> names.joinToString(", ")
    else -> "${names.take(2).joinToString(", ")} +${names.size - 2}"
}

/**
 * #EXP-1847 (agent-settings.json `voice`, TS `AGENT_VOICE`): every agent's
 * own Voice section (Language, Voice, Transcript, Wake phrase), its copy, the
 * clip upload route and the limits; [agentVoiceOf] resolves the voice against
 * the member's voice settings.
 */
object AgentVoiceSection {
    const val ROW_LANGUAGE = "language"
    const val ROW_CLIP = "clip"
    const val ROW_TRANSCRIPT = "transcript"
    const val ROW_WAKE_PHRASE = "wake-phrase"
    /** #EXP-1859: a TEAM agent's page only (the Assistant's keeps the member Wake word row). */
    const val ROW_WAKE_WORD = "wake-word"
    val ROWS = listOf(ROW_LANGUAGE, ROW_CLIP, ROW_TRANSCRIPT, ROW_WAKE_PHRASE, ROW_WAKE_WORD)

    object Copy {
        const val LABEL = "Voice"
        const val LANGUAGE = "Language"
        const val CLIP = "Voice"
        const val CLIP_BUILT_IN = "Built-in"
        const val CLIP_UPLOAD = "Upload a voice clip"
        const val CLIP_REMOVE = "Remove clip"
        const val TRANSCRIPT = "Transcript"
        const val TRANSCRIPT_PLACEHOLDER = "What the clip says"
        const val WAKE_PHRASE = "Wake phrase"
        const val WAKE_PHRASE_PLACEHOLDER = "Hey {agent}"
        const val ERROR_CLIP_TYPE = "Voice clips must be WAV files"
        const val ERROR_CLIP_SIZE = "Voice clips must be 5 MB or smaller"
    }

    /** `clipUpload`: one multipart part `file`; DELETE clears the clip. */
    object ClipUpload {
        const val ROUTE = "/api/agents/{agentId}/voice-clip"
        val ACCEPT = listOf("audio/wav", "audio/x-wav", "audio/wave")
        const val MAX_BYTES = 5_242_880L
    }

    object Limits {
        const val TRANSCRIPT = 500
        const val WAKE_PHRASE = 64
        const val WAKE_PHRASE_MIN_LETTERS = 6
    }

    object TestIds {
        const val SECTION = "agent-voice"
        const val LANGUAGE = "agent-voice-language"
        const val CLIP = "agent-voice-clip"
        const val CLIP_REMOVE = "agent-voice-clip-remove"
        const val TRANSCRIPT = "agent-voice-transcript"
        const val WAKE_PHRASE = "agent-voice-wake-phrase"
        const val WAKE_WORD = "agent-voice-wake-word"
    }

    fun clipUploadPath(agentId: String): String = ClipUpload.ROUTE.replace("{agentId}", agentId)

    fun wakePhrasePlaceholder(agentName: String): String = Copy.WAKE_PHRASE_PLACEHOLDER.replace("{agent}", agentName)

    /**
     * The picked file's refusal before any upload (null = send it): larger
     * than `clipUpload.maxBytes`, or a declared type that is neither one of
     * `clipUpload.accept` nor generic while the name is no `.wav` (the
     * server sniffs the bytes: pickers often send no or a generic type).
     */
    fun clipRefusal(contentType: String?, filename: String?, sizeBytes: Long?): String? {
        if (sizeBytes != null && sizeBytes > ClipUpload.MAX_BYTES) return Copy.ERROR_CLIP_SIZE
        val type = contentType?.substringBefore(';')?.trim()?.lowercase().orEmpty()
        val generic = type.isEmpty() || type == "application/octet-stream"
        val wavName = filename?.lowercase()?.endsWith(".wav") == true
        if (!generic && type !in ClipUpload.ACCEPT && !wavName) return Copy.ERROR_CLIP_TYPE
        return null
    }

    /** The content type the upload declares: an accepted one as is, else `audio/wav`. */
    fun clipContentType(contentType: String?): String {
        val type = contentType?.substringBefore(';')?.trim()?.lowercase().orEmpty()
        return if (type in ClipUpload.ACCEPT) type else ClipUpload.ACCEPT.first()
    }
}

/** #EXP-1847: an agent's own voice (`agents.voice` jsonb; `{}` = the member's). */
data class AgentVoiceLike(
    val language: String? = null,
    val clipId: String? = null,
    val clipName: String? = null,
    val transcript: String? = null,
    val wakePhrase: String? = null,
) {
    companion object {
        val NONE = AgentVoiceLike()

        /** The synced jsonb TEXT (or an object); null / malformed = none. */
        fun parse(raw: String?): AgentVoiceLike {
            if (raw.isNullOrBlank()) return NONE
            val element = runCatching { kotlinx.serialization.json.Json.parseToJsonElement(raw) }.getOrNull()
            return of(element as? kotlinx.serialization.json.JsonObject ?: return NONE)
        }

        fun of(obj: kotlinx.serialization.json.JsonObject): AgentVoiceLike {
            fun text(key: String): String? =
                (obj[key] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
            return AgentVoiceLike(
                language = text("language"),
                clipId = text("clipId"),
                clipName = text("clipName"),
                transcript = text("transcript"),
                wakePhrase = text("wakePhrase"),
            )
        }
    }
}

/** #EXP-1847: the member's voice settings the resolution falls back to (`voice.get`). */
data class MemberVoiceLike(
    val language: String,
    val voiceFile: String,
    val voiceTranscript: String,
    val wakePhrase: String,
)

/** The clip an agent speaks with: its uploaded attachment or the member's file path. */
sealed interface AgentVoiceClip {
    data class Attachment(val attachmentId: String) : AgentVoiceClip
    data class Path(val path: String) : AgentVoiceClip
}

data class ResolvedAgentVoice(
    val language: String,
    /** null = the built-in voice. */
    val clip: AgentVoiceClip?,
    val clipLabel: String,
    val transcript: String,
    val wakePhrase: String,
)

/** #EXP-1847 (`voiceOfCases`, TS `agentVoiceOf`): the voice an agent speaks
 *  with ([name] = the TS signature's; the placeholder reads it, not this). */
@Suppress("UNUSED_PARAMETER")
fun agentVoiceOf(role: String?, name: String, voice: AgentVoiceLike?, member: MemberVoiceLike): ResolvedAgentVoice {
    val own = voice ?: AgentVoiceLike.NONE
    val assistant = role == DomainContract.agentRoleAssistant
    val language = own.language?.takeIf { it.isNotEmpty() } ?: member.language
    val ownPhrase = own.wakePhrase?.takeIf { it.isNotBlank() }
    val wakePhrase = ownPhrase ?: if (assistant) member.wakePhrase else ""
    if (!own.clipId.isNullOrEmpty()) {
        return ResolvedAgentVoice(
            language = language,
            clip = AgentVoiceClip.Attachment(own.clipId),
            clipLabel = own.clipName?.takeIf { it.isNotEmpty() } ?: AgentVoiceSection.Copy.CLIP_BUILT_IN,
            transcript = own.transcript ?: "",
            wakePhrase = wakePhrase,
        )
    }
    if (assistant && member.voiceFile.isNotEmpty()) {
        val last = member.voiceFile.split('/', '\\').last()
        return ResolvedAgentVoice(
            language = language,
            clip = AgentVoiceClip.Path(member.voiceFile),
            clipLabel = last.ifEmpty { member.voiceFile },
            transcript = own.transcript ?: member.voiceTranscript,
            wakePhrase = wakePhrase,
        )
    }
    return ResolvedAgentVoice(language, null, AgentVoiceSection.Copy.CLIP_BUILT_IN, own.transcript ?: "", wakePhrase)
}

/**
 * #EXP-1859 (agent-settings.json `voice.wake`, TS `wakeListeners` /
 * `wakeRow` / `wakeToggle` / `wakeClash` / `wakePhraseClash`): wake phrases
 * per agent. The member's voice device listens for the voice agent, then the
 * team agents of `wakeAgentIds` that fit (at most [MAX_LISTENERS]); a team
 * agent's `wake-word` row reads [row] and flips through [toggle].
 */
object AgentWake {
    const val MAX_LISTENERS = 5
    const val MAX_IDS = 32
    /** talk-listen.json `wake.constants.maxEditRatio`. */
    const val MAX_EDIT_RATIO = 0.25

    object Copy {
        const val WAKE_WORD = "Wake word"
        const val ERROR_TOO_SHORT = "The wake phrase needs at least 6 letters"
        const val ERROR_CLASH = "Sounds like {agent}'s wake phrase"
        const val ERROR_CAP = "At most 5 agents listen at once"
    }

    data class AgentLike(val id: String, val role: String?, val name: String, val voice: AgentVoiceLike?)

    data class SettingsLike(
        val voiceDeviceId: String?,
        /** The voice agent (the member's Assistant). */
        val agentId: String?,
        val wakeWord: Boolean,
        val wakeAgentIds: List<String>,
        val member: MemberVoiceLike,
    )

    data class Input(val settings: SettingsLike, val agents: List<AgentLike>)

    data class Listener(val agentId: String, val phrase: String, val language: String)

    data class Row(val on: Boolean, val error: String?)

    data class Toggle(val wakeWord: Boolean, val wakeAgentIds: List<String>, val error: String?)

    // The same table as the desktop's `wake_words` (etalk-engine wake.rs).
    private val FOLD: Map<String, String> = buildMap {
        "äáàâå".forEach { put(it.toString(), "a") }
        "öóòô".forEach { put(it.toString(), "o") }
        "üúùû".forEach { put(it.toString(), "u") }
        "éèêë".forEach { put(it.toString(), "e") }
        "íìîï".forEach { put(it.toString(), "i") }
        put("ç", "c"); put("ñ", "n"); put("ß", "ss")
    }

    /** A phrase's letters (talk-listen.json `wake.words` joined without
     *  spaces): letters and digits, lower case, accents folded. */
    fun letters(phrase: String): String {
        val out = StringBuilder()
        var i = 0
        while (i < phrase.length) {
            val cp = phrase.codePointAt(i)
            i += Character.charCount(cp)
            if (!Character.isLetterOrDigit(cp)) continue
            val lower = String(Character.toChars(cp)).lowercase()
            var j = 0
            while (j < lower.length) {
                val c = lower.codePointAt(j)
                j += Character.charCount(c)
                val one = String(Character.toChars(c))
                out.append(FOLD[one] ?: one)
            }
        }
        return out.toString()
    }

    private fun codePoints(text: String): IntArray = text.codePoints().toArray()

    private fun levenshtein(a: IntArray, b: IntArray): Int {
        val row = IntArray(b.size + 1) { it }
        for (i in a.indices) {
            var diagonal = row[0]
            row[0] = i + 1
            for (j in b.indices) {
                val above = row[j + 1]
                row[j + 1] = minOf(above + 1, row[j] + 1, diagonal + if (a[i] == b[j]) 0 else 1)
                diagonal = above
            }
        }
        return row[b.size]
    }

    /** `clashCases`: alike enough for the matcher to take one for the other,
     *  or one starting with the other. */
    fun clash(a: String, b: String): Boolean {
        val left = codePoints(letters(a))
        val right = codePoints(letters(b))
        val (short, long) = if (right.size < left.size) right to left else left to right
        val tolerance = kotlin.math.floor(short.size * MAX_EDIT_RATIO).toInt()
        return minOf(levenshtein(short, long), levenshtein(short, long.copyOfRange(0, minOf(short.size, long.size)))) <= tolerance
    }

    /** The phrase an agent wakes with: its voice's ([agentVoiceOf]), else `Hey {name}`. */
    fun wakePhraseOf(agent: AgentLike, member: MemberVoiceLike): String {
        val phrase = agentVoiceOf(agent.role, agent.name, agent.voice, member).wakePhrase
        return if (phrase.isNotBlank()) phrase else AgentVoiceSection.wakePhrasePlaceholder(agent.name)
    }

    /** voice.json `wakePhraseError` for a resolved phrase: too few letters. */
    private fun tooShort(phrase: String): Boolean =
        codePoints(phrase.trim()).count { Character.isLetter(it) } < AgentVoiceSection.Limits.WAKE_PHRASE_MIN_LETTERS

    private fun isTeamAgent(agent: AgentLike): Boolean = agent.role != DomainContract.agentRoleAssistant

    private data class Named(val listener: Listener, val name: String)

    private fun refusal(agent: AgentLike, input: Input, listeners: List<Named>): String? {
        val phrase = wakePhraseOf(agent, input.settings.member)
        if (tooShort(phrase)) return Copy.ERROR_TOO_SHORT
        listeners.firstOrNull { clash(it.listener.phrase, phrase) }?.let { return Copy.ERROR_CLASH.replace("{agent}", it.name) }
        if (listeners.size >= MAX_LISTENERS) return Copy.ERROR_CAP
        return null
    }

    private fun named(agent: AgentLike, member: MemberVoiceLike): Named = Named(
        Listener(agent.id, wakePhraseOf(agent, member), agentVoiceOf(agent.role, agent.name, agent.voice, member).language),
        agent.name,
    )

    private fun namedListeners(input: Input): List<Named> {
        val s = input.settings
        if (!s.wakeWord || s.voiceDeviceId.isNullOrEmpty() || s.agentId.isNullOrEmpty()) return emptyList()
        val voiceAgent = input.agents.firstOrNull { it.id == s.agentId } ?: return emptyList()
        val out = mutableListOf(named(voiceAgent, s.member))
        for (id in s.wakeAgentIds) {
            val agent = input.agents.firstOrNull { it.id == id } ?: continue
            if (!isTeamAgent(agent) || out.any { it.listener.agentId == id }) continue
            if (refusal(agent, input, out) != null) continue
            out += named(agent, s.member)
        }
        return out
    }

    /** `listenersCases`: who the voice device listens for, in order. */
    fun listeners(input: Input): List<Listener> = namedListeners(input).map { it.listener }

    /** `rowCases`: a team agent's Wake word row: on, and why it does not
     *  listen (null = it does, or it is off). */
    fun row(input: Input, agentId: String): Row {
        val s = input.settings
        val on = s.wakeWord && agentId in s.wakeAgentIds
        if (!on) return Row(false, null)
        val agent = input.agents.firstOrNull { it.id == agentId }
        if (agent == null || !isTeamAgent(agent)) return Row(true, null)
        val index = s.wakeAgentIds.indexOf(agentId)
        val before = namedListeners(input.copy(settings = s.copy(wakeAgentIds = s.wakeAgentIds.take(index))))
        if (before.any { it.listener.agentId == agentId }) return Row(true, null)
        return Row(true, refusal(agent, input, before))
    }

    /** `toggleCases`: a flip. Off drops the id (Wake word as it was); on
     *  appends it once and turns Wake word on, unless refused (unchanged,
     *  `error` = the copy). Not a team agent = the server's NOT_FOUND. */
    fun toggle(input: Input, agentId: String, on: Boolean): Toggle {
        val s = input.settings
        val ids = s.wakeAgentIds
        if (!on) return Toggle(s.wakeWord, ids.filter { it != agentId }, null)
        val agent = input.agents.firstOrNull { it.id == agentId }
        if (agent == null || !isTeamAgent(agent)) return Toggle(s.wakeWord, ids, null)
        val others = ids.filter { it != agentId }
        val listeners = namedListeners(input.copy(settings = s.copy(wakeWord = true, wakeAgentIds = others)))
            .filter { it.listener.agentId != agentId }
        val error = refusal(agent, input, listeners)
        if (error != null) return Toggle(s.wakeWord, ids, error)
        return Toggle(true, if (agentId in ids) ids else ids + agentId, null)
    }

    /** `phraseClashCases`: the first OTHER team agent whose phrase [phrase]
     *  sounds like (an Assistant never counts); null = none. */
    fun phraseClash(phrase: String, agentId: String, agents: List<AgentLike>): String? {
        val member = MemberVoiceLike(language = "", voiceFile = "", voiceTranscript = "", wakePhrase = "")
        return agents.firstOrNull { it.id != agentId && isTeamAgent(it) && clash(wakePhraseOf(it, member), phrase) }?.name
    }
}
