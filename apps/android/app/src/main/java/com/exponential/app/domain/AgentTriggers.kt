package com.exponential.app.domain

import java.util.Calendar
import java.util.TimeZone
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

// Agent triggers (EXP-1291 Threads v2; the SLOP-2 action triggers moved onto
// the team AGENT). One stored trigger ([AgentTrigger]) = its id + enabled
// flag + the WHEN-part ([TriggerWhen]: a schedule — "daily at 07:00" — or an
// event — "when status changes"). The runner is the agent itself (its
// device, account, model). They ride the synced `sessions.triggers` jsonb
// array of the agent root; the agent's device watches for the when-part and
// posts the firing as a message into the agent's channel — there is no
// server scheduler, and a phone never runs one.
// Parsing is deliberately TOLERANT — an unknown kind/event/source or malformed
// JSON reads as null ("no trigger"), never a crash — so a future trigger shape
// can't brick this client. Summary strings mirror the web's
// `lib/action-triggers.ts` / iOS byte-for-byte.

/**
 * The event-trigger filter lists. Empty lists mean "no filter on that axis";
 * [priorities] carries wire priority values, the id lists uuids.
 */
data class TriggerFilters(
    val boardIds: List<String> = emptyList(),
    val labelIds: List<String> = emptyList(),
    val priorities: List<String> = emptyList(),
    val toStatusIds: List<String> = emptyList(),
) {
    /** Total picked entries across every list — the ` · N filters` count. */
    val totalCount: Int
        get() = boardIds.size + labelIds.size + priorities.size + toStatusIds.size

    val isEmpty: Boolean get() = totalCount == 0
}

sealed interface TriggerWhen {

    /** `kind: "schedule"` — fires on the bound device's LOCAL clock. */
    data class Schedule(
        /** Contract `agentScheduleIntervalValues`: daily | weekly | monthly. */
        val interval: String,
        /** Minutes past local midnight, 0..1439. */
        val minuteOfDay: Int,
        /** 1 = Monday … 7 = Sunday; present iff weekly. */
        val weekday: Int? = null,
        /** 1…28; present iff monthly. */
        val dayOfMonth: Int? = null,
    ) : TriggerWhen

    /** `kind: "event"` — fires when a matching issue event syncs to the device. */
    data class Event(
        /** Contract `agentTriggerEventValues` (created, status_changed, …). */
        val event: String,
        val filters: TriggerFilters = TriggerFilters(),
    ) : TriggerWhen

    /**
     * The wire JSON object with the server's field names, keys in the
     * canonical order (kind, interval/minuteOfDay/weekday/dayOfMonth or
     * event/filters{boardIds,labelIds,priorities,toStatusIds}) — the SAME
     * order the web's `draftToTrigger` builds, so `JSON.stringify` and this
     * render byte-identically inside [formatTriggerBlock]. Empty filter
     * lists are OMITTED, matching what the pickers produce.
     */
    fun toWireJson(): JsonObject = when (this) {
        is Schedule -> buildJsonObject {
            put("kind", "schedule")
            put("interval", interval)
            put("minuteOfDay", minuteOfDay)
            weekday?.let { put("weekday", it) }
            dayOfMonth?.let { put("dayOfMonth", it) }
        }
        is Event -> buildJsonObject {
            put("kind", "event")
            put("event", event)
            if (!filters.isEmpty) {
                putJsonObject("filters") {
                    if (filters.boardIds.isNotEmpty()) {
                        putJsonArray("boardIds") { filters.boardIds.forEach { add(JsonPrimitive(it)) } }
                    }
                    if (filters.labelIds.isNotEmpty()) {
                        putJsonArray("labelIds") { filters.labelIds.forEach { add(JsonPrimitive(it)) } }
                    }
                    if (filters.priorities.isNotEmpty()) {
                        putJsonArray("priorities") { filters.priorities.forEach { add(JsonPrimitive(it)) } }
                    }
                    if (filters.toStatusIds.isNotEmpty()) {
                        putJsonArray("toStatusIds") { filters.toStatusIds.forEach { add(JsonPrimitive(it)) } }
                    }
                }
            }
        }
    }

