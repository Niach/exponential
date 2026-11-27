package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-2109: replays `header-actions.json` (TS `header-actions.test.ts`). */
class HeaderActionsTest {
    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("header-actions.json")).jsonObject }

    private fun strings(element: JsonElement): List<String> = element.jsonArray.map { it.jsonPrimitive.content }

    @Test
    fun `the constants equal the fixture`() {
        assertEquals(fixture.getValue("circle").jsonPrimitive.int, HeaderActions.CIRCLE)
        assertEquals(HeaderActions.CIRCLE, Chats.Shell.Header.CIRCLE)
        assertEquals(fixture.getValue("glyph").jsonPrimitive.int, HeaderActions.GLYPH)
        assertEquals(fixture.getValue("gap").jsonPrimitive.int, HeaderActions.GAP)
        assertEquals(strings(fixture.getValue("order")), HeaderActions.ORDER)
        assertEquals(strings(fixture.getValue("menuDrops")), HeaderActions.MENU_DROPS)
        assertEquals(fixture.getValue("icons").jsonObject.mapValues { it.value.jsonPrimitive.content }, HeaderActions.ICONS)
    }

    @Test
    fun `order and menu cases replay`() {
        val order = fixture.getValue("orderCases").jsonArray.map { it.jsonObject }
        val menu = fixture.getValue("menuCases").jsonArray.map { it.jsonObject }
        assertTrue(order.isNotEmpty() && menu.isNotEmpty())
        for (case in order) {
            assertEquals(case["name"].toString(), strings(case.getValue("expected")), HeaderActions.order(strings(case.getValue("actions"))))
        }
        for (case in menu) {
            assertEquals(case["name"].toString(), strings(case.getValue("expected")), HeaderActions.menu(strings(case.getValue("entries"))))
        }
    }

    @Test
    fun `every page lists its actions in the order`() {
        for ((page, forms) in fixture.getValue("pages").jsonObject) {
            for (form in listOf("phone", "wide")) {
                val list = (forms.jsonObject.getValue(form) as? kotlinx.serialization.json.JsonArray) ?: continue
                assertEquals("$page $form", strings(list), HeaderActions.order(strings(list)))
            }
        }
    }

    @Test
    fun `collapse cases replay`() {
        val collapse = fixture.getValue("collapse").jsonObject
        assertEquals(collapse.getValue("titleMinShare").jsonPrimitive.double, HeaderActions.TITLE_MIN_SHARE, 0.0)
        assertEquals(strings(collapse.getValue("order")), HeaderActions.COLLAPSE_ORDER)
        val cases = fixture.getValue("collapseCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val chips = case.getValue("chips").jsonArray.map { chip ->
                val c = chip.jsonObject
                HeaderActions.CollapseChip(c.getValue("action").jsonPrimitive.content, c.getValue("full").jsonPrimitive.float, c.getValue("compact").jsonPrimitive.float)
            }
            assertEquals(
                case["name"].toString(),
                strings(case.getValue("expected")),
                HeaderActions.collapse(
                    case.getValue("bar").jsonPrimitive.float,
                    case.getValue("title").jsonPrimitive.float,
                    case.getValue("fixed").jsonPrimitive.float,
                    chips,
                ),
            )
        }
    }
}
