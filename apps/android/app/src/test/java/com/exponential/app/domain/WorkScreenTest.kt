package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** NEW-59: [WorkScreen] replays `work-screen.json` (copy, testIds, optionCases, pickerKeyCases). */
class WorkScreenTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("work-screen.json")).jsonObject

    private fun strOrNull(element: kotlinx.serialization.json.JsonElement?): String? =
        element?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    @Test
    fun copyMatchesTheFixture() {
        val copy = fixture.getValue("copy").jsonObject
        // Android mirrors every string key but the desktop-only refusals.
        assertEquals(copy.keys - "refusals", WorkScreen.Copy.all.keys)
        for ((key, value) in WorkScreen.Copy.all) {
            assertEquals(key, copy.getValue(key).jsonPrimitive.content, value)
        }
        assertEquals(
            fixture.getValue("testIds").jsonObject.getValue("row").jsonPrimitive.content,
            WorkScreen.ROW_TEST_ID,
        )
    }

    @Test
    fun everyOptionCaseReplays() {
        val cases = fixture.getValue("optionCases").jsonArray
        assert(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val displays = case["displays"]?.takeIf { it !is JsonNull }?.jsonArray?.map {
                val d = it.jsonObject
                WorkScreenDisplay(
                    id = d.getValue("id").jsonPrimitive.content,
                    number = d.getValue("number").jsonPrimitive.int,
                    width = d.getValue("width").jsonPrimitive.int,
                    height = d.getValue("height").jsonPrimitive.int,
                    primary = d.getValue("primary").jsonPrimitive.boolean,
                )
            }
            val expect = case.getValue("expect").jsonArray.map {
                val o = it.jsonObject
                WorkScreenOption(strOrNull(o["value"]), o.getValue("label").jsonPrimitive.content)
            }
            assertEquals(name, expect, workScreenOptions(displays, strOrNull(case["stored"])))
        }
    }

    @Test
    fun pickerKeysMatchTheFixture() {
        val keys = fixture.getValue("pickerKeys").jsonObject
        assertEquals(keys.getValue("auto").jsonPrimitive.content, WorkScreen.PickerKeys.AUTO)
        assertEquals(keys.getValue("display").jsonPrimitive.content, WorkScreen.PickerKeys.DISPLAY)
        for (element in fixture.getValue("pickerKeyCases").jsonArray) {
            val case = element.jsonObject
            val value = strOrNull(case["value"])
            val key = case.getValue("key").jsonPrimitive.content
            assertEquals(key, WorkScreen.pickerKey(value))
            assertEquals(value, WorkScreen.pickedId(key))
        }
    }

    @Test
    fun doctorDisplaysDecodeLeniently() {
        val doctor = parseDeviceDoctor(
            """{"items":[],"displays":[{"id":"uuid-a","number":1,"width":2560,"height":1440,"primary":false},""" +
                """{"number":2,"width":1920,"height":1080,"primary":false},""" +
                """{"id":"uuid-c","number":3,"width":3024,"height":1964,"primary":true},{"id":"uuid-d","number":"x"}]}""",
        )
        assertEquals(
            listOf(
                WorkScreenDisplay("uuid-a", 1, 2560, 1440, false),
                // #FEED-161: the entry without an `id` dropped alone.
                WorkScreenDisplay("uuid-c", 3, 3024, 1964, true),
            ),
            doctor?.displays,
        )
        assertNull(parseDeviceDoctor("""{"items":[]}""")?.displays)
    }
}