    /** Compact wire JSON string (kotlinx renders JsonObject compactly). */
    fun toWireJsonString(): String = toWireJson().toString()

    companion object {
        private val json = Json { ignoreUnknownKeys = true }

        /**
         * Tolerant parse of a when-part JSON string. ANY malformation —
         * unknown kind, unknown event/interval/source, missing schedule
         * fields, out-of-range values, non-object JSON — reads as null ("no
         * trigger"): a future server shape must never crash or half-render on
         * this build.
         */
        fun parse(raw: String?): TriggerWhen? {
            if (raw.isNullOrBlank()) return null
            val obj = runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonObject
                ?: return null
            return parse(obj)
        }

        /** The same tolerant read off an already-decoded object — the fields
         * beside the when-part (a stored trigger's id, enabled) are ignored. */
        fun parse(obj: JsonObject): TriggerWhen? {
            return when (obj.stringValue("kind")) {
                "schedule" -> {
                    val interval = obj.stringValue("interval")
                        ?.takeIf { it in DomainContract.agentScheduleIntervalValues }
                        ?: return null
                    val minuteOfDay = obj.intValue("minuteOfDay")
                        ?.takeIf { it in 0..1439 }
                        ?: return null
                    val weekday = obj.intValue("weekday")
                    val dayOfMonth = obj.intValue("dayOfMonth")
                    if (interval == "weekly" && (weekday == null || weekday !in 1..7)) return null
                    if (interval == "monthly" && (dayOfMonth == null || dayOfMonth !in 1..28)) return null
                    Schedule(
                        interval = interval,
                        minuteOfDay = minuteOfDay,
                        weekday = weekday.takeIf { interval == "weekly" },
                        dayOfMonth = dayOfMonth.takeIf { interval == "monthly" },
                    )
                }
                "event" -> {
                    // An old client reading a FUTURE event source or kind
                    // treats it as "no trigger". An absent source is
                    // Exponential's (rows from before sources existed).
                    if (obj.containsKey("source") &&
                        obj.stringValue("source") != TRIGGER_SOURCE_EXPONENTIAL
                    ) {
                        return null
                    }
                    val event = obj.stringValue("event")
                        ?.takeIf { it in DomainContract.agentTriggerEventValues }
                        ?: return null
                    val rawFilters = obj["filters"] as? JsonObject
                    Event(
                        event = event,
                        filters = TriggerFilters(
                            boardIds = rawFilters.stringList("boardIds"),
                            labelIds = rawFilters.stringList("labelIds"),
                            priorities = rawFilters.stringList("priorities")
                                .filter { it in DomainContract.issuePriorityValues },
                            toStatusIds = rawFilters.stringList("toStatusIds"),
                        ),
                    )
                }
                else -> null
            }
        }

        private fun JsonObject.stringValue(key: String): String? =
            (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

        // Postgres jsonb numbers can arrive as ints or whole-number doubles.
        private fun JsonObject.intValue(key: String): Int? {
            val content = (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content ?: return null
            content.toIntOrNull()?.let { return it }
            val double = content.toDoubleOrNull() ?: return null
            return if (double == Math.floor(double)) double.toInt() else null
        }

        private fun JsonObject?.stringList(key: String): List<String> {
            val array = this?.get(key) as? JsonArray ?: return emptyList()
            return array.mapNotNull { entry ->
                (entry as? JsonPrimitive)?.takeIf { it.isString }?.content
                    ?.takeIf { it.isNotEmpty() }
            }
        }
    }
}

/** ISO weekday name, 1 = Monday … 7 = Sunday (the wire convention). */
fun triggerWeekdayName(weekday: Int): String =
    TRIGGER_WEEKDAY_NAMES.getOrElse(weekday - 1) { "Monday" }

private val TRIGGER_WEEKDAY_NAMES = listOf(
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
)

/** `07:00` — zero-padded 24h clock from minutes past midnight. */
fun triggerClock(minuteOfDay: Int): String =
    String.format(java.util.Locale.ROOT, "%02d:%02d", minuteOfDay / 60, minuteOfDay % 60)

/**
 * Event picker labels — [triggerSummary] derives its "When …" sentence from
 * these, so the two surfaces can never disagree (web TRIGGER_EVENT_LABELS).
 */
fun triggerEventLabel(event: String): String = when (event) {
    "created" -> "An issue is created"
    "status_changed" -> "Status changes"
    "assignee_changed" -> "The assignee changes"
    "label_added" -> "A label is added"
    "priority_changed" -> "Priority changes"
    "pr_opened" -> "A pull request is opened"
    "pr_merged" -> "A pull request is merged"
    else -> "An issue changes"
}

/** The one event source this build reads and writes. */
const val TRIGGER_SOURCE_EXPONENTIAL = "exponential"

/**
 * ONE stored trigger of an agent: its [id] (null = new, the server mints it),
 * the [enabled] flag and the [whenPart]. Its row text is
 * [AgentSettings.triggerSummary].
 */
data class AgentTrigger(
    val id: String?,
    val enabled: Boolean = true,
    val whenPart: TriggerWhen,
) {
    /** This trigger as a `sessions.updateAgent({triggers})` / `sessions.send
     *  ({agent: {triggers}})` array element. */
    fun toWireJson(): JsonObject = agentTriggerWireJson(id, enabled, whenPart)
}

/**
 * One element of the `triggers` array (`agentTriggerSchema`, strict): `id`
 * (absent = new), `enabled`, then the when-part's keys
 * ([TriggerWhen.toWireJson]) with an event's `source` right after its kind.
 * No other key travels.
 */
fun agentTriggerWireJson(id: String?, enabled: Boolean, whenPart: TriggerWhen): JsonObject =
    buildJsonObject {
        id?.let { put("id", it) }
        put("enabled", enabled)
        whenPart.toWireJson().forEach { (key, value) ->
            put(key, value)
            if (key == "kind" && whenPart is TriggerWhen.Event) {
                put("source", TRIGGER_SOURCE_EXPONENTIAL)
            }
        }
    }

private val triggersJson = Json { ignoreUnknownKeys = true }

/**
 * Tolerant read of ONE stored agent trigger: an unreadable when-part is null.
 * Only an explicit `false` pauses — a missing `enabled` is an enabled
 * trigger. Never throws.
 */
fun parseAgentTrigger(value: JsonElement?): AgentTrigger? {
    val obj = value as? JsonObject ?: return null
    val whenPart = TriggerWhen.parse(obj) ?: return null
    val id = (obj["id"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotEmpty() }
    val enabled = (obj["enabled"] as? JsonPrimitive)
        ?.takeIf { !it.isString }?.content != "false"
    return AgentTrigger(id = id, enabled = enabled, whenPart = whenPart)
}

/**
 * Tolerant read of an agent's `triggers` (the synced column's raw JSON text):
 * the readable ones, in array order. Anything else — null, malformed JSON, a
 * non-array — reads as no triggers.
 */
fun parseAgentTriggers(raw: String?): List<AgentTrigger> {
    if (raw.isNullOrBlank()) return emptyList()
    val array = runCatching { triggersJson.parseToJsonElement(raw) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return array.mapNotNull(::parseAgentTrigger)
}

/**
 * The stored trigger objects this build CANNOT read (an event or interval
 * from a newer client or MCP). `triggers` is a whole-array replace, so every
 * write sends these back unchanged, appended — an edit here never erases
 * them on the server.
 */
fun unreadableAgentTriggers(raw: String?): List<JsonObject> {
    if (raw.isNullOrBlank()) return emptyList()
    val array = runCatching { triggersJson.parseToJsonElement(raw) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return array.filterIsInstance<JsonObject>().filter { parseAgentTrigger(it) == null }
}
