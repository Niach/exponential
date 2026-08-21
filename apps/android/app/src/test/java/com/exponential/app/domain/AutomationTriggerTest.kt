package com.exponential.app.domain

import java.util.Calendar
import java.util.TimeZone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-583: the tolerant when-part trigger parse (anything malformed reads as
// "no trigger", never a throw), the shared summary strings (byte-matching web
// `triggerSummary` / iOS `AutomationTriggerDisplay.summary`), the automation
// note the create sheet appends (byte-matching web `formatAutomationBlock`)
// and the next-run schedule math in a fixed viewer timezone.
class AutomationTriggerTest {

    // ── Tolerant parse ───────────────────────────────────────────────────────

    @Test
    fun parsesASchedule() {
        val trigger = AutomationTrigger.parse(
            """{"kind":"schedule","interval":"daily","minuteOfDay":420}""",
        )
        assertEquals(
            AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 420),
            trigger,
        )
    }

    @Test
    fun parsesAnEventWithFilters() {
        val trigger = AutomationTrigger.parse(
            """{"kind":"event","event":"status_changed",""" +
                """"filters":{"boardIds":["b1","b2"],"toStatusIds":["s1"]}}""",
        )
        assertEquals(
            AutomationTrigger.Event(
                event = "status_changed",
                filters = AutomationTriggerFilters(
                    boardIds = listOf("b1", "b2"),
                    toStatusIds = listOf("s1"),
                ),
            ),
            trigger,
        )
    }

    @Test
    fun theDeadDeviceAndEnabledFieldsAreIgnored() {
        // EXP-530 rows carried deviceId/enabled INSIDE the trigger; they are
        // columns on the automations row now and must simply be ignored here
        // (a legacy payload still parses).
        assertEquals(
            AutomationTrigger.Event(event = "created"),
            AutomationTrigger.parse(
                """{"kind":"event","deviceId":"dev-1","enabled":false,"event":"created"}""",
            ),
        )
    }

    @Test
    fun malformedTriggersReadAsNoAutomation() {
        assertNull(AutomationTrigger.parse(null))
        assertNull(AutomationTrigger.parse(""))
        assertNull(AutomationTrigger.parse("not json"))
        assertNull(AutomationTrigger.parse("[1,2]"))
        // Unknown kind (a FUTURE server shape).
        assertNull(AutomationTrigger.parse("""{"kind":"webhook"}"""))
        // Unknown event value.
        assertNull(AutomationTrigger.parse("""{"kind":"event","event":"issue_deleted"}"""))
        // Unknown interval / out-of-range minuteOfDay.
        assertNull(
            AutomationTrigger.parse("""{"kind":"schedule","interval":"hourly","minuteOfDay":0}"""),
        )
        assertNull(
            AutomationTrigger.parse("""{"kind":"schedule","interval":"daily","minuteOfDay":1440}"""),
        )
        // weekday required iff weekly; dayOfMonth required iff monthly.
        assertNull(
            AutomationTrigger.parse("""{"kind":"schedule","interval":"weekly","minuteOfDay":0}"""),
        )
        assertNull(
            AutomationTrigger.parse(
                """{"kind":"schedule","interval":"weekly","minuteOfDay":0,"weekday":8}""",
            ),
        )
        assertNull(
            AutomationTrigger.parse("""{"kind":"schedule","interval":"monthly","minuteOfDay":0}"""),
        )
        assertNull(
            AutomationTrigger.parse(
                """{"kind":"schedule","interval":"monthly","minuteOfDay":0,"dayOfMonth":29}""",
            ),
        )
    }

    @Test
    fun unknownFilterEntriesDegradeGracefully() {
        val trigger = AutomationTrigger.parse(
            """{"kind":"event","event":"created",""" +
                """"filters":{"priorities":["urgent","not-a-priority"],"boardIds":"nope"}}""",
        ) as AutomationTrigger.Event
        // Unknown priority values drop; a non-array list reads empty.
        assertEquals(listOf("urgent"), trigger.filters.priorities)
        assertEquals(emptyList<String>(), trigger.filters.boardIds)
    }

    // ── Summary strings (byte-locked, web/iOS parity) ────────────────────────

    @Test
    fun scheduleSummaries() {
        assertEquals(
            "Daily at 07:00",
            triggerSummary(AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 420)),
        )
        assertEquals(
            "Weekly on Monday at 09:00",
            triggerSummary(
                AutomationTrigger.Schedule(interval = "weekly", minuteOfDay = 540, weekday = 1),
            ),
        )
        assertEquals(
            "Monthly on day 5 at 09:00",
            triggerSummary(
                AutomationTrigger.Schedule(interval = "monthly", minuteOfDay = 540, dayOfMonth = 5),
            ),
        )
    }

    @Test
    fun eventSummaries() {
        fun event(name: String, filters: AutomationTriggerFilters = AutomationTriggerFilters()) =
            AutomationTrigger.Event(event = name, filters = filters)
        assertEquals("When an issue is created", triggerSummary(event("created")))
        assertEquals("When status changes", triggerSummary(event("status_changed")))
        assertEquals("When the assignee changes", triggerSummary(event("assignee_changed")))
        assertEquals("When a label is added", triggerSummary(event("label_added")))
        assertEquals("When priority changes", triggerSummary(event("priority_changed")))
        assertEquals("When a pull request is opened", triggerSummary(event("pr_opened")))
        assertEquals("When a pull request is merged", triggerSummary(event("pr_merged")))
        assertEquals(
            "When status changes · 3 filters",
            triggerSummary(
                event(
                    "status_changed",
                    AutomationTriggerFilters(
                        boardIds = listOf("b1", "b2"),
                        toStatusIds = listOf("s1"),
                    ),
                ),
            ),
        )
        // Singular for exactly one pick — web parity (`· 1 filter`).
        assertEquals(
            "When a label is added · 1 filter",
            triggerSummary(
                event("label_added", AutomationTriggerFilters(labelIds = listOf("l1"))),
            ),
        )
    }

    // ── Wire encoding ────────────────────────────────────────────────────────

    @Test
    fun wireJsonKeepsCanonicalKeyOrderAndOmitsEmptyFilters() {
        assertEquals(
            """{"kind":"schedule","interval":"weekly","minuteOfDay":540,"weekday":3}""",
            AutomationTrigger.Schedule(interval = "weekly", minuteOfDay = 540, weekday = 3)
                .toWireJsonString(),
        )
        assertEquals(
            """{"kind":"event","event":"created"}""",
            AutomationTrigger.Event(event = "created").toWireJsonString(),
        )
        assertEquals(
            """{"kind":"event","event":"label_added","filters":{"labelIds":["l1"]}}""",
            AutomationTrigger.Event(
                event = "label_added",
                filters = AutomationTriggerFilters(labelIds = listOf("l1")),
            ).toWireJsonString(),
        )
    }

    @Test
    fun wireJsonRoundTripsThroughTheTolerantParse() {
        val trigger = AutomationTrigger.Event(
            event = "priority_changed",
            filters = AutomationTriggerFilters(
                boardIds = listOf("b1"),
                priorities = listOf("high"),
            ),
        )
        assertEquals(trigger, AutomationTrigger.parse(trigger.toWireJsonString()))
    }

    // ── The automation note (byte-locked, web formatAutomationBlock) ─────────

    @Test
    fun descriptionBlockMatchesTheWebFormat() {
        assertEquals(
            "\n\nAutomation — after creating the action, call " +
                "exponential_automations_create with its id and exactly these fields: " +
                "`{\"deviceId\":\"d\",\"trigger\":" +
                "{\"kind\":\"schedule\",\"interval\":\"daily\",\"minuteOfDay\":420}}`. " +
                "An automated run fills no inputs, so declare none as required.",
            formatAutomationBlock(
                AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 420),
                deviceId = "d",
            ),
        )
    }

    @Test
    fun theNoteCarriesTheLaunchPinsOnlyWhenSet() {
        assertEquals(
            "\n\nAutomation — after creating the action, call " +
                "exponential_automations_create with its id and exactly these fields: " +
                "`{\"deviceId\":\"d\",\"trigger\":{\"kind\":\"event\",\"event\":\"created\"}," +
                "\"agent\":\"codex\",\"effort\":\"high\"}`. " +
                "An automated run fills no inputs, so declare none as required.",
            formatAutomationBlock(
                AutomationTrigger.Event(event = "created"),
                deviceId = "d",
                agent = "codex",
                // An empty model is "device default" — it must NOT ride.
                model = "",
                effort = "high",
            ),
        )
    }

    // ── Next-run schedule math (viewer-local, fixed zone) ────────────────────

    private val zone = TimeZone.getTimeZone("Europe/Vienna")

    private fun atLocal(
        year: Int,
        month: Int,
        day: Int,
        hour: Int,
        minute: Int,
    ): Long = Calendar.getInstance(zone).run {
        clear()
        set(year, month - 1, day, hour, minute, 0)
        timeInMillis
    }

    @Test
    fun dailyNextRunTodayOrTomorrow() {
        val schedule = AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 540)
        // Before 09:00 → today 09:00.
        assertEquals(
            atLocal(2026, 8, 18, 9, 0),
            nextScheduleRun(schedule, atLocal(2026, 8, 18, 8, 59), zone),
        )
        // Exactly 09:00 is NOT strictly after → tomorrow.
        assertEquals(
            atLocal(2026, 8, 19, 9, 0),
            nextScheduleRun(schedule, atLocal(2026, 8, 18, 9, 0), zone),
        )
    }

    @Test
    fun weeklyNextRunWrapsToNextWeek() {
        // 2026-08-18 is a Tuesday (ISO weekday 2).
        val schedule = AutomationTrigger.Schedule(
            interval = "weekly",
            minuteOfDay = 540,
            weekday = 2,
        )
        // Tuesday after 09:00 → NEXT Tuesday.
        assertEquals(
            atLocal(2026, 8, 25, 9, 0),
            nextScheduleRun(schedule, atLocal(2026, 8, 18, 10, 0), zone),
        )
        // Wednesday → the very next day.
        val wednesday = schedule.copy(weekday = 3)
        assertEquals(
            atLocal(2026, 8, 19, 9, 0),
            nextScheduleRun(wednesday, atLocal(2026, 8, 18, 10, 0), zone),
        )
    }

    @Test
    fun monthlyNextRunRollsToNextMonth() {
        val schedule = AutomationTrigger.Schedule(
            interval = "monthly",
            minuteOfDay = 540,
            dayOfMonth = 5,
        )
        assertEquals(
            atLocal(2026, 9, 5, 9, 0),
            nextScheduleRun(schedule, atLocal(2026, 8, 18, 10, 0), zone),
        )
        assertEquals(
            atLocal(2026, 8, 5, 9, 0),
            nextScheduleRun(schedule, atLocal(2026, 8, 1, 0, 0), zone),
        )
    }

    @Test
    fun malformedScheduleNextRunIsNull() {
        assertNull(
            nextScheduleRun(
                AutomationTrigger.Schedule(interval = "weekly", minuteOfDay = 540),
                atLocal(2026, 8, 18, 10, 0),
                zone,
            ),
        )
        assertNull(
            nextScheduleRun(
                AutomationTrigger.Schedule(interval = "hourly", minuteOfDay = 540),
                atLocal(2026, 8, 18, 10, 0),
                zone,
            ),
        )
    }
}
