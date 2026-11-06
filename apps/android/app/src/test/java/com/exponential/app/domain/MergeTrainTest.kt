package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1710: `packages/domain-contract/fixtures/merge-train.json`'s UI cases
 * (`statusPill`, `menu`, `stuckRow`) plus its client copy, limits, testIds and
 * `mergeNowPrompt`, replayed against [MergeTrain] (TS `merge-train.test.ts`,
 * desktop + iOS mirrors).
 */
class MergeTrainTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("merge-train.json")).jsonObject }

    private fun JsonObject.str(key: String): String? = this[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.contentOrNull
    private fun JsonObject.intOrNull(key: String): Int? = this[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.intOrNull
    private fun JsonElement.strings(): Map<String, String> = jsonObject.mapValues { it.value.jsonPrimitive.content }

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    @Test
    fun `copy, limits and testIds are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(
            copy.getValue("pill").strings(),
            mapOf(
                "draft" to MergeTrain.Copy.Pill.DRAFT,
                "checksRunning" to MergeTrain.Copy.Pill.CHECKS_RUNNING,
                "sessionWorking" to MergeTrain.Copy.Pill.SESSION_WORKING,
                "queued" to MergeTrain.Copy.Pill.QUEUED,
                "queuedAt" to MergeTrain.Copy.Pill.QUEUED_AT,
                "testing" to MergeTrain.Copy.Pill.TESTING,
                "testingWith" to MergeTrain.Copy.Pill.TESTING_WITH,
                "merged" to MergeTrain.Copy.Pill.MERGED,
                "conflict" to MergeTrain.Copy.Pill.CONFLICT,
                "failing" to MergeTrain.Copy.Pill.FAILING,
                "onHold" to MergeTrain.Copy.Pill.ON_HOLD,
                "stuck" to MergeTrain.Copy.Pill.STUCK,
                "needsApproval" to MergeTrain.Copy.Pill.NEEDS_APPROVAL,
            ),
        )
        assertEquals(
            copy.getValue("menu").strings(),
            mapOf(
                "mergeNow" to MergeTrain.Copy.Menu.MERGE_NOW,
                "hold" to MergeTrain.Copy.Menu.HOLD,
                "resume" to MergeTrain.Copy.Menu.RESUME,
            ),
        )
        assertEquals(
            copy.getValue("needsYou").strings(),
            mapOf(
                "title" to MergeTrain.Copy.NeedsYou.TITLE,
                "bounces" to MergeTrain.Copy.NeedsYou.BOUNCES,
                "timeout" to MergeTrain.Copy.NeedsYou.TIMEOUT,
                "offline" to MergeTrain.Copy.NeedsYou.OFFLINE,
                "retry" to MergeTrain.Copy.NeedsYou.RETRY,
                "mergeNow" to MergeTrain.Copy.NeedsYou.MERGE_NOW,
                "close" to MergeTrain.Copy.NeedsYou.CLOSE,
            ),
        )
        assertEquals(
            copy.getValue("settings").strings(),
            mapOf(
                "label" to MergeTrain.Copy.Settings.LABEL,
                "auto" to MergeTrain.Copy.Settings.AUTO,
                "approval" to MergeTrain.Copy.Settings.APPROVAL,
            ),
        )
        assertEquals(copy.getValue("events").strings(), MergeTrain.Copy.EVENTS)
        val limits = fixture.getValue("limits").jsonObject
        assertEquals(limits.getValue("bounces").jsonPrimitive.int, MergeTrain.Limits.BOUNCES)
        assertEquals(limits.getValue("offlineHours").jsonPrimitive.int, MergeTrain.Limits.OFFLINE_HOURS)
        assertEquals(
            fixture.getValue("testIds").strings(),
            mapOf(
                "statusPill" to MergeTrain.TestIds.STATUS_PILL,
                "mergeNow" to MergeTrain.TestIds.MERGE_NOW,
                "hold" to MergeTrain.TestIds.HOLD,
                "resume" to MergeTrain.TestIds.RESUME,
                "stuckRow" to MergeTrain.TestIds.STUCK_ROW,
                "settingsRow" to MergeTrain.TestIds.SETTINGS_ROW,
            ),
        )
    }

    @Test
    fun `mergeNowPrompt is prompts merge-now-pr`() {
        val spec = fixture.getValue("mergeNowPrompt").jsonObject
        val number = Prompts.MergeNowPr.prompt(12)
        assertEquals(spec.str("id"), Prompts.MergeNowPr.spec.id)
        assertEquals(spec.str("title")!!.replace("{number}", "12"), number.title)
        assertEquals(spec.str("body"), number.body)
        assertEquals(spec.str("titleStack"), Prompts.MergeNowPr.prompt(12, stack = true).title)
        assertEquals(spec.str("focus"), number.focus)
    }

    @Test
    fun `ordinal`() {
        val expected = mapOf(1 to "1st", 2 to "2nd", 3 to "3rd", 4 to "4th", 11 to "11th", 12 to "12th", 13 to "13th", 21 to "21st", 111 to "111th")
        for ((n, word) in expected) assertEquals(word, MergeTrain.ordinal(n))
    }

    @Test
    fun `every statusPill case matches the fixture`() {
        for (case in cases("statusPill")) {
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            val view = MergeTrain.statusPill(
                MergeTrain.StatusPillInput(
                    state = input.str("state")!!,
                    wait = input.str("wait"),
                    position = input.intOrNull("position"),
                    batchSize = input.intOrNull("batchSize"),
                    holdReason = input.str("holdReason"),
                    stack = input.getValue("stack").jsonPrimitive.boolean,
                    checks = input.getValue("checks").jsonPrimitive.int,
                ),
            )
            assertEquals(
                case.str("name"),
                MergeTrain.StatusPillView(
                    label = expected.str("label")!!,
                    glyph = expected.str("glyph")!!,
                    tone = expected.str("tone")!!,
                    primary = expected.getValue("primary").jsonPrimitive.boolean,
                    tap = expected.str("tap"),
                    confirm = expected.str("confirm"),
                ),
                view,
            )
        }
    }

    @Test
    fun `the styleguide specimen is every statusPill input in fixture order`() {
        val inputs = cases("statusPill").map { case ->
            val input = case.getValue("input").jsonObject
            MergeTrain.StatusPillInput(
                state = input.str("state")!!,
                wait = input.str("wait"),
                position = input.intOrNull("position"),
                batchSize = input.intOrNull("batchSize"),
                holdReason = input.str("holdReason"),
                stack = input.getValue("stack").jsonPrimitive.boolean,
                checks = input.getValue("checks").jsonPrimitive.int,
            )
        }
        assertEquals(inputs, MergeTrain.SPECIMEN)
    }

    @Test
    fun `every menu case matches the fixture`() {
        for (case in cases("menu")) {
            val input = case.getValue("input").jsonObject
            val entries = MergeTrain.mergeMenuEntries(
                state = input.str("state"),
                member = input.getValue("member").jsonPrimitive.boolean,
                owner = input.getValue("owner").jsonPrimitive.boolean,
                checks = input.getValue("checks").jsonPrimitive.int,
                mergeState = input.str("mergeState"),
            )
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                entries.map { it.wire },
            )
        }
    }

    @Test
    fun `every stuckRow case matches the fixture`() {
        for (case in cases("stuckRow")) {
            val input = case.getValue("input").jsonObject
            val row = MergeTrain.stuckRow(
                number = input.getValue("number").jsonPrimitive.long,
                state = input.str("state")!!,
                holdReason = input.str("holdReason"),
                deviceOfflineSince = input.str("deviceOfflineSince"),
                nowMs = WireTimestamps.parseEpochMs(input.str("now")!!)!!,
                owner = input.getValue("owner").jsonPrimitive.boolean,
            )
            val expected = case["expected"]
            if (expected == null || expected is JsonNull) {
                assertNull(case.str("name"), row)
                continue
            }
            val o = expected.jsonObject
            assertEquals(
                case.str("name"),
                MergeTrain.StuckRow(
                    title = o.str("title")!!,
                    meta = o.str("meta")!!,
                    actions = o.getValue("actions").jsonArray.map { a ->
                        MergeTrain.StuckAction.entries.first { it.wire == a.jsonPrimitive.content }
                    },
                ),
                row,
            )
        }
    }

    @Test
    fun `the pr_merge event copy reads by outcome, unknown = fallback`() {
        assertEquals("Merged PR {pr}", MergeTrain.eventTemplate("merged"))
        assertEquals("Auto-merge stopped on PR {pr}", MergeTrain.eventTemplate("held"))
        assertNull(MergeTrain.eventTemplate(null))
        assertNull(MergeTrain.eventTemplate("later"))
    }

    @Test
    fun `batchSize counts the PRs sharing a train`() {
        assertEquals(3, MergeTrain.batchSize("t1", listOf("t1", null, "t1", "t2", "t1")))
        assertNull(MergeTrain.batchSize(null, listOf("t1")))
    }

    @Test
    fun `the PR's own state wins over merge_state`() {
        assertEquals("merged", MergeTrain.prMergeState("merged", "queued"))
        assertEquals("draft", MergeTrain.prMergeState("draft", "checks"))
        assertEquals("testing", MergeTrain.prMergeState("open", "testing"))
    }

    @Test
    fun `a viewer's Needs approval is a plain status`() {
        val approval = MergeTrain.statusPill(MergeTrain.StatusPillInput("checks", "approval", stack = false, checks = 2))
        assertEquals(approval, MergeTrain.forViewer(approval, member = true, checks = 2))
        val viewer = MergeTrain.forViewer(approval, member = false, checks = 2)
        assertEquals(MergeTrain.StatusPillView("Needs approval", "pr-merged", "muted", primary = false, tap = "checks"), viewer)
        assertNull(MergeTrain.forViewer(approval, member = false, checks = 0).tap)
        val queued = MergeTrain.statusPill(MergeTrain.StatusPillInput("queued", null, position = 1, stack = false, checks = 0))
        assertEquals(queued, MergeTrain.forViewer(queued, member = false, checks = 0))
    }
}
