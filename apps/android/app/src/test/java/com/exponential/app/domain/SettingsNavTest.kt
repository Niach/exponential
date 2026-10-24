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
}
