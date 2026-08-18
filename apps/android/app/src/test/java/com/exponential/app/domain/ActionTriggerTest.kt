package com.exponential.app.domain

import java.util.Calendar
import java.util.TimeZone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-530: the tolerant trigger parse (anything malformed reads as "no
// automation", never a throw), the shared summary strings (byte-matching
// web `triggerSummary` / iOS `ActionTriggerDisplay.summary`) and the
// next-run schedule math in a fixed viewer timezone.
class ActionTriggerTest {

    // ── Tolerant parse ───────────────────────────────────────────────────────

    @Test
    fun parsesASchedule() {
        val trigger = ActionTrigger.parse(
            """{"kind":"schedule","deviceId":"dev-1","enabled":true,"interval":"daily","minuteOfDay":420}""",
        )
        assertEquals(
            ActionTrigger.Schedule(deviceId = "dev-1", enabled = true, interval = "daily", minuteOfDay = 420),
            trigger,
        )
    }

    @Test
    fun parsesAnEventWithFilters() {
        val trigger = ActionTrigger.parse(
            """{"kind":"event","deviceId":"dev-1","event":"status_changed",""" +
                """"filters":{"boardIds":["b1","b2"],"toStatusIds":["s1"]}}""",
        )
        assertEquals(
            ActionTrigger.Event(
                deviceId = "dev-1",
                enabled = true,
                event = "status_changed",
                filters = ActionTriggerFilters(
                    boardIds = listOf("b1", "b2"),
                    toStatusIds = listOf("s1"),
                ),
            ),
            trigger,
        )
    }

    @Test
    fun missingEnabledDefaultsTrueAndOnlyExplicitFalseDisables() {
        assertTrue(
            ActionTrigger.parse(
                """{"kind":"event","deviceId":"d","event":"created"}""",
            )!!.enabled,
        )
        // Garbage `enabled` (a string) still reads ON — only boolean false
        // disables (web `value.enabled !== false`).
        assertTrue(
            ActionTrigger.parse(
                """{"kind":"event","deviceId":"d","event":"created","enabled":"false"}""",
            )!!.enabled,
        )
        assertEquals(
            false,
            ActionTrigger.parse(
                """{"kind":"event","deviceId":"d","event":"created","enabled":false}""",
            )!!.enabled,
        )
    }

    @Test
    fun malformedTriggersReadAsNoAutomation() {
        assertNull(ActionTrigger.parse(null))
        assertNull(ActionTrigger.parse(""))
        assertNull(ActionTrigger.parse("not json"))
        assertNull(ActionTrigger.parse("[1,2]"))
        // Unknown kind (a FUTURE server shape).
        assertNull(ActionTrigger.parse("""{"kind":"webhook","deviceId":"d"}"""))
        // Unknown event value.
        assertNull(
            ActionTrigger.parse("""{"kind":"event","deviceId":"d","event":"issue_deleted"}"""),
        )
        // Missing/empty deviceId.
        assertNull(ActionTrigger.parse("""{"kind":"event","event":"created"}"""))
        assertNull(
            ActionTrigger.parse("""{"kind":"event","deviceId":"","event":"created"}"""),
        )
        // Unknown interval / out-of-range minuteOfDay.
        assertNull(
            ActionTrigger.parse(
                """{"kind":"schedule","deviceId":"d","interval":"hourly","minuteOfDay":0}""",
            ),
        )
        assertNull(
            ActionTrigger.parse(
                """{"kind":"schedule","deviceId":"d","interval":"daily","minuteOfDay":1440}""",
            ),
        )
        // weekday required iff weekly; dayOfMonth required iff monthly.
        assertNull(
            ActionTrigger.parse(
                """{"kind":"schedule","deviceId":"d","interval":"weekly","minuteOfDay":0}""",
            ),
        )
        assertNull(
            ActionTrigger.parse(
                """{"kind":"schedule","deviceId":"d","interval":"weekly","minuteOfDay":0,"weekday":8}""",
            ),
        )
        assertNull(
            ActionTrigger.parse(
                """{"kind":"schedule","deviceId":"d","interval":"monthly","minuteOfDay":0}""",
            ),
        )
        assertNull(
            ActionTrigger.parse(
                """{"kind":"schedule","deviceId":"d","interval":"monthly","minuteOfDay":0,"dayOfMonth":29}""",
            ),
        )
    }

    @Test
    fun unknownFilterEntriesDegradeGracefully() {
        val trigger = ActionTrigger.parse(
            """{"kind":"event","deviceId":"d","event":"created",""" +
                """"filters":{"priorities":["urgent","not-a-priority"],"boardIds":"nope"}}""",
        ) as ActionTrigger.Event
        // Unknown priority values drop; a non-array list reads empty.
        assertEquals(listOf("urgent"), trigger.filters.priorities)
        assertEquals(emptyList<String>(), trigger.filters.boardIds)
    }

    // ── Summary strings (byte-locked, web/iOS parity) ────────────────────────

    @Test
    fun scheduleSummaries() {
        assertEquals(
            "Daily at 07:00",
            triggerSummary(
                ActionTrigger.Schedule(deviceId = "d", interval = "daily", minuteOfDay = 420),
            ),
        )
        assertEquals(
            "Weekly on Monday at 09:00",
            triggerSummary(
                ActionTrigger.Schedule(
                    deviceId = "d",
                    interval = "weekly",
                    minuteOfDay = 540,
                    weekday = 1,
                ),
            ),
        )
        assertEquals(
            "Monthly on day 5 at 09:00",
            triggerSummary(
                ActionTrigger.Schedule(
                    deviceId = "d",
                    interval = "monthly",
                    minuteOfDay = 540,
                    dayOfMonth = 5,
                ),
            ),
        )
    }

    @Test
    fun eventSummaries() {
        fun event(name: String, filters: ActionTriggerFilters = ActionTriggerFilters()) =
            ActionTrigger.Event(deviceId = "d", event = name, filters = filters)
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
                    ActionTriggerFilters(boardIds = listOf("b1", "b2"), toStatusIds = listOf("s1")),
                ),
            ),
        )
        // Singular for exactly one pick — web parity (`· 1 filter`).
        assertEquals(
            "When a label is added · 1 filter",
            triggerSummary(
                event("label_added", ActionTriggerFilters(labelIds = listOf("l1"))),
            ),
        )
    }

    // ── Wire encoding ────────────────────────────────────────────────────────

    @Test
    fun wireJsonKeepsCanonicalKeyOrderAndOmitsEmptyFilters() {
        assertEquals(
            """{"kind":"schedule","deviceId":"d","enabled":true,"interval":"weekly","minuteOfDay":540,"weekday":3}""",
            ActionTrigger.Schedule(
                deviceId = "d",
                interval = "weekly",
                minuteOfDay = 540,
                weekday = 3,
            ).toWireJsonString(),
        )
        assertEquals(
            """{"kind":"event","deviceId":"d","enabled":false,"event":"created"}""",
            ActionTrigger.Event(deviceId = "d", enabled = false, event = "created")
                .toWireJsonString(),
        )
        assertEquals(
            """{"kind":"event","deviceId":"d","enabled":true,"event":"label_added","filters":{"labelIds":["l1"]}}""",
            ActionTrigger.Event(
                deviceId = "d",
                event = "label_added",
                filters = ActionTriggerFilters(labelIds = listOf("l1")),
            ).toWireJsonString(),
        )
    }

    @Test
    fun wireJsonRoundTripsThroughTheTolerantParse() {
        val trigger = ActionTrigger.Event(
            deviceId = "dev-1",
            enabled = false,
            event = "priority_changed",
            filters = ActionTriggerFilters(boardIds = listOf("b1"), priorities = listOf("high")),
        )
        assertEquals(trigger, ActionTrigger.parse(trigger.toWireJsonString()))
    }

    @Test
    fun descriptionBlockMatchesTheWebFormat() {
        val trigger = ActionTrigger.Schedule(deviceId = "d", interval = "daily", minuteOfDay = 420)
        assertEquals(
            "\n\nAutomation — set exactly this trigger via the `trigger` field on " +
                "exponential_actions_create: " +
                "`{\"kind\":\"schedule\",\"deviceId\":\"d\",\"enabled\":true,\"interval\":\"daily\",\"minuteOfDay\":420}`",
            triggerDescriptionBlock(trigger),
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
        val schedule = ActionTrigger.Schedule(deviceId = "d", interval = "daily", minuteOfDay = 540)
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
        val schedule = ActionTrigger.Schedule(
            deviceId = "d",
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
        val schedule = ActionTrigger.Schedule(
            deviceId = "d",
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
                ActionTrigger.Schedule(deviceId = "d", interval = "weekly", minuteOfDay = 540),
                atLocal(2026, 8, 18, 10, 0),
                zone,
            ),
        )
        assertNull(
            nextScheduleRun(
                ActionTrigger.Schedule(deviceId = "d", interval = "hourly", minuteOfDay = 540),
                atLocal(2026, 8, 18, 10, 0),
                zone,
            ),
        )
    }
}
