package com.exponential.app.domain

import com.exponential.app.ui.icons.ExpIcons
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1381: the Team page's settings rows are fixture-locked ×3 phones —
 * `settings-nav.json` `rows` + `phoneRows` + `testIds` replay against the
 * generated [SettingsNav] mirror (iOS `SettingsNavTests`, TS
 * `settings-nav.test.ts`).
 */
class SettingsNavTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("settings-nav.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun rows() {
        val expected = fixture.getValue("rows").jsonArray.map { it.jsonObject }.map { json ->
            SettingsNav.Row(
                id = json.str("id")!!,
                label = json.str("label")!!,
                icon = json.str("icon")!!,
                group = json.str("group")!!,
                visible = json.str("visible")!!,
                phone = json.getValue("phone").jsonPrimitive.boolean,
                count = json.str("count"),
            )
        }
        assertEquals(expected, SettingsNav.ROWS)
    }

    @Test
    fun phoneRows() {
        val expected = fixture.getValue("phoneRows").jsonArray.map { it.jsonPrimitive.content }
        assertEquals(expected, SettingsNav.PHONE_ROWS)
        // Every phone row is a `phone: true` row, in `rows` order.
        assertEquals(SettingsNav.ROWS.filter { it.phone }.map { it.id }, SettingsNav.PHONE_ROWS)
        // Apps is not a row (the Apps tab); Devices is the last phone row.
        assertTrue(SettingsNav.ROWS.none { it.id == "apps" })
        assertEquals(SettingsNav.DEVICES, SettingsNav.PHONE_ROWS.last())
    }

    @Test
    fun everyPhoneRowHasAGlyph() {
        for (id in SettingsNav.PHONE_ROWS) {
            assertNotNull(id, ExpIcons.byConcept(SettingsNav.row(id).icon))
        }
    }

    @Test
    fun testIds() {
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("row"), SettingsNav.ROW_TEST_ID)
        assertEquals(ids.str("page"), SettingsNav.PAGE_TEST_ID)
    }

    @Test
    fun `visibleCases replay`() {
        for (case in fixture.getValue("visibleCases").jsonArray.map { it.jsonObject }) {
            val isOwner = case.str("role") == "owner"
            val cloud = case.getValue("cloud").jsonPrimitive.boolean
            val expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content }
            val expectedPhone = case.getValue("expectedPhone").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.str("name"), expected, SettingsNav.ROWS.filter { SettingsNav.isVisible(it, isOwner, cloud) }.map { it.id })
            assertEquals(case.str("name"), expectedPhone, SettingsNav.visiblePhoneRows(isOwner, cloud))
        }
    }

    // S-3: THE settings nav (`nav`), replayed off `navCases`; the mirror's
    // groups, pages, platforms and new entries equal the fixture's.
    private val nav: JsonObject get() = fixture.getValue("nav").jsonObject

    @Test
    fun `nav groups and pages equal the fixture`() {
        val groups = nav.getValue("groups").jsonArray.map { it.jsonObject }.map { g ->
            SettingsNavGroups.Group(
                id = g.str("id")!!,
                label = g.str("label")!!,
                platforms = (g["platforms"] as? kotlinx.serialization.json.JsonArray)?.map { it.jsonPrimitive.content },
                items = g.getValue("items").jsonArray.map { it.jsonPrimitive.content },
            )
        }
        assertEquals(groups, SettingsNavGroups.GROUPS)
        val pages = nav.getValue("pages").jsonObject.mapValues { (_, v) ->
            val p = v.jsonObject
            SettingsNavGroups.Page(
                p.str("label")!!, p.str("icon")!!, p.str("visible")!!,
                (p["platforms"] as? kotlinx.serialization.json.JsonArray)?.map { it.jsonPrimitive.content },
            )
        }
        assertEquals(pages, SettingsNavGroups.PAGES)
        assertEquals(
            nav.getValue("children").jsonObject.mapValues { (_, v) -> v.jsonArray.map { it.jsonPrimitive.content } },
            SettingsNavGroups.CHILDREN,
        )
        assertEquals(
            nav.getValue("rowPlatforms").jsonObject.mapValues { (_, v) -> v.jsonArray.map { it.jsonPrimitive.content } },
            SettingsNavGroups.ROW_PLATFORMS,
        )
        assertEquals(
            nav.getValue("newEntries").jsonObject.mapValues { (_, v) -> v.jsonPrimitive.content },
            SettingsNavGroups.NEW_ENTRIES,
        )
        val ids = SettingsNavGroups.GROUPS.flatMap { it.items } + SettingsNavGroups.CHILDREN.values.flatten()
        for (id in ids.filter { !it.startsWith("@") }) assertNotNull(id, SettingsNavGroups.page(id))
    }

    @Test
    fun `navCases replay`() {
        val cases = nav.getValue("navCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val expected = case.getValue("expected").jsonObject.mapValues { (_, v) -> v.jsonArray.map { it.jsonPrimitive.content } }
            val actual = SettingsNavGroups.groups(
                platform = case.str("platform")!!,
                isOwner = case.str("role") == "owner",
                cloud = case.getValue("cloud").jsonPrimitive.boolean,
            )
            assertEquals(case.str("name"), expected.toList(), actual.toList())
        }
    }

    @Test
    fun `android nav for an owner and a member`() {
        val owner = SettingsNavGroups.groups(isOwner = true)
        assertEquals(listOf("team", "boards", "agents", "apps", "personal"), owner.keys.toList())
        assertEquals(listOf("general", "members", "issues", "repositories", "prompts", "widget", "storage"), owner["team"])
        assertEquals(listOf("account", "notifications", "security", "about"), owner["personal"])
        val member = SettingsNavGroups.groups(isOwner = false)
        assertEquals(listOf("@assistant", "@agents", "mcp-servers"), member["agents"])
    }
}
