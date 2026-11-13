package com.exponential.app.domain

// #EXP-1815: where a notification shows, when it clears, who mutes it ×4.
// Hand-ported from `packages/domain-contract/src/notification-delivery.ts`,
// locked to `fixtures/notification-delivery.json` (its `$comment` is the spec)
// by NotificationDeliveryTest, which replays every `*Cases` array and checks
// the constants below against the fixture's `dataKeys`, `limits`, `copy` and
// `channels`.

object NotificationDelivery {
    // ── Fixture constants ────────────────────────────────────────────────

    object Keys {
        const val THREAD = "thread"
        const val SOURCE = "source"
        const val SOURCE_NAME = "sourceName"
        const val DISMISS_TYPE = "dismiss"
        const val IDS = "ids"
        const val THREADS = "threads"
        const val ALL = "all"
        const val TEAM_ID = "teamId"
    }

    const val MAX_IDS = 60
    const val MAX_THREADS = 30

    object Copy {
        const val TOAST_ACTION = "Open"
        const val SESSIONS_BAND = "Sessions"
        const val ASSISTANT = "Personal Assistant"
        const val TEMPORARY = "Temporary sessions"
        const val AGENTS_BAND = "Team agents"
    }

    object Channels {
        const val ASSISTANT_ID = "source-assistant"
        const val ASSISTANT_NAME = "Personal Assistant"
        const val TEMPORARY_ID = "source-temporary"
        const val TEMPORARY_NAME = "Temporary sessions"
        const val AGENT_PREFIX = "source-agent-"
        const val AGENTS_GROUP_ID = "team-agents"
        const val AGENTS_GROUP_NAME = "Team agents"
        const val FALLBACK = "issues_default"
    }

    // ── Thread ───────────────────────────────────────────────────────────

    /** The thread every screen of the Inbox covers. */
    const val INBOX_THREAD = "inbox"

    /** The key a notification stacks and clears under. [sessionRootId] = the row's session root (else the session id). */
    fun notificationThread(type: String, issueId: String?, sessionRootId: String?, deviceId: String?): String = when {
        !issueId.isNullOrEmpty() -> "issue:$issueId"
        !sessionRootId.isNullOrEmpty() -> "session:$sessionRootId"
        type == "agent_signed_out" && !deviceId.isNullOrEmpty() -> "device:$deviceId"
        else -> INBOX_THREAD
    }

    // ── Presentation ─────────────────────────────────────────────────────

    enum class Presentation(val wire: String) { None("none"), Toast("toast"), System("system") }

    /** [platform] = `web` | `desktop` | `ios` | `android`; [viewing] = the current screen's thread. */
    fun notificationPresentation(
        platform: String,
        active: Boolean,
        viewing: String?,
        thread: String,
        muted: Boolean,
    ): Presentation {
        if (muted) return Presentation.None
        if (!active) return if (platform == "web") Presentation.Toast else Presentation.System
        if (viewing == INBOX_THREAD || viewing == thread) return Presentation.None
        return Presentation.Toast
    }

    // ── Clearing ─────────────────────────────────────────────────────────

    data class ReadRow(val id: String, val thread: String)

    /** The silent push for the rows one write turned read; null = no push. */
    fun dismissPayload(rows: List<ReadRow>, all: Boolean, teamId: String?): Map<String, String>? {
        if (rows.isEmpty()) return null
        val base = mapOf("type" to Keys.DISMISS_TYPE)
        val everything = {
            if (teamId != null) base + mapOf(Keys.ALL to "1", Keys.TEAM_ID to teamId) else base + (Keys.ALL to "1")
        }
        if (all) return everything()
        val ids = rows.map { it.id }.distinct()
        if (ids.size <= MAX_IDS) return base + (Keys.IDS to ids.joinToString(","))
        val threads = rows.map { it.thread }.distinct()
        if (threads.size <= MAX_THREADS) return base + (Keys.THREADS to threads.joinToString(","))
        return everything()
    }

    data class Delivered(val notificationId: String?, val thread: String?, val teamId: String?)

    private fun splitList(value: String?): List<String> =
        value?.split(",")?.filter { it.isNotEmpty() } ?: emptyList()

