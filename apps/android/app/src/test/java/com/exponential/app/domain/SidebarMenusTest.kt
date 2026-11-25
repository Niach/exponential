package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-2124, #EXP-2109: replays `sidebar-menus.json` (TS `sidebar-menus.test.ts`). */
class SidebarMenusTest {
    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("sidebar-menus.json")).jsonObject }

    private fun JsonObject.str(key: String): String? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun strings(element: JsonElement): List<String> = element.jsonArray.map { it.jsonPrimitive.content }

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun map(key: String): Map<String, String> =
        fixture.getValue(key).jsonObject.mapValues { it.value.jsonPrimitive.content }

    private fun input(case: JsonObject): SidebarMenus.Input {
        val o = case.getValue("input").jsonObject
        fun flag(name: String) = o[name]?.jsonPrimitive?.boolean ?: false
        return SidebarMenus.Input(owner = flag("owner"), mine = flag("mine"), canDelete = flag("canDelete"))
    }

    @Test
    fun `the table equals the fixture`() {
        assertEquals(strings(fixture.getValue("groups")), SidebarMenus.GROUPS)
        assertEquals(map("copy"), SidebarMenus.COPY)
        assertEquals(map("icons"), SidebarMenus.ICONS)
        assertEquals(map("newCopy"), SidebarMenus.NEW_COPY)
        assertEquals(map("newIcons"), SidebarMenus.NEW_ICONS)
        assertEquals(strings(fixture.getValue("newKinds")), SidebarMenus.NEW_KINDS)
        assertEquals(strings(fixture.getValue("newOwnerOnly")), SidebarMenus.NEW_OWNER_ONLY)
        assertEquals(strings(fixture.getValue("phone").jsonObject.getValue("drops")), SidebarMenus.PHONE_DROPS)
        val kinds = fixture.getValue("kinds").jsonObject.mapValues { (_, groups) ->
            groups.jsonObject.mapValues { (_, entries) ->
                entries.jsonArray.map { e ->
                    (e as? JsonObject)?.let { SidebarMenus.Entry(it.str("id")!!, it.str("when")) }
                        ?: SidebarMenus.Entry(e.jsonPrimitive.content)
                }
            }.filterValues { it.isNotEmpty() }
        }
        assertEquals(kinds, SidebarMenus.KINDS)
    }

    @Test
    fun `menu cases replay`() {
        for (case in cases("menuCases")) {
            assertEquals(case.str("name"), strings(case.getValue("expected")), SidebarMenus.itemMenu(case.str("kind")!!, input(case)))
        }
    }

    @Test
    fun `phone cases replay`() {
        for (case in cases("phoneCases")) {
            assertEquals(case.str("name"), strings(case.getValue("expected")), SidebarMenus.phoneMenu(case.str("kind")!!, input(case)))
        }
    }

    @Test
    fun `new cases replay`() {
        for (case in cases("newCases")) {
            assertEquals(case.str("name"), strings(case.getValue("expected")), SidebarMenus.newKinds(case.getValue("owner").jsonPrimitive.boolean))
        }
    }

    @Test
    fun `move cases replay`() {
        for (case in cases("moveCases")) {
            val rows = case.getValue("rows").jsonArray.map { it.jsonObject }.map {
                SidebarMenus.MoveRow(it.str("kind")!!, it.str("id")!!, it.getValue("depth").jsonPrimitive.int, it.str("name"))
            }
            val expected = case.getValue("expected").jsonObject.let { o ->
                SidebarMenus.MoveTargets(
                    o.getValue("folders").jsonArray.map { it.jsonObject }.map { SidebarMenus.MoveFolder(it.str("id")!!, it.str("name")) },
                    o.str("topLevel"),
                )
            }
            assertEquals(case.str("name"), expected, SidebarMenus.moveTargets(rows, case.str("id")!!))
        }
    }

    @Test
    fun `every glyph resolves`() {
        for (concept in SidebarMenus.ICONS.values + SidebarMenus.NEW_ICONS.values) {
            assertTrue(concept, com.exponential.app.ui.icons.ExpIcons.byConcept(concept) != null)
        }
    }
}
