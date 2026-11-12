package com.exponential.app.domain

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
            // NEW-2: nobody speaks cron — THE schedule picker's sentence.
            SchedulePicker.scheduleSummary(trigger.cron ?: "", trigger.tz)
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
                }
            },
        )

        /** The triggers a preset saves as (`off` = none). */
        fun scheduleTriggers(id: String, tz: String? = null): List<AgentTrigger> {
            val cron = SCHEDULES.firstOrNull { it.id == id }?.cron ?: return emptyList()
            return listOf(AgentTrigger(id = null, enabled = true, whenPart = TriggerWhen.Schedule(cron, tz?.takeIf { it.isNotEmpty() })))
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

        /** #EXP-1714: the team a composer BROWSES (`/` prompts, `#` issues,
         *  `@` members): its own team when that is a synced team, else the
         *  active one (the global Assistant's personal team never syncs).
         *  Uploads and sends keep the own team. No synced teams yet = own. */
        fun browseTeam(own: String?, teamIds: Collection<String>, active: String?): String? =
            if (own != null && teamIds.isNotEmpty() && own !in teamIds) active else own

        /** #EXP-1714: a team-scoped session list also keeps the viewer's own
         *  sessions that sit in no synced team (their personal team). */
        fun inTeamOrPersonal(sessionTeamId: String, userId: String?, teamId: String, teamIds: Collection<String>, me: String?): Boolean =
            sessionTeamId == teamId || (me != null && userId == me && teamIds.isNotEmpty() && sessionTeamId !in teamIds)
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
