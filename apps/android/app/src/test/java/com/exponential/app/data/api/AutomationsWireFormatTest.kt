package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-995: wire-format locks for `automations.update`. The input is
 * TRI-STATE (iOS `UpdateInput` parity): an untouched key is ABSENT (the
 * server keeps the stored value), a cleared launch pin is a LITERAL null
 * (back to the device's launch defaults), a picked one its string. The
 * shared Json drops null properties (`explicitNulls = false`), which is why
 * the input is a [JsonObject] and why these tests run it through that
 * encoder, not just `toString()`.
 */
class AutomationsWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private val trigger: JsonObject = buildJsonObject {
        put("kind", "schedule")
        put("interval", "daily")
        put("minuteOfDay", 540)
    }

    private fun encode(input: JsonObject): String =
        json.encodeToString(JsonObject.serializer(), input)

    @Test
    fun `clearing the account pin sends a literal null`() {
        val input = updateAutomationInput(
            id = "auto-1",
            actionId = "act-1",
            deviceId = "dev-1",
            trigger = trigger,
            launch = AutomationLaunchPatch(agent = "claude", account = null, model = null, effort = null),
        )
        assertEquals(
            """{"id":"auto-1","actionId":"act-1","deviceId":"dev-1",""" +
                """"trigger":{"kind":"schedule","interval":"daily","minuteOfDay":540},""" +
                """"agent":"claude","account":null,"model":null,"effort":null}""",
            encode(input),
        )
    }

    @Test
    fun `a blank pin clears like a null one`() {
        val input = updateAutomationInput(
            id = "auto-1",
            launch = AutomationLaunchPatch(agent = "claude", account = "", model = "", effort = ""),
        )
        assertEquals(
            """{"id":"auto-1","agent":"claude","account":null,"model":null,"effort":null}""",
            encode(input),
        )
    }

    @Test
    fun `an untouched launch patch omits every pin key`() {
        val input = updateAutomationInput(id = "auto-1", enabled = false)
        assertEquals("""{"id":"auto-1","enabled":false}""", encode(input))
        // The enable toggle's exact wire: nothing else travels.
        assertEquals(setOf("id", "enabled"), input.keys)
    }

    @Test
    fun `a picked account rides as its string`() {
        val input = updateAutomationInput(
            id = "auto-1",
            launch = AutomationLaunchPatch(agent = "claude", account = "work", model = "opus", effort = "high"),
        )
        assertEquals(
            """{"id":"auto-1","agent":"claude","account":"work","model":"opus","effort":"high"}""",
            encode(input),
        )
    }
}
