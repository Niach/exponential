package com.exponential.app.domain

/**
 * NEW-2: THE schedule picker's pure core ×4 — nobody speaks cron. A schedule
 * trigger STORES a 5-field cron; the rows ([com.exponential.app.ui.components.ScheduleFields])
 * edit it and [SchedulePicker.scheduleSummary] reads it. The Android mirror of
 * `packages/domain-contract/src/schedule-picker.ts` (desktop
 * `domain::schedule_picker`, iOS ExpCore `SchedulePicker.swift`), byte-locked
 * by `fixtures/schedule-picker.json`, which `SchedulePickerTest` replays; the
 * fixture's `$comment` holds the rules and the copy below is its `copy`.
 */
data class Schedule(
    /** One of [SchedulePicker.REPEAT_VALUES]. */
    val repeat: String,
    /** 0..59; unused by `minutes`. */
    val minute: Int,
    /** 0..23; unused by `hourly`. */
    val hour: Int,
    /** ISO weekdays 1 = Monday .. 7 = Sunday, ascending, unique; weekly only. */
    val weekdays: List<Int>,
    /** 1..[SchedulePicker.DAY_OF_MONTH_MAX]; monthly only. */
    val dayOfMonth: Int,
    /** EXP-1363: one of [SchedulePicker.EVERY_VALUES]; minutes only (a read
     *  carries the default otherwise). */
    val every: Int = SchedulePicker.DEFAULT_EVERY,
)

object SchedulePicker {

    /** EXP-1363: every [Schedule.every] minutes (a minute step cron). */
    const val REPEAT_MINUTES = "minutes"
    const val REPEAT_HOURLY = "hourly"
    const val REPEAT_DAILY = "daily"
    const val REPEAT_WEEKLY = "weekly"
    const val REPEAT_MONTHLY = "monthly"
    /** The Repeat row's value while the stored cron is custom (never stored). */
    const val REPEAT_CUSTOM = "custom"

    val REPEAT_VALUES: List<String> = listOf(REPEAT_MINUTES, REPEAT_HOURLY, REPEAT_DAILY, REPEAT_WEEKLY, REPEAT_MONTHLY)
    const val DAY_OF_MONTH_MAX = 28
    /** EXP-1363: the `minutes` repeat's intervals (the fixture's `everyValues`). */
    val EVERY_VALUES: List<Int> = listOf(5, 10, 15, 20, 30)
    const val DEFAULT_EVERY = 10

    /** EXP-1363: a minutes or hourly schedule has no Time zone row and no
     *  zone in its summary (a stored zone is kept as is). */
    fun showsTimeZone(repeat: String?): Boolean = repeat != REPEAT_MINUTES && repeat != REPEAT_HOURLY

    object Copy {
        const val ROW_REPEAT = "Repeat"
        const val ROW_DAYS = "Days"
        const val ROW_DAY = "Day"
        const val ROW_MINUTE = "Minute"
        const val ROW_TIME = "Time"
        const val ROW_TIME_ZONE = "Time zone"
        const val ROW_CRON = "Cron"
        const val ROW_EVERY = "Every"

        /** The Repeat row's options by [REPEAT_VALUES]. */
        val REPEAT: Map<String, String> = linkedMapOf(
            REPEAT_MINUTES to "Every few minutes",
            REPEAT_HOURLY to "Every hour",
            REPEAT_DAILY to "Every day",
            REPEAT_WEEKLY to "Every week",
            REPEAT_MONTHLY to "Every month",
        )
        const val REPEAT_CUSTOM = "Custom"

        /** ISO order, Monday first. */
        val WEEKDAYS: List<String> = listOf("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")
        val WEEKDAYS_SHORT: List<String> = listOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
        const val WEEKDAYS_ALL = "Every day"
        const val WEEKDAYS_WORK = "Weekdays"
        const val WEEKDAYS_WEEKEND = "Weekends"
        const val DAY_OF_MONTH = "Day {day}"
        const val MINUTE_OF_HOUR = ":{minute}"
        const val TIME_ZONE_PLACEHOLDER = "Search time zones"
        const val CRON_PLACEHOLDER = "0 9 * * 1-5"
        /** An Every row option / value. */
        const val EVERY = "{n} minutes"

