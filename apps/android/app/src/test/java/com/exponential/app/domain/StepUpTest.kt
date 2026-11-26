package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * #EXP-2143 follow-up: the step-up reducer, locked ×4 against
 * `domain-contract/fixtures/step-up.json` — same flows as the web
 * (`step-up.test.ts`), the desktop (domain::step_up) and iOS (StepUpTests).
 */
class StepUpTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("step-up.json")).jsonObject

    private fun string(obj: JsonObject, key: String): String? =
        obj[key]?.takeIf { it != JsonNull }?.jsonPrimitive?.content

    private fun methods(obj: JsonObject, key: String): List<StepUp.Method> =
        obj.getValue(key).jsonArray.map { StepUp.Method.fromWire(it.jsonPrimitive.content)!! }

    private fun event(obj: JsonObject): StepUp.Event = when (val on = string(obj, "on")) {
        "refused" -> StepUp.Event.Refused(string(obj, "code")!!, methods(obj, "methods"))
        "choose" -> StepUp.Event.Choose(StepUp.Method.fromWire(string(obj, "method")!!)!!)
        "submit" -> StepUp.Event.Submit
        "proved" -> StepUp.Event.Proved
        "rejected" -> StepUp.Event.Rejected(string(obj, "code")!!)
        "cancel" -> StepUp.Event.Cancel
        "done" -> StepUp.Event.Done
        else -> error("unknown event $on")
    }

    private fun state(obj: JsonObject) = StepUp.State(
        sheet = StepUp.Sheet.valueOf(string(obj, "sheet")!!.replaceFirstChar { it.uppercase() }),
        method = string(obj, "method")?.let { StepUp.Method.fromWire(it) },
        methods = methods(obj, "methods"),
        error = string(obj, "error"),
        action = StepUp.Action.valueOf(string(obj, "action")!!.replaceFirstChar { it.uppercase() }),
        retried = obj.getValue("retried").jsonPrimitive.boolean,
    )

    @Test
    fun `initial state and copy match the fixture`() {
        assertEquals(state(fixture.getValue("initial").jsonObject), StepUp.State())
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(string(copy, "title"), StepUp.TITLE)
        assertEquals(string(copy, "passkey"), StepUp.PASSKEY_BUTTON)
        assertEquals(string(copy, "confirm"), StepUp.CONFIRM)
        assertEquals(string(copy, "resend"), StepUp.RESEND)
        assertEquals(string(copy, "notFresh"), StepUp.NOT_FRESH_COPY)
        val description = copy.getValue("description").jsonObject
        val field = copy.getValue("field").jsonObject
        val switch = copy.getValue("switch").jsonObject
        for (method in StepUp.Method.entries) {
            assertEquals(string(description, method.wire)!!.replace("{email}", "a@b.c"), method.description("a@b.c"))
            assertEquals(string(field, method.wire), method.field)
            assertEquals(string(switch, method.wire), method.switchLabel)
        }
        for ((code, text) in fixture.getValue("errors").jsonObject) {
            val key = if (code == "default") "SOMETHING_ELSE" else code
            assertEquals(code, text.jsonPrimitive.content, StepUp.errorCopy(key))
        }
        assertEquals(null, StepUp.errorCopy(StepUp.CANCELLED))
    }

    @Test
    fun `flows`() {
        for (flow in fixture.getValue("flows").jsonArray.map { it.jsonObject }) {
            val end = flow.getValue("events").jsonArray
                .map { event(it.jsonObject) }
                .fold(StepUp.State(), StepUp::reduce)
            assertEquals(string(flow, "name"), state(flow.getValue("expect").jsonObject), end)
        }
    }
}
