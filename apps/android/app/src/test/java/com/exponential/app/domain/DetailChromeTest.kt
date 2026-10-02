package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-1162: the detail chrome, locked ×4 against `detail-chrome.json` (web
 * `detail-chrome.test.ts`, iOS `DetailChromeTests`, desktop
 * `domain::detail_chrome`). Each case's `name` is the assertion message.
 */
class DetailChromeTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("detail-chrome.json")).jsonObject

    @Test
    fun `the constants are the contract's`() {
        val c = fixture().getValue("constants").jsonObject
        val expected = mapOf(
            "collapseMs" to DetailChrome.COLLAPSE_MS.toFloat(),
            "collapseRise" to DetailChrome.COLLAPSE_RISE,
            "edgeTop" to DetailChrome.EDGE_TOP,
            "edgeBottom" to DetailChrome.EDGE_BOTTOM,
            "edgeBlur" to DetailChrome.EDGE_BLUR,
            "scrim" to DetailChrome.SCRIM,
        )
        assertEquals(expected.keys, c.keys)
        for ((key, value) in expected) {
            assertEquals(key, c.getValue(key).jsonPrimitive.float, value, 0f)
        }
    }

    @Test
    fun `every collapse case matches the fixture`() {
        val cases = fixture().getValue("collapse").jsonArray
        assertEquals(6, cases.size)
        for (element in cases) {
            val case = element.jsonObject
            val titleBottom = case.getValue("titleBottom").let { if (it is JsonNull) null else it.jsonPrimitive.double }
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("collapsed").jsonPrimitive.boolean,
                DetailChrome.isTitleCollapsed(
                    hasTitleRow = case.getValue("hasTitleRow").jsonPrimitive.boolean,
                    titleBottom = titleBottom,
                    headerBottom = case.getValue("headerBottom").jsonPrimitive.double,
                ),
            )
        }
    }
}
