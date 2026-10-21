package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1291 Threads v2: Team settings → Agents is fixture-locked ×4 — every
 * `summaryCases` / `validationCases` / `accountValueCases` entry of `agent-settings.json` runs here
 * (TS `agent-settings.test.ts`), the validation output re-encoded in the
 * fixture's shape and compared WHOLE; the embedded copy and fields are proved
 * equal to the fixture's.
 */
class AgentSettingsTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("agent-settings.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `every builtin event has a summary`() {
        for (event in DomainContract.eventBuiltinNames) {
            assertTrue(event, !AgentSettings.Copy.EVENTS[event].isNullOrEmpty())
        }
        assertEquals(DomainContract.eventBuiltinNames.toSet(), AgentSettings.Copy.EVENTS.keys)
    }

    @Test
    fun `every summary case matches the fixture`() {
        val cases = fixture.getValue("summaryCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val trigger = case.getValue("trigger").jsonObject
            val like = AgentSettings.TriggerLike(
                kind = trigger.str("kind")!!,
                enabled = trigger["enabled"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean,
                cron = trigger.str("cron"),
                tz = trigger.str("tz"),
                name = trigger.str("name"),
            )
            assertEquals(case.str("expected"), AgentSettings.triggerSummary(like))
            // The same text off the PARSED stored trigger (the Triggers rows).
            val parsed = parseAgentTrigger(trigger)
            assertTrue(trigger.toString(), parsed != null)
            if (parsed != null) assertEquals(case.str("expected"), AgentSettings.triggerSummary(parsed))
        }
    }

    @Test
    fun `every validation case matches the fixture`() {
        val cases = fixture.getValue("validationCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val form = case.getValue("form").jsonObject
            val actual = AgentSettings.validateAgentForm(
                AgentSettings.Form(
                    name = form.str("name")!!,
                    deviceId = form.str("deviceId"),
                    instructions = form.str("instructions")!!,
                ),
            )
            val encoded = JsonArray(
                actual.map { error ->
                    buildJsonObject {
                        put("field", error.field)
                        put("message", error.message)
                    }
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `every account value case matches the fixture`() {
        val cases = fixture.getValue("accountValueCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val options = input.getValue("options").jsonArray.map { it.jsonObject }.map { option ->
                AgentSettings.AccountValueOption(
                    id = option.str("id")!!,
                    agent = option.str("agent")!!,
                    email = option.str("email")!!,
                    isLastUsed = option.getValue("isLastUsed").jsonPrimitive.boolean,
                )
            }
            val actual = AgentSettings.agentAccountValue(options, input.str("account"), input.str("cli"))
            val encoded = actual?.let { picked ->
                buildJsonObject {
                    put("agent", picked.agent)
                    put("email", picked.email)
                }
            } ?: JsonNull
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `the fields equal the fixture`() {
        val fields = fixture.getValue("fields").jsonArray.map { it.jsonObject }
        assertEquals(fields.size, AgentSettings.FIELDS.size)
        fields.zip(AgentSettings.FIELDS).forEach { (json, field) ->
            assertEquals(json.str("key"), field.key)
            assertEquals(json.str("label"), field.label)
            assertEquals(json.str("control"), field.control)
            assertEquals(json.getValue("required").jsonPrimitive.boolean, field.required)
            assertEquals(json["ownerOnly"]?.jsonPrimitive?.boolean ?: false, field.ownerOnly)
        }
    }

    @Test
    fun `the copy equals the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), AgentSettings.Copy.TITLE)
        assertEquals(copy.str("newAgent"), AgentSettings.Copy.NEW_AGENT)
        assertEquals(copy.str("namePlaceholder"), AgentSettings.Copy.NAME_PLACEHOLDER)
        assertEquals(copy.str("instructionsPlaceholder"), AgentSettings.Copy.INSTRUCTIONS_PLACEHOLDER)
        assertEquals(copy.str("addTrigger"), AgentSettings.Copy.ADD_TRIGGER)
        assertEquals(copy.str("noSchedule"), AgentSettings.Copy.NO_SCHEDULE)
        assertEquals(copy.str("paused"), AgentSettings.Copy.PAUSED)
        assertEquals(copy.str("deleteConfirm"), AgentSettings.Copy.DELETE_CONFIRM)
        // NEW-2: a schedule's summary is the schedule picker's (SchedulePickerTest).
        assertTrue(copy["schedule"] == null)
        assertEquals(copy.str("event"), AgentSettings.Copy.EVENT)
        val events = copy.getValue("events").jsonObject
        assertEquals(events.keys, AgentSettings.Copy.EVENTS.keys)
        for ((key, value) in events) assertEquals(value.jsonPrimitive.content, AgentSettings.Copy.EVENTS[key])
        val errors = copy.getValue("errors").jsonObject
        assertEquals(errors.str("nameRequired"), AgentSettings.Copy.ERROR_NAME_REQUIRED)
        assertEquals(errors.str("nameTooLong"), AgentSettings.Copy.ERROR_NAME_TOO_LONG)
        assertEquals(errors.str("deviceRequired"), AgentSettings.Copy.ERROR_DEVICE_REQUIRED)
        assertEquals(errors.str("instructionsTooLong"), AgentSettings.Copy.ERROR_INSTRUCTIONS_TOO_LONG)
        assertEquals(copy.str("accountLastUsed"), AgentSettings.Copy.ACCOUNT_LAST_USED)
        val testIds = fixture.getValue("testIds").jsonObject
        assertEquals(testIds.str("list"), AgentSettings.TEST_ID_LIST)
        assertEquals(testIds.str("page"), AgentSettings.TEST_ID_PAGE)
    }

    @Test
    fun `an agent trigger rides the strict wire shape`() {
        val schedule = AgentTrigger(
            id = null,
            enabled = true,
            whenPart = TriggerWhen.Schedule(cron = "0 9 * * 1-5", tz = "Europe/Vienna"),
        )
        assertEquals(
            """{"enabled":true,"kind":"schedule","cron":"0 9 * * 1-5","tz":"Europe/Vienna"}""",
            schedule.toWireJson().toString(),
        )
        val utc = AgentTrigger(id = null, enabled = true, whenPart = TriggerWhen.Schedule(cron = "0 0 * * 0"))
        assertEquals("""{"enabled":true,"kind":"schedule","cron":"0 0 * * 0"}""", utc.toWireJson().toString())
        val event = AgentTrigger(id = "t-1", enabled = false, whenPart = TriggerWhen.Event(name = "pr.merged"))
        assertEquals(
            """{"id":"t-1","enabled":false,"kind":"event","name":"pr.merged"}""",
            event.toWireJson().toString(),
        )
        // …and reads back tolerant: one unreadable element is skipped.
        val raw = JsonArray(listOf(event.toWireJson(), JsonPrimitive("junk"))).toString()
        assertEquals(listOf(event), parseAgentTriggers(raw))
    }
}
