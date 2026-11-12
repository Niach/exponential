package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
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
    fun `add-file's phone choices match the fixture`() {
        val choices = fixture.getValue("addFile").jsonObject.getValue("phoneChoices").jsonArray.map { it.jsonObject }
        assertEquals(choices.size, ComposerAddFileChoice.entries.size)
        choices.zip(ComposerAddFileChoice.entries).forEach { (want, got) ->
            assertEquals(want.string("id"), got.wire)
            assertEquals(want.string("label"), got.label)
            assertEquals(want.string("icon"), got.icon)
        }
        assertEquals(ComposerAddFileChoice.entries.toList(), ComposerAddFileChoice.visible(hasCamera = true))
        assertEquals(
            listOf(ComposerAddFileChoice.Photos, ComposerAddFileChoice.File),
            ComposerAddFileChoice.visible(hasCamera = false),
        )
    }

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
            val whenList = when (val w = want["when"]) {
                null -> emptyList()
                is JsonArray -> w.map { it.jsonPrimitive.content }
                else -> listOf(w.jsonPrimitive.content)
            }
            assertEquals(whenList, row.conditions.map { it.wire })
            assertEquals(
                want.getValue("surfaces").jsonArray.map { it.jsonPrimitive.content }.toSet(),
                row.surfaces.map { it.wire }.toSet(),
            )
        }
    }

    @Test
    fun `composerMenuLayout cases match the fixture`() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val c = case.getValue("conditions").jsonObject
            val surface = ComposerMenuSurface.entries.single { it.wire == case.string("surface") }
            // #EXP-1715: a case names only the conditions it needs (absent = false).
            fun b(key: String) = c[key]?.jsonPrimitive?.boolean ?: false
            val layout = ComposerMenu.composerMenuLayout(
                surface,
                ComposerMenuConditions(
                    temporary = b("temporary"),
                    device = b("device"),
                    account = b("account"),
                    model = b("model"),
                    repositories = b("repositories"),
                    ultracode = b("ultracode"),
                    mcp = b("mcp"),
                    mcpPick = b("mcpPick"),
                    skills = b("skills"),
                    computerUse = b("computerUse"),
                    prompts = b("prompts"),
                    assistant = b("assistant"),
                    teams = b("teams"),
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
    fun `the conditions are the fixture's`() {
        assertEquals(
            fixture.getValue("conditions").jsonObject.keys,
            ComposerMenuCondition.entries.map { it.wire }.toSet(),
        )
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
    fun `the test ids, submenu geometry and plus label are the fixture's`() {
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.string("plus"), ComposerMenu.PLUS_TEST_ID)
        assertEquals(ids.string("menu"), ComposerMenu.MENU_TEST_ID)
        assertEquals(ids.string("implementSubmit"), ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID)
        assertEquals(ids.string("submenuSearch"), ComposerMenu.SUBMENU_SEARCH_TEST_ID)
        // EXP-1259: the suggestion rows and the brand mark left the contract.
        assertTrue("suggestion" !in ids && "brandMark" !in ids && "suggestions" !in fixture)
        assertEquals(ids.string("row").replace("{id}", "effort"), ComposerMenu.rowTestId(ComposerMenuRowId.Effort))
        assertEquals(fixture.string("plusLabel"), ComposerMenu.PLUS_LABEL)
        assertEquals(fixture.getValue("submenuRowHeight").jsonPrimitive.int, ComposerMenu.SUBMENU_ROW_HEIGHT)
        assertEquals(fixture.getValue("submenuMaxRows").jsonPrimitive.int, ComposerMenu.SUBMENU_MAX_ROWS)
    }

    @Test
    fun `teamsPick copy and valueCases match the fixture (EXP-1715)`() {
        val block = fixture.getValue("teamsPick").jsonObject
        val copy = block.getValue("copy").jsonObject
        assertEquals(copy.string("all"), TeamsPick.ALL)
        assertEquals(copy.string("many"), TeamsPick.MANY)
        val cases = block.getValue("valueCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val picked = (case["picked"] as? JsonArray)?.map { it.jsonPrimitive.content }
            val teams = case.getValue("teams").jsonArray.map { it.jsonObject }.map { TeamsPick.Team(it.string("id"), it.string("name")) }
            assertEquals(case.string("name"), case.string("expected"), TeamsPick.value(picked, teams))
        }
    }

    @Test
    fun `the teams pick never empties and every team reads as null (EXP-1715)`() {
        val ids = listOf("t1", "t2", "t3")
        assertEquals(null, TeamsPick.next(listOf("t3", "t1", "t2"), ids))
        assertEquals(null, TeamsPick.next(emptyList(), ids))
        assertEquals(listOf("t1", "t3"), TeamsPick.next(listOf("t3", "t1"), ids))
        assertEquals(listOf("t2"), TeamsPick.remove(listOf("t1", "t2"), ids, "t1"))
        assertEquals(null, TeamsPick.remove(listOf("t2"), ids, "t2"))
        assertEquals(emptyList<String>(), TeamsPick.chips(null, ids) { it })
        assertEquals(emptyList<String>(), TeamsPick.chips(ids, ids) { it })
        assertEquals(listOf("t2"), TeamsPick.chips(listOf("t2", "t9"), ids) { it })
    }
}
