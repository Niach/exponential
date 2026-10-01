package com.exponential.app.data.api

import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.AutomationTrigger
import com.exponential.app.domain.AutomationTriggerFilters
import com.exponential.app.domain.actionTriggerWireJson
import com.exponential.app.domain.parseActionTriggers
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * SLOP-2: wire-format locks for the ONE trigger write —
 * `actions.update({id, triggers})`, a WHOLE-ARRAY replace against a STRICT
 * server schema (no stray key survives). An existing trigger keeps its `id`,
 * a new one travels without (the server mints it); an unset pin is ABSENT;
 * an event names its `source`. The tests run the input through the shared
 * encoder (`explicitNulls = false`), not just `toString()`.
 */
class ActionTriggersWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private fun encode(input: JsonObject): String =
        json.encodeToString(JsonObject.serializer(), input)

    private val daily = AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 540)

    private val stored = ActionTrigger(
        id = "t-1",
        enabled = true,
        deviceId = "dev-1",
        agent = "claude",
        account = "work",
        model = "opus",
        effort = "high",
        whenPart = daily,
    )

    @Test
    fun `an existing trigger rides whole with its id`() {
        assertEquals(
            """{"id":"act-1","triggers":[{"id":"t-1","enabled":true,"deviceId":"dev-1",""" +
                """"agent":"claude","account":"work","model":"opus","effort":"high",""" +
                """"kind":"schedule","interval":"daily","minuteOfDay":540}]}""",
            encode(updateActionTriggersInput(id = "act-1", triggers = listOf(stored.toWireJson()))),
        )
    }

    @Test
    fun `a new trigger travels without an id and with unset pins absent`() {
        val added = actionTriggerWireJson(
            id = null,
            enabled = true,
            deviceId = "dev-2",
            agent = null,
            account = null,
            model = "",
            effort = null,
            whenPart = AutomationTrigger.Schedule(interval = "weekly", minuteOfDay = 480, weekday = 5),
        )
        assertEquals(
            """{"enabled":true,"deviceId":"dev-2",""" +
                """"kind":"schedule","interval":"weekly","minuteOfDay":480,"weekday":5}""",
            encode(added),
        )
        // Adding sends the existing triggers plus the new element.
        val input = updateActionTriggersInput(id = "act-1", triggers = listOf(stored.toWireJson(), added))
        assertEquals(setOf("id", "triggers"), input.keys)
    }

    @Test
    fun `an event names its source and keeps only its set filters`() {
        assertEquals(
            """{"id":"t-2","enabled":false,"deviceId":"dev-1",""" +
                """"kind":"event","source":"exponential","event":"status_changed",""" +
                """"filters":{"boardIds":["b1"],"toStatusIds":["s1"]}}""",
            encode(
                ActionTrigger(
                    id = "t-2",
                    enabled = false,
                    deviceId = "dev-1",
                    whenPart = AutomationTrigger.Event(
                        event = "status_changed",
                        filters = AutomationTriggerFilters(
                            boardIds = listOf("b1"),
                            toStatusIds = listOf("s1"),
                        ),
                    ),
                ).toWireJson(),
            ),
        )
    }

    @Test
    fun `an account rides only beside its agent`() {
        val orphan = actionTriggerWireJson(
            id = "t-3",
            enabled = true,
            deviceId = "dev-1",
            agent = "",
            account = "work",
            model = null,
            effort = null,
            whenPart = daily,
        )
        assertEquals(setOf("id", "enabled", "deviceId", "kind", "interval", "minuteOfDay"), orphan.keys)
    }

    @Test
    fun `deleting the last trigger sends an empty array`() {
        assertEquals(
            """{"id":"act-1","triggers":[]}""",
            encode(updateActionTriggersInput(id = "act-1", triggers = emptyList())),
        )
    }

    @Test
    fun `the wire form round-trips through the tolerant read`() {
        val event = stored.copy(
            id = "t-4",
            enabled = false,
            account = null,
            whenPart = AutomationTrigger.Event(event = "created"),
        )
        val array = encode(
            updateActionTriggersInput(id = "act-1", triggers = listOf(stored.toWireJson(), event.toWireJson())),
        ).substringAfter(""""triggers":""").removeSuffix("}")
        assertEquals(listOf(stored, event), parseActionTriggers(array))
    }

    @Test
    fun `a synced or fetched row hands its triggers to the dto`() {
        val dto = json.decodeFromString(
            ActionResult.serializer(),
            """{"action":{"id":"act-1","teamId":"team-1","name":"Digest","triggers":[""" +
                """{"id":"t-1","enabled":true,"deviceId":"dev-1","agent":"claude","account":"work",""" +
                """"model":"opus","effort":"high","kind":"schedule","interval":"daily","minuteOfDay":540},""" +
                """{"id":"t-9","deviceId":"dev-1","kind":"webhook"}]}}""",
        ).action
        // The future kind is skipped, never a crash.
        assertEquals(listOf(stored), dto.parsedTriggers)
        // Builtins and rows from before the column carry none.
        assertEquals(emptyList<ActionTrigger>(), builtinTidyUpAction("team-1").parsedTriggers)
    }
}
