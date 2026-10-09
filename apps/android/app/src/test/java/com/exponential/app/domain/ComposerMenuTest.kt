package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1249: the composer "+" menu, replayed off `composer-menu.json` (web
// `composer-menu.test.ts`, same case names).
class ComposerMenuTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("composer-menu.json")).jsonObject

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content

    @Test
    fun `the rows match the fixture's order, copy and glyphs`() {
        val rows = fixture.getValue("rows").jsonArray.map { it.jsonObject }
        assertEquals(rows.size, ComposerMenu.ROWS.size)
        rows.zip(ComposerMenu.ROWS).forEach { (want, got) ->
            if (want.string("kind") == "separator") {
                assertEquals(ComposerMenuEntry.Separator, got)
                return@forEach
            }
            val row = got as ComposerMenuEntry.Row
            assertEquals(want.string("id"), row.id.wire)
            assertEquals(want.string("kind"), row.kind.name.lowercase())
            assertEquals(want.string("label"), row.label)
            assertEquals(want["codexLabel"]?.jsonPrimitive?.content, row.codexLabel)
            assertEquals(want.string("icon"), row.icon)
            assertEquals(want["when"]?.jsonPrimitive?.content, row.condition?.wire)
        }
    }

    @Test
    fun `composerMenuLayout cases match the fixture`() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val c = case.getValue("conditions").jsonObject
            val layout = ComposerMenu.composerMenuLayout(
                ComposerMenuConditions(
                    subagentModel = c.getValue("subagentModel").jsonPrimitive.boolean,
                    ultracode = c.getValue("ultracode").jsonPrimitive.boolean,
                    mcp = c.getValue("mcp").jsonPrimitive.boolean,
                    computerUse = c.getValue("computerUse").jsonPrimitive.boolean,
                ),
            )
            assertEquals(
                case.string("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                layout.map { if (it is ComposerMenuEntry.Row) it.id.wire else "-" },
            )
        }
    }

    @Test
    fun `implementButtonLabel matches the fixture`() {
        for (element in fixture.getValue("implementLabels").jsonArray) {
            val case = element.jsonObject
            assertEquals(case.string("expected"), ComposerMenu.implementButtonLabel(case.getValue("count").jsonPrimitive.int))
        }
    }

    @Test
    fun `names the contract's computer-use payload key and cap`() {
        val computerUse = fixture.getValue("computerUse").jsonObject
        assertEquals(computerUse.string("payloadKey"), ComposerMenu.COMPUTER_USE_PAYLOAD_KEY)
        assertEquals(computerUse.string("cap"), ComposerMenu.COMPUTER_USE_RUN_CAP)
        assertTrue(ComposerMenu.COMPUTER_USE_PAYLOAD_KEY in DomainContract.codingSessionLaunchKeys)
    }

    @Test
    fun `the test ids, suggestions and plus label are the fixture's`() {
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.string("plus"), ComposerMenu.PLUS_TEST_ID)
        assertEquals(ids.string("menu"), ComposerMenu.MENU_TEST_ID)
        assertEquals(ids.string("implementSubmit"), ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID)
        assertEquals(ids.string("suggestion"), ComposerMenu.SUGGESTION_TEST_ID)
        assertEquals(ids.string("brandMark"), ComposerMenu.BRAND_MARK_TEST_ID)
        assertEquals(ids.string("row").replace("{id}", "effort"), ComposerMenu.rowTestId(ComposerMenuRowId.Effort))
        assertEquals(fixture.string("plusLabel"), ComposerMenu.PLUS_LABEL)
        val suggestions = fixture.getValue("suggestions").jsonObject
        assertEquals(suggestions.getValue("count").jsonPrimitive.int, ComposerMenu.SUGGESTION_COUNT)
        assertEquals(suggestions.string("icon"), ComposerMenu.SUGGESTION_ICON)
    }
}
