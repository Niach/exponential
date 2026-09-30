package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1031: the stacking toast, against `toast-stack.json` (case `name` = message). */
class ToastStackTest {
    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("toast-stack.json")).jsonObject

    private fun JsonObject.num(key: String): Double = getValue(key).jsonPrimitive.double

    @Test
    fun `the constants are the contract's`() {
        val c = fixture().getValue("constants").jsonObject
        val k = ToastStack.Constants
        val expected = mapOf(
            "width" to k.WIDTH,
            "gap" to k.GAP,
            "peek" to k.PEEK,
            "scaleStep" to k.SCALE_STEP,
            "visible" to k.VISIBLE.toDouble(),
            "durationMs" to k.DURATION_MS.toDouble(),
            "swipeThreshold" to k.SWIPE_THRESHOLD,
            "viewportOffset" to k.VIEWPORT_OFFSET,
            "mobileViewportOffset" to k.MOBILE_VIEWPORT_OFFSET,
        )
        assertEquals(c.keys - "kinds", expected.keys)
        for ((key, value) in expected) assertEquals(key, c.num(key), value, 0.0)
        assertEquals(
            c.getValue("kinds").jsonArray.map { it.jsonPrimitive.content },
            k.KINDS,
        )
    }

    @Test
    fun `every geometry case lays the stack out`() {
        val cases = fixture().getValue("geometry").jsonArray
        assertTrue(cases.size >= 7)
        for (element in cases) {
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val layout = ToastStack.geometry(
                heights = case.getValue("heights").jsonArray.map { it.jsonPrimitive.double },
                expanded = case.getValue("expanded").jsonPrimitive.boolean,
                anchoredBottom = case.getValue("anchoredBottom").jsonPrimitive.boolean,
            )
            assertEquals("$name: height", case.num("height"), layout.height, 1e-9)
            val items = case.getValue("items").jsonArray
            assertEquals("$name: count", items.size, layout.items.size)
            items.forEachIndexed { i, item ->
                val want = item.jsonObject
                val got = layout.items[i]
                assertEquals("$name[$i]: offset", want.num("offset"), got.offset, 1e-9)
                assertEquals("$name[$i]: scale", want.num("scale"), got.scale, 1e-9)
                assertEquals(
                    "$name[$i]: visible",
                    want.getValue("visible").jsonPrimitive.boolean,
                    got.visible,
                )
            }
        }
    }
}
