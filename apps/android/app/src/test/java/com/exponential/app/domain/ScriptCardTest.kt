package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1279 (EXP-1259 D13): [ScriptCards.of] replays `fixtures/script-card.json`
 * and compares the WHOLE expected card (TS `script-card.test.ts`, desktop
 * `steer::script_card`, iOS `ScriptCardTests`).
 */
class ScriptCardTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("script-card.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.list(key: String): List<JsonObject> =
        this[key]?.takeUnless { it is JsonNull }?.jsonArray?.map { it.jsonObject }.orEmpty()

    @Test
    fun `every fixture case builds the whole expected card`() {
        val cases = fixture.list("cases")
        assertTrue(cases.size >= 6)
        for (case in cases) {
            val name = case.str("name")!!
            val input = case.getValue("input").jsonObject
            val actual = ScriptCards.of(
                ScriptCardInput(
                    running = input.getValue("running").jsonPrimitive.boolean,
                    count = input["count"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.int,
                    calls = input.list("calls").map {
                        ScriptCall(
                            tool = it.str("tool")!!,
                            subject = it.str("subject"),
                            ok = it.getValue("ok").jsonPrimitive.boolean,
                            ms = it["ms"]?.takeUnless { ms -> ms is JsonNull }?.jsonPrimitive?.long,
                        )
                    },
                    elapsedMs = input["elapsedMs"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long,
                ),
            )
            val expected = case.getValue("expected").jsonObject
            val want = ScriptCard(
                title = expected.str("title")!!,
                meta = expected.str("meta"),
                groups = expected.list("groups").map {
                    ScriptCardGroup(
                        tool = it.str("tool")!!,
                        exponential = it.getValue("exponential").jsonPrimitive.boolean,
                        label = it.str("label")!!,
                        subject = it.str("subject"),
                        more = it.str("more"),
                        count = it.str("count"),
                        failed = it.str("failed"),
                    )
                },
                overflow = expected.str("overflow"),
            )
            assertEquals(name, want, actual)
        }
    }

    @Test
    fun `the spec constants match the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("running"), ScriptCardSpec.RUNNING)
        assertEquals(copy.str("done"), ScriptCardSpec.DONE)
        assertEquals(copy.str("script"), ScriptCardSpec.SCRIPT)
        assertEquals(copy.str("call"), ScriptCardSpec.CALL)
        assertEquals(copy.str("calls"), ScriptCardSpec.CALLS)
        assertEquals(copy.str("failed"), ScriptCardSpec.FAILED)
        assertEquals(copy.str("moreCall"), ScriptCardSpec.MORE_CALL)
        assertEquals(copy.str("moreCalls"), ScriptCardSpec.MORE_CALLS)
        assertEquals(fixture.str("toolNameSuffix"), ScriptCardSpec.TOOL_NAME_SUFFIX)
        assertEquals(fixture.getValue("logCap").jsonPrimitive.int, ScriptCardSpec.LOG_CAP)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("card"), ScriptCardSpec.TEST_ID_CARD)
        assertEquals(ids.str("group"), ScriptCardSpec.TEST_ID_GROUP)
        assertEquals(ids.str("source"), ScriptCardSpec.TEST_ID_SOURCE)
        assertTrue(ScriptCardSpec.TOOL_KIND in DomainContract.toolKindValues)
    }

    @Test
    fun `script calls are recognised by kind or name`() {
        assertTrue(ScriptCards.isScriptCall("anything", "script"))
        assertTrue(ScriptCards.isScriptCall("mcp__codemode__exec", null))
        assertTrue(ScriptCards.isScriptCall("mcp.codemode.exec", "other"))
        assertFalse(ScriptCards.isScriptCall("Bash", "execute"))
    }

    @Test
    fun `durations replay the fixture in integer tenths half up`() {
        val durations = fixture.list("durations")
        assertTrue(durations.isNotEmpty())
        for (case in durations) {
            val ms = case.getValue("ms").jsonPrimitive.long
            assertEquals("$ms ms", case.str("expected"), ScriptCards.duration(ms))
        }
    }
}