        const val SUMMARY_MINUTES = "Every {n} minutes"
        const val SUMMARY_HOURLY = "Every hour at :{minute}"
        const val SUMMARY_DAILY = "Every day at {time}"
        const val SUMMARY_WEEKLY = "{days} at {time}"
        const val SUMMARY_WEEKLY_ONE = "Every {weekday} at {time}"
        const val SUMMARY_MONTHLY = "Every month on day {day} at {time}"
        const val SUMMARY_CUSTOM = "Cron {cron}"
        const val SUMMARY_ZONED = "{summary} ({tz})"

        const val ERROR_NO_DAYS = "Pick a day"
        const val ERROR_CRON = "Use five fields: minute hour day month weekday"
    }

    const val TEST_ID_REPEAT = "schedule-repeat"
    const val TEST_ID_DAYS = "schedule-days"
    const val TEST_ID_DAY = "schedule-day"
    const val TEST_ID_MINUTE = "schedule-minute"
    const val TEST_ID_TIME = "schedule-time"
    const val TEST_ID_TIME_ZONE = "schedule-time-zone"
    const val TEST_ID_CRON = "schedule-cron"
    const val TEST_ID_EVERY = "schedule-every"

    /** A new schedule: daily at 09:00 (the fixture's `defaults`). */
    fun defaults(): Schedule = Schedule(
        repeat = REPEAT_DAILY,
        minute = 0,
        hour = 9,
        weekdays = listOf(1, 2, 3, 4, 5),
        dayOfMonth = 1,
        every = DEFAULT_EVERY,
    )

    /** `N minutes`, an Every row option. */
    fun everyLabel(every: Int): String = Copy.EVERY.replace("{n}", every.toString())

    private fun pad2(value: Int): String = value.toString().padStart(2, '0')

    /** The 24h `HH:mm` a Time row reads. */
    fun timeLabel(hour: Int, minute: Int): String = "${pad2(hour)}:${pad2(minute)}"

    /** `:mm`, a Minute row option. */
    fun minuteOfHourLabel(minute: Int): String = Copy.MINUTE_OF_HOUR.replace("{minute}", pad2(minute))

    /** `Day n`, a Day row option. */
    fun dayOfMonthLabel(day: Int): String = Copy.DAY_OF_MONTH.replace("{day}", day.toString())

    /** ISO weekday → cron's (Sunday = 0). */
    private fun toCronWeekday(iso: Int): Int = if (iso == 7) 0 else iso

    /** cron weekday (0 or 7 = Sunday) → ISO. */
    private fun fromCronWeekday(cron: Int): Int = if (cron == 0) 7 else cron

    /** The canonical cron of a schedule (the fixture's `cronCases`). */
    fun cronFromSchedule(schedule: Schedule): String {
        val minute = schedule.minute.toString()
        return when (schedule.repeat) {
            REPEAT_MINUTES -> "*/${schedule.every} * * * *"
            REPEAT_HOURLY -> "$minute * * * *"
            REPEAT_WEEKLY -> {
                val days = schedule.weekdays.map(::toCronWeekday).distinct().sorted()
                "$minute ${schedule.hour} * * ${days.joinToString(",")}"
            }
            REPEAT_MONTHLY -> "$minute ${schedule.hour} ${schedule.dayOfMonth} * *"
            else -> "$minute ${schedule.hour} * * *"
        }
    }

    private val STEP = Regex("^\\*/(\\d{1,2})$")
    private val PLAIN_NUMBER = Regex("^\\d{1,2}$")
    private val WEEKDAY_PART = Regex("^(\\d)(?:-(\\d))?$")
    private val WHITESPACE = Regex("\\s+")

    /** A single plain number within [min]..[max], else null (no lists, no `*`). */
    private fun plainNumber(field: String, min: Int, max: Int): Int? {
        if (!PLAIN_NUMBER.matches(field)) return null
        val value = field.toInt()
        return if (value in min..max) value else null
    }

