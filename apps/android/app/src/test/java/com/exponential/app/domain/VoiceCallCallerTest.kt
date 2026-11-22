package com.exponential.app.domain

import com.exponential.app.domain.VoiceCall.Caller.Effect
import com.exponential.app.domain.VoiceCall.Caller.Frame
import com.exponential.app.domain.VoiceCall.Caller.Input
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1993: the phone caller's session, every `caller.cases` row of
 * `fixtures/voice-call.json` (iOS replays the same rows in
 * `VoiceCallCallerTests`): per event the effects in order, then the final
 * stage and phase.
 */
class VoiceCallCallerTest {
    private val cases by lazy {
        Json.parseToJsonElement(etalkFixtureJson("voice-call.json")).jsonObject.getValue("caller").jsonObject.getValue("cases").jsonArray.map { it.jsonObject }
    }

    private fun JsonObject.str(key: String): String? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean

    private fun input(e: JsonObject): Input = when (e.str("t")) {
        "start" -> Input.Start(e.str("device")!!, e.str("language"), e.bool("fallback") ?: false)
        "held" -> Input.Held(e.bool("ok")!!, e.str("code"))
        "ticket" -> Input.Ticket(e.str("device"), e.bool("fallback"), e.bool("sealed") ?: false)
        "refused" -> Input.Refused(e.str("message")!!)
        "frame" -> Input.Received(Frame.of(e.getValue("frame").jsonObject))
        "closed" -> Input.Closed
        "failed" -> Input.Failed(e.str("code")!!, e.bool("told") ?: false)
        "ringTimeout" -> Input.RingTimeout
        "mute" -> Input.Mute
        "speaker" -> Input.Speaker
        "end" -> Input.End
        else -> error("unknown input $e")
    }

    private fun json(effect: Effect): JsonObject = buildJsonObject {
        when (effect) {
            Effect.Hold -> put("do", "hold")
            is Effect.Ring -> { put("do", "ring"); put("ms", effect.ms) }
            Effect.Ticket -> put("do", "ticket")
            Effect.Dial -> put("do", "dial")
            Effect.StartAudio -> put("do", "startAudio")
            Effect.StopRing -> put("do", "stopRing")
            Effect.Flush -> put("do", "flush")
            is Effect.Mute -> { put("do", "mute"); put("muted", effect.muted) }
            is Effect.Speaker -> { put("do", "speaker"); put("on", effect.on) }
            is Effect.Send -> { put("do", "send"); put("frame", effect.frame.json) }
            Effect.Alive -> put("do", "alive")
            Effect.Release -> put("do", "release")
            is Effect.Toast -> { put("do", "toast"); put("text", effect.text) }
        }
    }

    @Test
    fun everyCaseReplays() {
        assertTrue(cases.size >= 31)
        for (entry in cases) {
            val name = entry.str("name")
            val events = entry.getValue("events").jsonArray.map { it.jsonObject }
            val effects = entry.getValue("effects").jsonArray.map { row -> row.jsonArray.map { it.jsonObject } }
            assertEquals(name, events.size, effects.size)
            var state = VoiceCall.Caller.IDLE
            events.forEachIndexed { index, event ->
                val stepped = VoiceCall.Caller.step(state, input(event))
                state = stepped.state
                assertEquals("$name: event $index $event", effects[index], stepped.effects.map(::json))
            }
            assertEquals(name, entry.str("stage"), state.stage.wire)
            assertEquals(name, entry.str("phase"), state.call.phase.name.lowercase())
            entry.str("device")?.let { assertEquals(name, it, state.call.device) }
            entry.bool("fallback")?.let { assertEquals(name, it, state.call.fallback) }
            entry.bool("muted")?.let { assertEquals(name, it, state.call.muted) }
            entry.bool("speaker")?.let { assertEquals(name, it, state.call.speaker) }
        }
    }

    @Test
    fun theRingCarriesTheFixturesBound() {
        val ring = cases.first().getValue("effects").jsonArray[1].jsonArray[0].jsonObject
        assertEquals(VoiceCall.Constants.MAX_RING_MS, ring.getValue("ms").jsonPrimitive.int)
    }
}
