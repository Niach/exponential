package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * NEW-2: THE schedule picker is fixture-locked ×4 — every `cronCases` (both
 * ways), `parseCases`, `weekdaysLabelCases` and `summaryCases` entry of
 * `schedule-picker.json` runs here (TS `schedule-picker.test.ts`); the
 * embedded copy, defaults and test ids are proved equal to the fixture's.
 */
class SchedulePickerTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("schedule-picker.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun scheduleOf(json: JsonObject): Schedule = Schedule(
        repeat = json.str("repeat")!!,
        minute = json.getValue("minute").jsonPrimitive.int,
        hour = json.getValue("hour").jsonPrimitive.int,
        weekdays = json.getValue("weekdays").jsonArray.map { it.jsonPrimitive.int },
        dayOfMonth = json.getValue("dayOfMonth").jsonPrimitive.int,
    )

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    @Test
    fun `every cron case writes its canonical cron and reads back`() {
        for (case in cases("cronCases")) {
            val schedule = scheduleOf(case.getValue("schedule").jsonObject)
            val cron = case.str("cron")!!
            assertEquals(case.str("name"), cron, SchedulePicker.cronFromSchedule(schedule))
            assertEquals(case.str("name"), schedule, SchedulePicker.scheduleFromCron(cron))
        }
    }

    @Test
    fun `every parse case matches the fixture`() {
        for (case in cases("parseCases")) {
            val cron = case.str("cron")!!
            val expected = case.getValue("schedule").takeUnless { it is JsonNull }?.jsonObject?.let(::scheduleOf)
            assertEquals(cron, expected, SchedulePicker.scheduleFromCron(cron))
        }
    }

    @Test
    fun `every weekdays label case matches the fixture`() {
        for (case in cases("weekdaysLabelCases")) {
            val weekdays = case.getValue("weekdays").jsonArray.map { it.jsonPrimitive.int }
            assertEquals(weekdays.toString(), case.str("expected"), SchedulePicker.weekdaysLabel(weekdays))
        }
    }

    @Test
    fun `every summary case matches the fixture`() {
        for (case in cases("summaryCases")) {
            assertEquals(
                case.str("cron"),
                case.str("expected"),
                SchedulePicker.scheduleSummary(case.str("cron")!!, case.str("tz")),
            )
        }
    }

    @Test
    fun `the copy equals the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        val rows = copy.getValue("rows").jsonObject
        assertEquals(rows.str("repeat"), SchedulePicker.Copy.ROW_REPEAT)
        assertEquals(rows.str("days"), SchedulePicker.Copy.ROW_DAYS)
        assertEquals(rows.str("day"), SchedulePicker.Copy.ROW_DAY)
        assertEquals(rows.str("minute"), SchedulePicker.Copy.ROW_MINUTE)
        assertEquals(rows.str("time"), SchedulePicker.Copy.ROW_TIME)
        assertEquals(rows.str("timeZone"), SchedulePicker.Copy.ROW_TIME_ZONE)
        assertEquals(rows.str("cron"), SchedulePicker.Copy.ROW_CRON)
        assertEquals(7, rows.size)
        val repeat = copy.getValue("repeat").jsonObject
        assertEquals(repeat.keys - "custom", SchedulePicker.Copy.REPEAT.keys)
        for ((key, label) in SchedulePicker.Copy.REPEAT) assertEquals(repeat.str(key), label)
        assertEquals(repeat.str("custom"), SchedulePicker.Copy.REPEAT_CUSTOM)
        assertEquals(copy.getValue("weekdays").jsonArray.map { it.jsonPrimitive.content }, SchedulePicker.Copy.WEEKDAYS)
        assertEquals(
            copy.getValue("weekdaysShort").jsonArray.map { it.jsonPrimitive.content },
            SchedulePicker.Copy.WEEKDAYS_SHORT,
        )
        assertEquals(copy.str("weekdaysAll"), SchedulePicker.Copy.WEEKDAYS_ALL)
        assertEquals(copy.str("weekdaysWork"), SchedulePicker.Copy.WEEKDAYS_WORK)
        assertEquals(copy.str("weekdaysWeekend"), SchedulePicker.Copy.WEEKDAYS_WEEKEND)
        assertEquals(copy.str("dayOfMonth"), SchedulePicker.Copy.DAY_OF_MONTH)
        assertEquals(copy.str("minuteOfHour"), SchedulePicker.Copy.MINUTE_OF_HOUR)
        assertEquals(copy.str("timeZonePlaceholder"), SchedulePicker.Copy.TIME_ZONE_PLACEHOLDER)
        assertEquals(copy.str("cronPlaceholder"), SchedulePicker.Copy.CRON_PLACEHOLDER)
        val summary = copy.getValue("summary").jsonObject
        assertEquals(summary.str("hourly"), SchedulePicker.Copy.SUMMARY_HOURLY)
        assertEquals(summary.str("daily"), SchedulePicker.Copy.SUMMARY_DAILY)
        assertEquals(summary.str("weekly"), SchedulePicker.Copy.SUMMARY_WEEKLY)
        assertEquals(summary.str("weeklyOne"), SchedulePicker.Copy.SUMMARY_WEEKLY_ONE)
        assertEquals(summary.str("monthly"), SchedulePicker.Copy.SUMMARY_MONTHLY)
        assertEquals(summary.str("custom"), SchedulePicker.Copy.SUMMARY_CUSTOM)
        assertEquals(summary.str("zoned"), SchedulePicker.Copy.SUMMARY_ZONED)
        assertEquals(7, summary.size)
        val errors = copy.getValue("errors").jsonObject
        assertEquals(errors.str("noDays"), SchedulePicker.Copy.ERROR_NO_DAYS)
        assertEquals(errors.str("cron"), SchedulePicker.Copy.ERROR_CRON)
        assertEquals(2, errors.size)
        // Every copy key is mirrored above.
        assertEquals(
            setOf(
                "rows", "repeat", "weekdays", "weekdaysShort", "weekdaysAll", "weekdaysWork", "weekdaysWeekend",
                "dayOfMonth", "minuteOfHour", "timeZonePlaceholder", "cronPlaceholder", "summary", "errors",
            ),
            copy.keys,
        )
    }

    @Test
    fun `the defaults, repeats, day max and test ids equal the fixture`() {
        assertEquals(scheduleOf(fixture.getValue("defaults").jsonObject), SchedulePicker.defaults())
        assertEquals(
            fixture.getValue("repeatValues").jsonArray.map { it.jsonPrimitive.content },
            SchedulePicker.REPEAT_VALUES,
        )
        assertEquals(fixture.getValue("dayOfMonthMax").jsonPrimitive.int, SchedulePicker.DAY_OF_MONTH_MAX)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("repeat"), SchedulePicker.TEST_ID_REPEAT)
        assertEquals(ids.str("days"), SchedulePicker.TEST_ID_DAYS)
        assertEquals(ids.str("day"), SchedulePicker.TEST_ID_DAY)
        assertEquals(ids.str("minute"), SchedulePicker.TEST_ID_MINUTE)
        assertEquals(ids.str("time"), SchedulePicker.TEST_ID_TIME)
        assertEquals(ids.str("timeZone"), SchedulePicker.TEST_ID_TIME_ZONE)
        assertEquals(ids.str("cron"), SchedulePicker.TEST_ID_CRON)
        assertEquals(7, ids.size)
    }

    @Test
    fun `the contract zones start at UTC`() {
        assertEquals("UTC", DomainContract.timeZoneNames.first())
        assertTrue(DomainContract.timeZoneNames.contains("Europe/Vienna"))
    }

    @Test
    fun `a draft reads, validates and re-picks like the web rows`() {
        val read = scheduleDraftFromCron("  0  9 * * 1-5 ", null)
        assertEquals("UTC", read.tz)
        assertEquals("0 9 * * 1,2,3,4,5", scheduleDraftCron(read))
        assertTrue(validateScheduleDraft(read).isEmpty())

        val custom = scheduleDraftFromCron("*/10  * * * 1-5", "Europe/Vienna")
        assertNull(custom.schedule)
        assertEquals("*/10 * * * 1-5", scheduleDraftCron(custom))
        assertTrue(validateScheduleDraft(custom).isEmpty())
        assertEquals(
            listOf(SchedulePicker.Copy.ERROR_CRON),
            validateScheduleDraft(custom.copy(cron = "0 9 * *")),
        )

        val fresh = freshScheduleDraft("Europe/Vienna")
        assertEquals(SchedulePicker.defaults(), fresh.schedule)
        assertEquals("0 9 * * *", scheduleDraftCron(fresh))

        val weekly = pickRepeat(fresh.copy(schedule = fresh.schedule!!.copy(weekdays = emptyList())), "weekly")
        assertEquals(listOf(1, 2, 3, 4, 5), weekly.schedule!!.weekdays)
        assertEquals(
            listOf(SchedulePicker.Copy.ERROR_NO_DAYS),
            validateScheduleDraft(weekly.copy(schedule = weekly.schedule!!.copy(weekdays = emptyList()))),
        )

        val monthly = pickRepeat(custom, "monthly")
        assertEquals(1, monthly.schedule!!.dayOfMonth)
        assertEquals("", monthly.cron)
        assertEquals("Europe/Vienna", monthly.tz)
    }
}
