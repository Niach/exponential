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

    @Test
    fun `a seeded launch mounts on Temporary session, a plain mount on the default`() {
        assertEquals(null, ComposerAgentRow.seedPick(AgentComposerSeed()))
        assertEquals(null, ComposerAgentRow.seedPick(AgentComposerSeed(text = "hi")))
        assertEquals(ComposerAgentRow.TEMPORARY_PICK, ComposerAgentRow.seedPick(AgentComposerSeed(issueIds = listOf("i-3"))))
        assertEquals(ComposerAgentRow.TEMPORARY_PICK, ComposerAgentRow.seedPick(AgentComposerSeed(promptId = "p-1")))
        assertEquals(ComposerAgentRow.TEMPORARY_PICK, ComposerAgentRow.seedPick(AgentComposerSeed(deviceId = "d-1")))
    }

    @Test
    fun `an issue or prompt subject moves the row to Temporary session`() {
        assertEquals(ComposerAgentRow.TEMPORARY_PICK, ComposerAgentRow.pickForSubject(true, null))
        assertEquals(ComposerAgentRow.TEMPORARY_PICK, ComposerAgentRow.pickForSubject(true, "g-me"))
        assertEquals("g-me", ComposerAgentRow.pickForSubject(false, "g-me"))
        assertEquals(null, ComposerAgentRow.pickForSubject(false, null))
    }
}