    /** Does a dismiss push remove this delivered notification? */
    fun dismissMatches(payload: Map<String, String>, delivered: Delivered): Boolean {
        if (payload["type"] != Keys.DISMISS_TYPE) return false
        if (payload[Keys.ALL] == "1") {
            val teamId = payload[Keys.TEAM_ID]
            return teamId.isNullOrEmpty() || delivered.teamId == teamId
        }
        if (delivered.notificationId != null && delivered.notificationId in splitList(payload[Keys.IDS])) return true
        return delivered.thread != null && delivered.thread in splitList(payload[Keys.THREADS])
    }

    // ── Source switches ──────────────────────────────────────────────────

    /** The source of a row naming a session, read off the session's ROOT. */
    fun notificationSource(
        agentId: String?,
        agentRole: String?,
        startedByAgentId: String?,
        startedByAgentRole: String?,
    ): String {
        val id = agentId ?: startedByAgentId
        if (id.isNullOrEmpty()) return "temporary"
        val role = if (!agentId.isNullOrEmpty()) agentRole else startedByAgentRole
        return if (role == "assistant") "assistant" else "agent:$id"
    }

    /** `user_notification_prefs.source_prefs`; null / absent = on. */
    data class SourcePrefs(
        val assistant: Boolean? = null,
        val temporary: Boolean? = null,
        val agents: Map<String, Boolean>? = null,
    )

    fun sourceAllowed(prefs: SourcePrefs?, source: String?): Boolean {
        if (source == null || prefs == null) return true
        return when {
            source == "assistant" -> prefs.assistant != false
            source == "temporary" -> prefs.temporary != false
            source.startsWith("agent:") -> prefs.agents?.get(source.removePrefix("agent:")) != false
            else -> true
        }
    }

    /** Is the switch with this settings row [key] on? */
    fun switchOn(prefs: SourcePrefs?, key: String): Boolean = sourceAllowed(prefs, key)

    /** The prefs after one switch flips (`key` = a settings row's key). */
    fun toggleSourcePref(prefs: SourcePrefs, key: String, on: Boolean): SourcePrefs = when {
        key == "assistant" -> prefs.copy(assistant = on)
        key == "temporary" -> prefs.copy(temporary = on)
        key.startsWith("agent:") -> prefs.copy(agents = (prefs.agents ?: emptyMap()) + (key.removePrefix("agent:") to on))
        else -> prefs
    }

    // ── Settings ─────────────────────────────────────────────────────────

    data class SettingsTeam(val id: String, val name: String)
    data class SettingsAgent(val id: String, val teamId: String, val name: String, val role: String)
    data class SettingsRow(val key: String, val label: String, val detail: String?)
    data class SettingsGroup(val band: String, val rows: List<SettingsRow>)

    fun settingsRows(teams: List<SettingsTeam>, agents: List<SettingsAgent>): List<SettingsGroup> {
        val groups = mutableListOf(
            SettingsGroup(
                Copy.SESSIONS_BAND,
                listOf(
                    SettingsRow("assistant", Copy.ASSISTANT, null),
                    SettingsRow("temporary", Copy.TEMPORARY, null),
                ),
            ),
        )
        val teamNames = teams.associate { it.id to it.name }
        // The TS reference compares by UTF-16 code units, exactly like String.compareTo.
        val listed = agents
            .filter { it.role != "assistant" && it.teamId in teamNames }
            .map { it to teamNames.getValue(it.teamId) }
            .sortedWith(compareBy<Pair<SettingsAgent, String>>({ it.second }, { it.first.name }, { it.first.id }))
        if (listed.isNotEmpty()) {
            val many = teams.size > 1
            groups += SettingsGroup(
                Copy.AGENTS_BAND,
                listed.map { (agent, team) -> SettingsRow("agent:${agent.id}", agent.name, if (many) team else null) },
            )
        }
        return groups
    }

    // ── Android channels ─────────────────────────────────────────────────

    data class AndroidChannel(val id: String, val name: String?, val group: String?)

    fun androidChannel(source: String?, sourceName: String?): AndroidChannel = when {
        source == "assistant" -> AndroidChannel(Channels.ASSISTANT_ID, Channels.ASSISTANT_NAME, null)
        source == "temporary" -> AndroidChannel(Channels.TEMPORARY_ID, Channels.TEMPORARY_NAME, null)
        source != null && source.startsWith("agent:") -> AndroidChannel(
            Channels.AGENT_PREFIX + source.removePrefix("agent:"),
            sourceName?.takeIf { it.isNotEmpty() } ?: Channels.AGENTS_GROUP_NAME,
            Channels.AGENTS_GROUP_ID,
        )
        else -> AndroidChannel(Channels.FALLBACK, null, null)
    }
}
