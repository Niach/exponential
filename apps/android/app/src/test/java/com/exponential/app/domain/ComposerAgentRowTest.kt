package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// S-3: the composer agent row, replayed off `composer-home.json` `agentRow`
// (TS `composer-agent-row.test.ts`, same case names).
class ComposerAgentRowTest {
    private val row: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("composer-home.json")).jsonObject.getValue("agentRow").jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `geometry, copy and ids are the fixture's`() {
        assertEquals(row.getValue("size").jsonPrimitive.int, ComposerAgentRow.SIZE)
        assertEquals(row.getValue("ringWidth").jsonPrimitive.int, ComposerAgentRow.RING_WIDTH)
        assertEquals(row.getValue("ringGap").jsonPrimitive.int, ComposerAgentRow.RING_GAP)
        assertEquals(row.getValue("gap").jsonPrimitive.int, ComposerAgentRow.GAP)
        assertEquals(row.getValue("labelToRow").jsonPrimitive.int, ComposerAgentRow.LABEL_TO_ROW)
        assertEquals(row.getValue("rowToCard").jsonPrimitive.int, ComposerAgentRow.ROW_TO_CARD)
        assertEquals(row.str("temporaryIcon"), ComposerAgentRow.TEMPORARY_ICON)
        assertEquals(row.str("temporaryId"), ComposerAgentRow.TEMPORARY_PICK)
        val copy = row.getValue("copy").jsonObject
        assertEquals(copy.str("to"), ComposerAgentRow.COPY_TO)
        assertEquals(copy.str("temporary"), ComposerAgentRow.COPY_TEMPORARY)
        val ids = row.getValue("testIds").jsonObject
        assertEquals(ids.str("row"), ComposerAgentRow.TEST_ID_ROW)
        assertEquals(ids.str("label"), ComposerAgentRow.TEST_ID_LABEL)
        assertEquals(ids.str("option")!!.replace("{id}", "x"), ComposerAgentRow.optionTestId("x"))
    }

    @Test
    fun `agentRow cases match the fixture`() {
        val cases = row.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agents = case.getValue("agents").jsonArray.map {
                val a = it.jsonObject
                ComposerAgentRow.Agent(a.str("id")!!, a.str("role"), a.str("ownerId"), a.str("name")!!, a.getValue("sortOrder").jsonPrimitive.int)
            }
            val state = ComposerAgentRow.composerAgentRow(agents, case.str("me"), case.str("picked")) { it }
            val expected = case.getValue("expected").jsonObject
            val name = case.str("name")
            assertEquals(name, expected.getValue("options").jsonArray.map { it.jsonPrimitive.content }, state.options)
            assertEquals(name, expected.str("selected"), state.selected)
            assertEquals(name, expected.str("label"), state.label)
        }
    }

    // `agentRow.seedCases` (TS `seedPick`): a seeded launch mounts on
    // Temporary session, a plain or text-only mount on the default.
    @Test
    fun `seedCases match the fixture`() {
        val cases = row.getValue("seedCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val seed = case.getValue("seed").jsonObject
            val parsed = AgentComposerSeed(
                text = seed.str("text"),
                issueIds = seed["issueIds"]?.jsonArray?.map { it.jsonPrimitive.content } ?: emptyList(),
                promptId = seed.str("promptId"),
                deviceId = seed.str("deviceId"),
            )
            assertEquals(case.str("name"), case.str("expected"), ComposerAgentRow.seedPick(parsed))
        }
    }

    // `agentRow.subjectCases` (TS `pickForSubject`): `subject` = the
    // composer's subject kind (`issues` / `prompt` = a launch, null = none).
    @Test
    fun `subjectCases match the fixture`() {
        val cases = row.getValue("subjectCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val isLaunch = case.str("subject") in setOf("issues", "prompt")
            assertEquals(case.str("name"), case.str("expected"), ComposerAgentRow.pickForSubject(isLaunch, case.str("current")))
        }
    }

    // `agentRow.pickCases` (TS `subjectAfterPick`, EXP-1450): a tap keeps the
    // issue chips; only a prompt goes when an agent is picked.
    @Test
    fun `pickCases match the fixture`() {
        val cases = row.getValue("pickCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val subject = fixtureSubject(case["subject"])
            val after = ComposerAgentRow.subjectAfterPick(subject, case.str("pick")!!) { it is FixtureSubject.Prompt }
            assertEquals(case.str("name"), fixtureSubject(case["expected"]), after)
        }
    }
}

/** The fixture's `{kind, ids | id}` subject (null = none). */
internal sealed interface FixtureSubject {
    data class Issues(val ids: List<String>) : FixtureSubject
    data class Prompt(val id: String) : FixtureSubject
}

internal fun fixtureSubject(element: kotlinx.serialization.json.JsonElement?): FixtureSubject? {
    if (element == null || element is JsonNull) return null
    val obj = element.jsonObject
    return when (val kind = obj.getValue("kind").jsonPrimitive.content) {
        "issues" -> FixtureSubject.Issues(obj.getValue("ids").jsonArray.map { it.jsonPrimitive.content })
        "prompt" -> FixtureSubject.Prompt(obj.getValue("id").jsonPrimitive.content)
        else -> error("unknown subject kind $kind")
    }
}