    /** A cron weekday field as ISO weekdays: plain numbers, lists and ranges
     *  of 0..7 (0 and 7 = Sunday), any order; null for anything else. */
    private fun weekdaysOf(field: String): List<Int>? {
        val out = mutableSetOf<Int>()
        for (part in field.split(",")) {
            val match = WEEKDAY_PART.matchEntire(part) ?: return null
            val lo = match.groupValues[1].toInt()
            val hi = match.groupValues[2].takeIf { it.isNotEmpty() }?.toInt() ?: lo
            if (lo > 7 || hi > 7 || lo > hi) return null
            for (day in lo..hi) out.add(fromCronWeekday(day))
        }
        return out.sorted()
    }

    /** The schedule a cron reads as, or null when the rows cannot express it
     *  (a CUSTOM schedule; the fixture's `parseCases`). */
    fun scheduleFromCron(cron: String): Schedule? {
        val fields = cron.trim().split(WHITESPACE)
        if (fields.size != 5 || fields[0].isEmpty()) return null
        val (minuteField, hourField, domField, monthField, dowField) = fields
        if (monthField != "*") return null
        STEP.matchEntire(minuteField)?.let { step ->
            val every = step.groupValues[1].toInt()
            val plain = hourField == "*" && domField == "*" && dowField == "*"
            return if (plain && every in EVERY_VALUES) {
                Schedule(repeat = REPEAT_MINUTES, minute = 0, hour = 0, weekdays = emptyList(), dayOfMonth = 1, every = every)
            } else {
                null
            }
        }
        val minute = plainNumber(minuteField, 0, 59) ?: return null
        // Only the repeat's own fields are populated: `weekdays` [] unless
        // weekly, `dayOfMonth` 1 unless monthly (the rows seed defaults).
        val base = Schedule(repeat = REPEAT_DAILY, minute = minute, hour = 0, weekdays = emptyList(), dayOfMonth = 1)
        if (hourField == "*") {
            return if (domField == "*" && dowField == "*") base.copy(repeat = REPEAT_HOURLY) else null
        }
        val hour = plainNumber(hourField, 0, 23) ?: return null
        if (domField == "*" && dowField == "*") return base.copy(repeat = REPEAT_DAILY, hour = hour)
        if (domField == "*") {
            val weekdays = weekdaysOf(dowField)
            return if (!weekdays.isNullOrEmpty()) base.copy(repeat = REPEAT_WEEKLY, hour = hour, weekdays = weekdays) else null
        }
        if (dowField != "*") return null
        val dayOfMonth = plainNumber(domField, 1, DAY_OF_MONTH_MAX) ?: return null
        return base.copy(repeat = REPEAT_MONTHLY, hour = hour, dayOfMonth = dayOfMonth)
    }

    /** The Days row's value and the weekly summary's `{days}`. */
    fun weekdaysLabel(weekdays: List<Int>): String {
        val days = weekdays.distinct().sorted()
        return when {
            days.isEmpty() -> ""
            days == listOf(1, 2, 3, 4, 5, 6, 7) -> Copy.WEEKDAYS_ALL
            days == listOf(1, 2, 3, 4, 5) -> Copy.WEEKDAYS_WORK
            days == listOf(6, 7) -> Copy.WEEKDAYS_WEEKEND
            days.size == 1 -> Copy.WEEKDAYS.getOrNull(days[0] - 1) ?: ""
            else -> days.joinToString(", ") { Copy.WEEKDAYS_SHORT.getOrNull(it - 1) ?: "" }
        }
    }

