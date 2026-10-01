package com.exponential.app.domain

import java.util.Calendar
import java.util.TimeZone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-583 / SLOP-2: the tolerant when-part parse (anything malformed reads as
// "no trigger", never a throw), the stored trigger read off `actions.triggers`
// (`parseActionTriggers`), the row glyph rule (`triggerBadges`), the shared
// summary strings (byte-matching web `triggerSummary`), the trigger block a
// suggestion appends (byte-matching web `formatTriggerBlock`) and the next-run
// schedule math in a fixed viewer timezone.
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
    fun theRunnerFieldsBesideTheWhenPartAreIgnored() {
        // A stored trigger carries its runner (id, deviceId, enabled, pins)
        // BESIDE the when-part keys; the when-part parse simply ignores them.
        assertEquals(
            AutomationTrigger.Event(event = "created"),
            AutomationTrigger.parse(
                """{"kind":"event","deviceId":"dev-1","enabled":false,"event":"created"}""",
            ),
        )
    }

    @Test
    fun malformedTriggersReadAsNoTrigger() {
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

    // ── Event source (SLOP-2) ────────────────────────────────────────────────

    @Test
    fun anAbsentOrExponentialSourceReadsAndAForeignOneDoesNot() {
        val created = AutomationTrigger.Event(event = "created")
        assertEquals(created, AutomationTrigger.parse("""{"kind":"event","event":"created"}"""))
        assertEquals(
            created,
            AutomationTrigger.parse("""{"kind":"event","source":"exponential","event":"created"}"""),
        )
        // A FUTURE source reads as "never fires" on this build.
        assertNull(
            AutomationTrigger.parse("""{"kind":"event","source":"github","event":"created"}"""),
        )
        assertNull(AutomationTrigger.parse("""{"kind":"event","source":null,"event":"created"}"""))
    }

    // ── Stored triggers (actions.triggers) ───────────────────────────────────

    @Test
    fun parsesAStoredTriggerWithItsRunner() {
        assertEquals(
            listOf(
                ActionTrigger(
                    id = "t-1",
                    enabled = false,
                    deviceId = "d-1",
                    agent = "claude",
                    account = "work",
                    model = "opus",
                    effort = "high",
                    whenPart = AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 540),
                ),
                ActionTrigger(
                    id = "t-2",
                    deviceId = "d-2",
                    whenPart = AutomationTrigger.Event(
                        event = "label_added",
                        filters = AutomationTriggerFilters(labelIds = listOf("l1")),
                    ),
                ),
            ),
            parseActionTriggers(
                """[{"id":"t-1","enabled":false,"deviceId":"d-1","agent":"claude",""" +
                    """"account":"work","model":"opus","effort":"high",""" +
                    """"kind":"schedule","interval":"daily","minuteOfDay":540},""" +
                    """{"id":"t-2","deviceId":"d-2","kind":"event","source":"exponential",""" +
                    """"event":"label_added","filters":{"labelIds":["l1"]}}]""",
            ),
        )
    }

    @Test
    fun onlyAnExplicitFalsePauses() {
        fun enabled(flag: String) = parseActionTriggers(
            """[{"id":"t","deviceId":"d",$flag"kind":"schedule","interval":"daily","minuteOfDay":0}]""",
        ).single().enabled
        assertEquals(true, enabled(""))
        assertEquals(true, enabled(""""enabled":true,"""))
        assertEquals(true, enabled(""""enabled":null,"""))
        assertEquals(true, enabled(""""enabled":"false","""))
        assertEquals(false, enabled(""""enabled":false,"""))
    }

    @Test
    fun unreadableStoredTriggersAreSkippedInOrder() {
        val triggers = parseActionTriggers(
            "[" +
                // No id.
                """{"deviceId":"d","kind":"schedule","interval":"daily","minuteOfDay":0},""" +
                // A non-string id.
                """{"id":7,"deviceId":"d","kind":"schedule","interval":"daily","minuteOfDay":0},""" +
                """{"id":"keep-1","deviceId":"d","kind":"schedule","interval":"daily","minuteOfDay":0},""" +
                // No device.
                """{"id":"x","kind":"event","event":"created"},""" +
                // An unreadable when-part (future kind, foreign source).
                """{"id":"y","deviceId":"d","kind":"webhook"},""" +
                """{"id":"z","deviceId":"d","kind":"event","source":"github","event":"created"},""" +
                // Not an object at all.
                """"nope",null,""" +
                """{"id":"keep-2","deviceId":"d","kind":"event","event":"created","agent":""}""" +
                "]",
        )
        assertEquals(listOf("keep-1", "keep-2"), triggers.map { it.id })
        // An empty pin is an unset one.
        assertNull(triggers[1].agent)
    }

    @Test
    fun aNonArrayTriggersColumnReadsAsNoTriggers() {
        assertEquals(emptyList<ActionTrigger>(), parseActionTriggers(null as String?))
        assertEquals(emptyList<ActionTrigger>(), parseActionTriggers(""))
        assertEquals(emptyList<ActionTrigger>(), parseActionTriggers("not json"))
        assertEquals(emptyList<ActionTrigger>(), parseActionTriggers("""{"id":"t"}"""))
        assertEquals(emptyList<ActionTrigger>(), parseActionTriggers("[]"))
    }

    // ── Row glyphs ───────────────────────────────────────────────────────────

    private fun stored(id: String, enabled: Boolean, whenPart: AutomationTrigger) =
        ActionTrigger(id = id, enabled = enabled, deviceId = "d", whenPart = whenPart)

    private val daily = AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 540)
    private val onCreated = AutomationTrigger.Event(event = "created")

    @Test
    fun badgesDrawAKindOnceAndMuteItWhileNoneOfItIsEnabled() {
        assertEquals(TriggerBadges(), triggerBadges(emptyList()))
        assertEquals(
            TriggerBadges(schedule = TriggerBadge(active = true)),
            triggerBadges(listOf(stored("a", false, daily), stored("b", true, daily))),
        )
        assertEquals(
            TriggerBadges(
                schedule = TriggerBadge(active = false),
                event = TriggerBadge(active = true),
            ),
            triggerBadges(listOf(stored("a", false, daily), stored("b", true, onCreated))),
        )
        assertEquals(
            TriggerBadges(event = TriggerBadge(active = false)),
            triggerBadges(listOf(stored("a", false, onCreated))),
        )
    }

    @Test
    fun aSuggestionSeedWearsItsOwnKindActive() {
        assertEquals(TriggerBadges(), triggerBadges(null as AutomationTrigger?))
        assertEquals(TriggerBadges(schedule = TriggerBadge(active = true)), triggerBadges(daily))
        assertEquals(TriggerBadges(event = TriggerBadge(active = true)), triggerBadges(onCreated))
    }

    @Test
    fun aScheduleCaptionCarriesTheDeviceTimeCaveat() {
        assertEquals("Daily at 09:00 (device time)", triggerCaption(daily))
        assertEquals("When an issue is created", triggerCaption(onCreated))
    }

    // ── Run titles (×4, web actionRunTitle) ──────────────────────────────────

    @Test
    fun aRunInItsActionsRunsListIsTitledByWhatStartedIt() {
        assertEquals("Scheduled run", actionRunTitle("schedule"))
        assertEquals("Event run", actionRunTitle("event"))
        // Another run started it — today's reasons and any added later.
        assertEquals("Agent run", actionRunTitle("agent"))
        assertEquals("Agent run", actionRunTitle("workflow"))
        assertEquals("Manual run", actionRunTitle(null))
        assertEquals("Manual run", actionRunTitle(""))
    }

    // ── The trigger block (byte-locked ×4, web formatTriggerBlock) ───────────

    @Test
    fun theTriggerBlockMatchesTheSharedExample() {
        assertEquals(
            "\n\nTrigger — after creating the action, call exponential_actions_update " +
                "with its id and `triggers` set to exactly this array: " +
                "`[{\"kind\":\"schedule\",\"interval\":\"daily\",\"minuteOfDay\":540," +
                "\"deviceId\":\"d-1\"}]`. " +
                "A triggered run fills no inputs, so declare none as required.",
            formatTriggerBlock(
                AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 540),
                deviceId = "d-1",
            ),
        )
    }

    @Test
    fun theTriggerBlockCarriesTheLaunchPinsOnlyWhenSet() {
        assertEquals(
            "\n\nTrigger — after creating the action, call exponential_actions_update " +
                "with its id and `triggers` set to exactly this array: " +
                "`[{\"kind\":\"event\",\"event\":\"created\",\"deviceId\":\"d\"," +
                "\"agent\":\"codex\",\"effort\":\"high\"}]`. " +
                "A triggered run fills no inputs, so declare none as required.",
            formatTriggerBlock(
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
