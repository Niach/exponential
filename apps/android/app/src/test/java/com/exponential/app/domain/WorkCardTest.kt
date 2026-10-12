package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1240 (EXP-1259 D12): [WorkCards.of] replays `fixtures/work-card.json`
 * and compares the WHOLE expected card (`fill` within 1e-9), like the TS
 * reference's own test, desktop `steer::work_card` and iOS `WorkCardTests`.
 */
class WorkCardTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("work-card.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.lng(key: String): Long? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long

    private fun JsonObject.integer(key: String): Int? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.int

    private fun JsonObject.list(key: String): List<JsonObject> =
        this[key]?.takeUnless { it is JsonNull }?.jsonArray?.map { it.jsonObject }.orEmpty()

    private fun input(json: JsonObject): WorkCardInput = when (json.str("kind")) {
        "workflow" -> WorkCardInput.Workflow(
            name = json.str("name")!!,
            status = json.str("status")!!,
            phases = json.list("phases").map { WorkCardPhaseInput(it.integer("index")!!, it.str("title")!!) },
            agents = json.list("agents").map {
                WorkCardWorkflowAgent(
                    index = it.integer("index")!!,
                    label = it.str("label"),
                    phaseIndex = it.integer("phaseIndex"),
                    model = it.str("model"),
                    state = it.str("state")!!,
                    durationMs = it.lng("durationMs"),
                )
            },
            elapsedMs = json.lng("elapsedMs"),
        )
        "tasks" -> WorkCardInput.Tasks(
            json.list("tasks").map { WorkCardTask(it.str("content")!!, it.str("status")!!) },
        )
        else -> WorkCardInput.Subagent(
            title = json.str("title"),
            state = json.str("state")!!,
            toolCalls = json.integer("toolCalls"),
            elapsedMs = json.lng("elapsedMs"),
        )
    }

    private fun row(json: JsonObject): WorkCardRow = when (json.str("type")) {
        "folded" -> WorkCardRow.Folded(json.str("label")!!, json.str("lanes")!!, json.str("duration"))
        "phase" -> WorkCardRow.Phase(
            title = json.str("title")!!,
            state = WorkState.fromWire(json.str("state")!!),
            detail = json.str("detail"),
            duration = json.str("duration"),
            lanes = json.list("lanes").map {
                WorkCardLane(it.str("label")!!, it.str("model"), it.str("state")!!, it.str("duration"))
            },
        )
        "task" -> WorkCardRow.Task(json.str("label")!!, WorkState.fromWire(json.str("state")!!))
        else -> WorkCardRow.Status(json.str("label")!!)
    }

    private fun expected(json: JsonObject): WorkCard = WorkCard(
        kind = WorkCardKind.entries.first { it.wire == json.str("kind") },
        icon = json.str("icon")!!,
        name = json.str("name")!!,
        kindWord = json.str("kindWord"),
        progress = json.str("progress"),
        elapsed = json.str("elapsed"),
        running = json.getValue("running").jsonPrimitive.boolean,
        segments = json.list("segments").map {
            WorkCardSegment(WorkState.fromWire(it.str("state")!!), it.getValue("fill").jsonPrimitive.double)
        },
        rows = json.list("rows").map(::row),
    )

    @Test
    fun `every fixture case builds the whole expected card`() {
        val cases = fixture.list("cases")
        assertTrue(cases.size >= 12)
        for (case in cases) {
            val name = case.str("name")!!
            val actual = WorkCards.of(input(case.getValue("input").jsonObject))
            val want = expected(case.getValue("expected").jsonObject)
            assertEquals(name, want.copy(segments = emptyList()), actual.copy(segments = emptyList()))
            assertEquals(name, want.segments.size, actual.segments.size)
            want.segments.zip(actual.segments).forEach { (w, a) ->
                assertEquals(name, w.state, a.state)
                assertEquals(name, w.fill, a.fill, 1e-9)
            }
        }
    }

    @Test
    fun `the spec constants match the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        fun copy(key: String) = copy.getValue(key).jsonPrimitive.content
        assertEquals(copy("workflow"), WorkCardSpec.WORKFLOW)
        assertEquals(copy("tasks"), WorkCardSpec.TASKS)
        assertEquals(copy("subagent"), WorkCardSpec.SUBAGENT)
        assertEquals(copy("phase"), WorkCardSpec.PHASE)
        assertEquals(copy("loosePhase"), WorkCardSpec.LOOSE_PHASE)
        assertEquals(copy("agent"), WorkCardSpec.AGENT)
        assertEquals(copy("lane"), WorkCardSpec.LANE)
        assertEquals(copy("lanes"), WorkCardSpec.LANES)
        assertEquals(copy("call"), WorkCardSpec.CALL)
        assertEquals(copy("calls"), WorkCardSpec.CALLS)
        assertEquals(copy("running"), WorkCardSpec.RUNNING)
        assertEquals(copy("done"), WorkCardSpec.DONE)
        assertEquals(copy("failed"), WorkCardSpec.FAILED)
        val icons = fixture.getValue("icons").jsonObject
        assertEquals(icons.str("workflow"), WorkCardSpec.ICON_WORKFLOW)
        assertEquals(icons.str("tasks"), WorkCardSpec.ICON_TASKS)
        assertEquals(icons.str("subagent"), WorkCardSpec.ICON_SUBAGENT)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("card"), WorkCardSpec.TEST_ID_CARD)
        assertEquals(ids.str("lane"), WorkCardSpec.TEST_ID_LANE)
        assertEquals(ids.str("folded"), WorkCardSpec.TEST_ID_FOLDED)
        val geometry: Map<String, JsonElement> = fixture.getValue("geometry").jsonObject
        fun geo(key: String) = geometry.getValue(key).jsonPrimitive.int
        assertEquals(geo("header"), WorkCardSpec.HEADER)
        assertEquals(geo("row"), WorkCardSpec.ROW)
        assertEquals(geo("lane"), WorkCardSpec.LANE_ROW)
        assertEquals(geo("laneIndent"), WorkCardSpec.LANE_INDENT)
        assertEquals(geo("segmentHeight"), WorkCardSpec.SEGMENT_HEIGHT)
        assertEquals(geo("segmentGap"), WorkCardSpec.SEGMENT_GAP)
    }

    @Test
    fun `durations and model labels`() {
        assertEquals("42s", WorkCards.durationShort(42_000))
        assertEquals("48m", WorkCards.durationShort(48 * 60_000L))
        assertEquals("1h 12m", WorkCards.durationShort(72 * 60_000L))
        assertEquals("2h", WorkCards.durationShort(120 * 60_000L))
        assertEquals("8m 29s", WorkCards.durationLong(509_000))
        assertEquals("opus", WorkCards.modelLabel("claude-opus-5-5"))
        assertEquals("gpt-5", WorkCards.modelLabel(" gpt-5 "))
        assertEquals(null, WorkCards.modelLabel("  "))
    }
}
