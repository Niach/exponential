package com.exponential.app.domain

import java.util.Calendar
import java.util.TimeZone
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

// EXP-583 automations: an `automations` row (the 19th Electric shape) pairs an
// action with a bound device, and its `trigger` jsonb is the WHEN-part only —
// a schedule ("daily at 07:00") or an event ("when status changes"). The
// device/enabled/agent fields that used to live INSIDE the trigger (EXP-530's
// actions.trigger) are columns on the row now. The bound device watches for
// the trigger and fires locally; there is no server scheduler.
// Parsing is deliberately TOLERANT — an unknown kind/event or malformed JSON
// reads as null ("no trigger"), never a crash — so a future trigger shape
// can't brick this client. Summary strings mirror the web's
// `lib/action-triggers.ts` / iOS `AutomationTriggerDisplay` byte-for-byte.

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
     * render byte-identically inside [formatAutomationBlock]. Empty filter
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

    /** Compact wire JSON string — rides `automations.create` and the create
     * sheet's machine-readable note (kotlinx renders JsonObject compactly). */
    fun toWireJsonString(): String = toWireJson().toString()

    companion object {
        private val json = Json { ignoreUnknownKeys = true }

        /**
         * Tolerant parse of the stored/synced JSON string. ANY malformation —
         * unknown kind, unknown event/interval, missing schedule fields,
         * out-of-range values, non-object JSON — reads as null ("no
         * trigger"): a future server shape must never crash or half-render on
         * this build.
         */
        fun parse(raw: String?): AutomationTrigger? {
            if (raw.isNullOrBlank()) return null
            val obj = runCatching { json.parseToJsonElement(raw) }.getOrNull() as? JsonObject
                ?: return null

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
                    // An old client reading a FUTURE event kind treats it as
                    // "no trigger".
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
 * time, so callers must label the result "(device time)". Null only for a
 * malformed schedule (weekly without a valid weekday, monthly without a
 * valid day), which the tolerant parse already rejects.
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

/**
 * The machine-readable note the create sheet appends to the builtin "Create
 * action" description — the creator agent authors the action, then copies this
 * JSON verbatim into `exponential_automations_create` (adding the new action's
 * id). The sheet never talks to the server itself. BYTE-IDENTICAL to web
 * `formatAutomationBlock`: key order deviceId, trigger, then agent/model/effort
 * only when set, compact JSON.
 */
fun formatAutomationBlock(
    trigger: AutomationTrigger,
    deviceId: String,
    agent: String? = null,
    model: String? = null,
    effort: String? = null,
): String {
    val payload = buildJsonObject {
        put("deviceId", deviceId)
        put("trigger", trigger.toWireJson())
        if (!agent.isNullOrEmpty()) put("agent", agent)
        if (!model.isNullOrEmpty()) put("model", model)
        if (!effort.isNullOrEmpty()) put("effort", effort)
    }
    return "\n\nAutomation — after creating the action, call " +
        "exponential_automations_create with its id and exactly these fields: " +
        "`$payload`. An automated run fills no inputs, so declare none as required."
}
