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

// Action triggers (EXP-530; SLOP-2: an action CARRIES its triggers). One
// stored trigger ([ActionTrigger]) = its runner (id, enabled flag, bound
// device, optional agent pins) plus the WHEN-part ([AutomationTrigger]: a
// schedule — "daily at 07:00" — or an event — "when status changes"). They
// ride the synced `actions.triggers` jsonb array; the bound device watches
// for the when-part and fires locally — there is no server scheduler, and a
// phone never runs one.
// Parsing is deliberately TOLERANT — an unknown kind/event/source or malformed
// JSON reads as null ("no trigger"), never a crash — so a future trigger shape
// can't brick this client. Summary strings mirror the web's
// `lib/action-triggers.ts` / iOS byte-for-byte.

/**
 * The event-trigger filter lists. Empty lists mean "no filter on that axis";
 * [priorities] carries wire priority values, the id lists uuids.
 */
data class AutomationTriggerFilters(
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

sealed interface AutomationTrigger {

    /** `kind: "schedule"` — fires on the bound device's LOCAL clock. */
    data class Schedule(
        /** Contract `actionScheduleIntervalValues`: daily | weekly | monthly. */
        val interval: String,
        /** Minutes past local midnight, 0..1439. */
        val minuteOfDay: Int,
        /** 1 = Monday … 7 = Sunday; present iff weekly. */
        val weekday: Int? = null,
        /** 1…28; present iff monthly. */
        val dayOfMonth: Int? = null,
    ) : AutomationTrigger

    /** `kind: "event"` — fires when a matching issue event syncs to the device. */
    data class Event(
        /** Contract `actionTriggerEventValues` (created, status_changed, …). */
        val event: String,
        val filters: AutomationTriggerFilters = AutomationTriggerFilters(),
    ) : AutomationTrigger

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
        fun parse(raw: String?): AutomationTrigger? {
            if (raw.isNullOrBlank()) return null
            val obj = runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonObject
                ?: return null
            return parse(obj)
        }

        /** The same tolerant read off an already-decoded object — the runner
         * fields beside the when-part (a stored trigger's id, device, pins)
         * are simply ignored. */
        fun parse(obj: JsonObject): AutomationTrigger? {
            return when (obj.stringValue("kind")) {
                "schedule" -> {
                    val interval = obj.stringValue("interval")
                        ?.takeIf { it in DomainContract.actionScheduleIntervalValues }
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
                        ?.takeIf { it in DomainContract.actionTriggerEventValues }
                        ?: return null
                    val rawFilters = obj["filters"] as? JsonObject
                    Event(
                        event = event,
                        filters = AutomationTriggerFilters(
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

/**
 * The one-line trigger sentence, byte-matching the web's `triggerSummary` /
 * iOS `AutomationTriggerDisplay.summary`: `Daily at 07:00` / `Weekly on Monday
 * at 09:00` / `Monthly on day 5 at 09:00`; `When status changes` (+ ` · N
 * filters` when any filter ids are present).
 */
fun triggerSummary(trigger: AutomationTrigger): String = when (trigger) {
    is AutomationTrigger.Schedule -> {
        val at = triggerClock(trigger.minuteOfDay)
        when (trigger.interval) {
            "weekly" -> "Weekly on ${triggerWeekdayName(trigger.weekday ?: 1)} at $at"
            "monthly" -> "Monthly on day ${trigger.dayOfMonth ?: 1} at $at"
            else -> "Daily at $at"
        }
    }
    is AutomationTrigger.Event -> {
        val label = triggerEventLabel(trigger.event)
        val sentence = "When ${label.replaceFirstChar { it.lowercaseChar() }}"
        val count = trigger.filters.totalCount
        val noun = if (count == 1) "filter" else "filters"
        if (count > 0) "$sentence · $count $noun" else sentence
    }
}

/**
 * The next occurrence STRICTLY after [nowMs], as epoch millis in [zone].
 * This is the VIEWER's wall clock — the bound device fires on ITS OWN local
 * time, which is why no surface prints this as an absolute date any more
 * (EXP-812: the calendar moved it under every screenshot). A Triggers row
 * labels the RECURRENCE "(device time)" instead. Null only for a malformed
 * schedule (weekly without a valid weekday, monthly without a valid day),
 * which the tolerant parse already rejects.
 */
fun nextScheduleRun(
    schedule: AutomationTrigger.Schedule,
    nowMs: Long = System.currentTimeMillis(),
    zone: TimeZone = TimeZone.getDefault(),
): Long? {
    val hours = schedule.minuteOfDay / 60
    val minutes = schedule.minuteOfDay % 60

    fun atTime(base: Calendar): Calendar = (base.clone() as Calendar).apply {
        set(Calendar.HOUR_OF_DAY, hours)
        set(Calendar.MINUTE, minutes)
        set(Calendar.SECOND, 0)
        set(Calendar.MILLISECOND, 0)
    }

    val now = Calendar.getInstance(zone).apply { timeInMillis = nowMs }

    return when (schedule.interval) {
        "daily" -> {
            val today = atTime(now)
            if (today.timeInMillis > nowMs) {
                today.timeInMillis
            } else {
                atTime(now.cloneShiftedDays(1)).timeInMillis
            }
        }
        "weekly" -> {
            val weekday = schedule.weekday?.takeIf { it in 1..7 } ?: return null
            for (offset in 0..7) {
                val day = now.cloneShiftedDays(offset)
                // Calendar DAY_OF_WEEK is 1=Sun…7=Sat; wire is 1=Mon…7=Sun.
                val isoWeekday = (day.get(Calendar.DAY_OF_WEEK) + 5) % 7 + 1
                if (isoWeekday != weekday) continue
                val candidate = atTime(day)
                if (candidate.timeInMillis > nowMs) return candidate.timeInMillis
            }
            null
        }
        "monthly" -> {
            val day = schedule.dayOfMonth?.takeIf { it in 1..28 } ?: return null
            // dayOfMonth caps at 28, so the day exists in every month.
            val thisMonth = atTime(now).apply { set(Calendar.DAY_OF_MONTH, day) }
            if (thisMonth.timeInMillis > nowMs) {
                thisMonth.timeInMillis
            } else {
                thisMonth.apply { add(Calendar.MONTH, 1) }.timeInMillis
            }
        }
        else -> null
    }
}

private fun Calendar.cloneShiftedDays(days: Int): Calendar =
    (clone() as Calendar).apply { add(Calendar.DAY_OF_YEAR, days) }

/** The one event source this build reads and writes. */
const val TRIGGER_SOURCE_EXPONENTIAL = "exponential"

/**
 * ONE stored trigger of an action: the runner plus its [whenPart]. [id] is
 * stable — it is also what a triggered start passes as `automationId`, so a
 * run's `coding_sessions.automation_id` names the trigger that fired it. Null
 * [agent]/[account]/[model]/[effort] mean the bound device's own launch
 * defaults.
 */
data class ActionTrigger(
    val id: String,
    val enabled: Boolean = true,
    /** Steer deviceId (= devices.device_id) of the machine that runs it. */
    val deviceId: String,
    val agent: String? = null,
    /** The agent profile id on the bound device (belongs to [agent]). */
    val account: String? = null,
    val model: String? = null,
    val effort: String? = null,
    val whenPart: AutomationTrigger,
) {
    /** This trigger as an `actions.update` array element (its id kept). */
    fun toWireJson(): JsonObject = actionTriggerWireJson(
        id = id,
        enabled = enabled,
        deviceId = deviceId,
        agent = agent,
        account = account,
        model = model,
        effort = effort,
        whenPart = whenPart,
    )
}

/**
 * One element of the `triggers` array `actions.update` REPLACES WHOLE: the
 * runner, then the when-part's keys ([AutomationTrigger.toWireJson]) with an
 * event's `source` beside its kind. A null [id] is a NEW trigger (the server
 * mints the id); an unset (null/blank) pin is ABSENT, and [account] rides only
 * beside its agent. The server's schema is strict — no other key travels.
 */
fun actionTriggerWireJson(
    id: String?,
    enabled: Boolean,
    deviceId: String,
    agent: String?,
    account: String?,
    model: String?,
    effort: String?,
    whenPart: AutomationTrigger,
): JsonObject = buildJsonObject {
    id?.let { put("id", it) }
    put("enabled", enabled)
    put("deviceId", deviceId)
    val pinnedAgent = agent?.takeIf { it.isNotEmpty() }
    pinnedAgent?.let { put("agent", it) }
    account?.takeIf { it.isNotEmpty() && pinnedAgent != null }?.let { put("account", it) }
    model?.takeIf { it.isNotEmpty() }?.let { put("model", it) }
    effort?.takeIf { it.isNotEmpty() }?.let { put("effort", it) }
    whenPart.toWireJson().forEach { (key, value) ->
        put(key, value)
        if (key == "kind" && whenPart is AutomationTrigger.Event) {
            put("source", TRIGGER_SOURCE_EXPONENTIAL)
        }
    }
}

private val triggersJson = Json { ignoreUnknownKeys = true }

/**
 * Tolerant read of ONE stored trigger: a trigger without a string id or
 * device, or with an unreadable when-part, is null. Only an explicit `false`
 * pauses — a missing `enabled` is an enabled trigger. Never throws.
 */
fun parseActionTrigger(value: JsonElement?): ActionTrigger? {
    val obj = value as? JsonObject ?: return null
    val whenPart = AutomationTrigger.parse(obj) ?: return null
    fun string(key: String): String? =
        (obj[key] as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it.isNotEmpty() }
    val id = string("id") ?: return null
    val deviceId = string("deviceId") ?: return null
    val enabled = (obj["enabled"] as? JsonPrimitive)
        ?.takeIf { !it.isString }?.content != "false"
    return ActionTrigger(
        id = id,
        enabled = enabled,
        deviceId = deviceId,
        agent = string("agent"),
        account = string("account"),
        model = string("model"),
        effort = string("effort"),
        whenPart = whenPart,
    )
}

/**
 * Tolerant read of an action's `triggers` (the synced column's raw JSON text):
 * the readable ones, in array order. Anything else — null, malformed JSON, a
 * non-array — reads as no triggers.
 */
fun parseActionTriggers(raw: String?): List<ActionTrigger> {
    if (raw.isNullOrBlank()) return emptyList()
    val array = runCatching { triggersJson.parseToJsonElement(raw) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return parseActionTriggers(array)
}

/** The same read off an already-decoded array (the tRPC answers). */
fun parseActionTriggers(array: JsonArray?): List<ActionTrigger> =
    array.orEmpty().mapNotNull(::parseActionTrigger)

/** One trigger glyph of an action row: drawn, and [active] while at least one
 * trigger of its kind is enabled (a paused kind is muted). */
data class TriggerBadge(val active: Boolean)

/**
 * What an action row's trigger glyphs draw: [schedule] (clock) and [event]
 * (bolt), each present when the action has a trigger of that kind (web
 * `triggerBadges`).
 */
data class TriggerBadges(
    val schedule: TriggerBadge? = null,
    val event: TriggerBadge? = null,
) {
    val isEmpty: Boolean get() = schedule == null && event == null
}

fun triggerBadges(triggers: List<ActionTrigger>): TriggerBadges {
    fun badge(ofKind: List<ActionTrigger>): TriggerBadge? =
        if (ofKind.isEmpty()) null else TriggerBadge(active = ofKind.any { it.enabled })
    return TriggerBadges(
        schedule = badge(triggers.filter { it.whenPart is AutomationTrigger.Schedule }),
        event = badge(triggers.filter { it.whenPart is AutomationTrigger.Event }),
    )
}

/** A suggestion's glyph: the seed's own when-part, always drawn active. */
fun triggerBadges(whenPart: AutomationTrigger?): TriggerBadges = when (whenPart) {
    is AutomationTrigger.Schedule -> TriggerBadges(schedule = TriggerBadge(active = true))
    is AutomationTrigger.Event -> TriggerBadges(event = TriggerBadge(active = true))
    null -> TriggerBadges()
}

/**
 * The trigger sentence as a Triggers row prints it. A schedule fires on the
 * BOUND MACHINE's wall clock, so the recurrence carries the caveat the row
 * used to hang off an absolute next-run date (EXP-812).
 */
fun triggerCaption(whenPart: AutomationTrigger): String =
    if (whenPart is AutomationTrigger.Schedule) {
        "${triggerSummary(whenPart)} (device time)"
    } else {
        triggerSummary(whenPart)
    }

/**
 * A run's title in its OWN action's Runs list (×4, web `actionRunTitle`):
 * every row there ran the same action, so the row says what started it
 * instead of repeating the action's name. Any other non-empty reason is
 * another run starting it (`agent`, `workflow`, or one added later).
 */
fun actionRunTitle(startedReason: String?): String = when {
    startedReason == "schedule" -> "Scheduled run"
    startedReason == "event" -> "Event run"
    !startedReason.isNullOrEmpty() -> "Agent run"
    else -> "Manual run"
}

/**
 * The machine-readable block a suggestion appends to the builtin "Create
 * action" request — the creator agent creates the action, then passes this
 * JSON verbatim as `triggers` to `exponential_actions_update`. The client
 * never talks to the server itself. BYTE-IDENTICAL ×4 (web
 * `formatTriggerBlock`): the when-part's keys first, in their stored order,
 * then `deviceId`, then the set pins, compact JSON.
 */
fun formatTriggerBlock(
    trigger: AutomationTrigger,
    deviceId: String,
    agent: String? = null,
    model: String? = null,
    effort: String? = null,
): String {
    val payload = buildJsonObject {
        trigger.toWireJson().forEach { (key, value) -> put(key, value) }
        put("deviceId", deviceId)
        if (!agent.isNullOrEmpty()) put("agent", agent)
        if (!model.isNullOrEmpty()) put("model", model)
        if (!effort.isNullOrEmpty()) put("effort", effort)
    }
    return "\n\nTrigger — after creating the action, call " +
        "exponential_actions_update with its id and `triggers` set to exactly this array: " +
        "`[$payload]`. A triggered run fills no inputs, so declare none as required."
}