    /** A Triggers row's text for a stored schedule (the fixture's `summaryCases`). */
    fun scheduleSummary(cron: String, tz: String? = null): String {
        val schedule = scheduleFromCron(cron)
        if (schedule != null && !showsTimeZone(schedule.repeat)) {
            // EXP-1363: a zone means nothing for a minutes or hourly schedule.
            return if (schedule.repeat == REPEAT_MINUTES) {
                Copy.SUMMARY_MINUTES.replace("{n}", schedule.every.toString())
            } else {
                Copy.SUMMARY_HOURLY.replace("{minute}", pad2(schedule.minute))
            }
        }
        val text = if (schedule == null) {
            Copy.SUMMARY_CUSTOM.replace("{cron}", cron.trim().replace(WHITESPACE, " "))
        } else {
            val time = timeLabel(schedule.hour, schedule.minute)
            when (schedule.repeat) {
                REPEAT_WEEKLY -> if (schedule.weekdays.size == 1) {
                    Copy.SUMMARY_WEEKLY_ONE.replace("{weekday}", weekdaysLabel(schedule.weekdays)).replace("{time}", time)
                } else {
                    Copy.SUMMARY_WEEKLY.replace("{days}", weekdaysLabel(schedule.weekdays)).replace("{time}", time)
                }
                REPEAT_MONTHLY -> Copy.SUMMARY_MONTHLY.replace("{day}", schedule.dayOfMonth.toString()).replace("{time}", time)
                else -> Copy.SUMMARY_DAILY.replace("{time}", time)
            }
        }
        return if (!tz.isNullOrEmpty()) Copy.SUMMARY_ZONED.replace("{summary}", text).replace("{tz}", tz) else text
    }

    internal fun collapseWhitespace(cron: String): String = cron.trim().replace(WHITESPACE, " ")
}

/**
 * What the rows edit (web `ScheduleDraft`): the structured [schedule], or —
 * for a cron the rows cannot express — null with the expression in [cron];
 * plus the IANA zone [tz], never blank on a draft (a read without one is UTC).
 */
data class ScheduleDraft(
    val schedule: Schedule?,
    /** The raw expression while [schedule] is null (custom). */
    val cron: String,
    val tz: String,
)

/** A stored schedule read into the rows (a cron the rows cannot express =
 *  custom; a missing zone reads as UTC). */
fun scheduleDraftFromCron(cron: String, tz: String?): ScheduleDraft = ScheduleDraft(
    schedule = SchedulePicker.scheduleFromCron(cron),
    cron = SchedulePicker.collapseWhitespace(cron),
    tz = tz?.trim()?.takeIf { it.isNotEmpty() } ?: "UTC",
)

/** A new schedule: daily at 09:00 in [tz] (the device's zone). */
fun freshScheduleDraft(tz: String): ScheduleDraft =
    ScheduleDraft(schedule = SchedulePicker.defaults(), cron = "", tz = tz.trim().ifEmpty { "UTC" })

/** The wire cron of a draft: the structured schedule's canonical form, else
 *  the custom expression (whitespace collapsed). */
fun scheduleDraftCron(draft: ScheduleDraft): String =
    draft.schedule?.let(SchedulePicker::cronFromSchedule) ?: SchedulePicker.collapseWhitespace(draft.cron)

/** Inline errors; empty = savable. A weekly needs a day; a custom cron five fields. */
fun validateScheduleDraft(draft: ScheduleDraft): List<String> {
    val schedule = draft.schedule
    if (schedule != null) {
        return if (schedule.repeat == SchedulePicker.REPEAT_WEEKLY && schedule.weekdays.isEmpty()) {
            listOf(SchedulePicker.Copy.ERROR_NO_DAYS)
        } else {
            emptyList()
        }
    }
    val fields = draft.cron.trim().split(Regex("\\s+"))
    return if (fields.size == 5 && fields[0].isNotEmpty()) emptyList() else listOf(SchedulePicker.Copy.ERROR_CRON)
}

/**
 * A Repeat pick: replaces a custom cron; switching to weekly with no days
 * seeds Monday–Friday, switching to monthly FROM custom seeds day 1.
 */
fun pickRepeat(draft: ScheduleDraft, repeat: String): ScheduleDraft {
    val seed = SchedulePicker.defaults()
    val base = draft.schedule ?: seed
    val next = base.copy(
        repeat = repeat,
        weekdays = if (repeat == SchedulePicker.REPEAT_WEEKLY && base.weekdays.isEmpty()) seed.weekdays else base.weekdays,
        dayOfMonth = if (repeat == SchedulePicker.REPEAT_MONTHLY && draft.schedule == null) seed.dayOfMonth else base.dayOfMonth,
    )
    return draft.copy(schedule = next, cron = "")
}
