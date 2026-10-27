package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * NEW-69: caller mode, locked ×4 (TS `voice-call.ts`, desktop
 * `domain::voice_call`, iOS `VoiceCallTests`) against
 * `fixtures/voice-call.json`: every `copy` key (reasons + failed included),
 * the `testIds`, `icons`, `constants`, `wire`, `surfaceSpecimens`, and every
 * `availabilityCases` / `flowCases` row (view + toast).
 */
class VoiceCallTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("voice-call.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun obj(key: String): JsonObject = fixture.getValue(key).jsonObject

    private fun JsonObject.strings(): Map<String, String?> = keys.associateWith { str(it) }

    private fun cases(key: String): List<JsonObject> {
        val cases = fixture.getValue(key).jsonArray.map { it.jsonObject }
        assertTrue(key, cases.isNotEmpty())
        return cases
    }

    @Test
    fun `copy matches the fixture key by key`() {
        val copy = obj("copy")
        val c = VoiceCall.Copy
        val flat = mapOf(
            "call" to c.CALL,
            "connecting" to c.CONNECTING,
            "listening" to c.LISTENING,
            "thinking" to c.THINKING,
            "speaking" to c.SPEAKING,
            "muted" to c.MUTED,
            "mutedLine" to c.MUTED_LINE,
            "mute" to c.MUTE,
            "unmute" to c.UNMUTE,
            "endCall" to c.END_CALL,
            "queued" to c.QUEUED,
        )
        // A key added to the fixture must land here too.
        assertEquals(copy.keys - setOf("reasons", "failed"), flat.keys)
        flat.forEach { (key, value) -> assertEquals(key, copy.str(key), value) }
        val reasons = mapOf(
            "noVoiceDevice" to VoiceCall.Copy.Reasons.NO_VOICE_DEVICE,
            "speakOff" to VoiceCall.Copy.Reasons.SPEAK_OFF,
            "offline" to VoiceCall.Copy.Reasons.OFFLINE,
            "notHere" to VoiceCall.Copy.Reasons.NOT_HERE,
            "preparing" to VoiceCall.Copy.Reasons.PREPARING,
            "noGpu" to VoiceCall.Copy.Reasons.NO_GPU,
            "busy" to VoiceCall.Copy.Reasons.BUSY,
        )
        assertEquals(copy.getValue("reasons").jsonObject.strings(), reasons)
        assertEquals(copy.getValue("failed").jsonObject.strings(), VoiceCall.Copy.Failed.byCode)
        assertEquals("Call Triage", c.call("Triage"))
    }

    @Test
    fun `test ids, icons, constants and wire match the fixture`() {
        val ids = mapOf(
            "call" to VoiceCall.TestIds.CALL,
            "surface" to VoiceCall.TestIds.SURFACE,
            "mute" to VoiceCall.TestIds.MUTE,
            "end" to VoiceCall.TestIds.END,
        )
        assertEquals(obj("testIds").strings(), ids)
        assertEquals(obj("icons").strings(), mapOf("call" to VoiceCall.Icons.CALL))
        val k = VoiceCall.Constants
        val constants = mapOf(
            "ringTimeoutMs" to k.RING_TIMEOUT_MS,
            "minSilenceMs" to k.MIN_SILENCE_MS,
            "bargeInMs" to k.BARGE_IN_MS,
            "frameMs" to k.FRAME_MS,
            "upRate" to k.UP_RATE,
            "downRate" to k.DOWN_RATE,
            "upKbps" to k.UP_KBPS,
            "downKbps" to k.DOWN_KBPS,
            "jitterMs" to k.JITTER_MS,
            "maxFramesPerSecond" to k.MAX_FRAMES_PER_SECOND,
            "maxCallMs" to k.MAX_CALL_MS,
            "idleTimeoutMs" to k.IDLE_TIMEOUT_MS,
        )
        val fixtureConstants = obj("constants")
        assertEquals(fixtureConstants.keys.associateWith { fixtureConstants.getValue(it).jsonPrimitive.int }, constants)
        val wire = obj("wire")
        assertEquals(setOf("kinds", "maxFrameBytes", "control"), wire.keys)
        val kinds = wire.getValue("kinds").jsonObject
        assertEquals(mapOf("up" to VoiceCall.Wire.KIND_UP, "down" to VoiceCall.Wire.KIND_DOWN), kinds.keys.associateWith { kinds.getValue(it).jsonPrimitive.int })
        assertEquals(wire.getValue("maxFrameBytes").jsonPrimitive.int, VoiceCall.Wire.MAX_FRAME_BYTES)
        assertEquals(wire.getValue("control").jsonObject.keys.toList(), VoiceCall.Wire.control)
    }

    @Test
    fun `an audio frame is kind, u16 big-endian seq, packet`() {
        assertArrayEquals(byteArrayOf(1, 0xff.toByte(), 0xfe.toByte(), 7, 8), VoiceCall.Wire.frame(VoiceCall.Wire.KIND_UP, 0xfffe, byteArrayOf(7, 8)))
        assertArrayEquals(byteArrayOf(2, 0, 0), VoiceCall.Wire.frame(VoiceCall.Wire.KIND_DOWN, 0x10000, byteArrayOf()))
    }

    @Test
    fun `surface specimens match the fixture`() {
        val specimens = cases("surfaceSpecimens").map { s ->
            VoiceCall.SurfaceSpecimen(
                activity = VoiceCall.Activity.entries.single { it.wire == s.str("activity") },
                level = s.getValue("level").jsonPrimitive.double,
                bands = s.getValue("bands").jsonArray.map { it.jsonPrimitive.double },
                state = s.str("state")!!,
                line = s.str("line")!!,
                mute = s.str("mute")!!,
            )
        }
        assertEquals(specimens, VoiceCall.surfaceSpecimens)
    }

    @Test
    fun `every availability case matches the fixture`() {
        for (case in cases("availabilityCases")) {
            val input = case.getValue("input").jsonObject
            val got = VoiceCall.callAvailability(
                VoiceCall.AvailabilityInput(
                    platform = VoiceCall.Platform.entries.single { it.wire == input.str("platform") },
                    device = input.str("device"),
                    speakReplies = input.getValue("speakReplies").jsonPrimitive.boolean,
                    online = input.getValue("online").jsonPrimitive.boolean,
                    here = input.getValue("here").jsonPrimitive.boolean,
                    speech = input.str("speech")?.let { s -> VoiceCall.Speech.entries.single { it.wire == s } },
                    busy = input.getValue("busy").jsonPrimitive.boolean,
                ),
            )
            val expected = case.getValue("expected").jsonObject
            assertEquals(
                case.str("name"),
                VoiceCall.Availability(expected.getValue("enabled").jsonPrimitive.boolean, expected.str("reason")),
                got,
            )
        }
    }

    @Test
    fun `every flow case matches the fixture`() {
        for (case in cases("flowCases")) {
            val name = case.str("name")
            var state = VoiceCall.CALL_IDLE
            for (raw in case.getValue("events").jsonArray) {
                val event = VoiceCall.Event.of(raw.jsonObject)
                assertNotNull("$name: $raw", event)
                state = VoiceCall.callStep(state, event!!)
            }
            val view = VoiceCall.callView(state)
            val expected = case["view"]?.takeUnless { it is JsonNull }?.jsonObject
            if (expected == null) {
                assertNull(name, view)
            } else {
                assertEquals(
                    name,
                    VoiceCall.View(
                        activity = VoiceCall.Activity.entries.single { it.wire == expected.str("activity") },
                        state = expected.str("state")!!,
                        line = expected.str("line")!!,
                        mute = expected.str("mute")!!,
                    ),
                    view,
                )
            }
            assertEquals(name, case.str("toast"), state.toast)
        }
    }

    /** A code the fixture has no copy for (the relay's `no_such_call`) reads "dropped". */
    @Test
    fun `an unknown failure code reads dropped`() {
        val started = VoiceCall.callStep(VoiceCall.CALL_IDLE, VoiceCall.Event.Start("mint"))
        assertEquals(VoiceCall.Copy.Failed.DROPPED, VoiceCall.callStep(started, VoiceCall.Event.Failed("no_such_call")).toast)
        assertEquals("mint is offline", VoiceCall.callStep(started, VoiceCall.Event.Failed("device_offline")).toast)
    }

    /** The relay's `busy` error and its `max_length` / `idle` `call_end`s end a running call with their toast. */
    @Test
    fun `relay ends toast`() {
        fun ended(last: VoiceCall.Event) = listOf(VoiceCall.Event.Start("mint"), VoiceCall.Event.Connected, last)
            .fold(VoiceCall.CALL_IDLE, VoiceCall::callStep)
        assertEquals("mint is in another call", ended(VoiceCall.Event.Failed("busy")).toast)
        // hangUpRules: ANY reason (one this build doesn't know too) drops.
        for (reason in listOf("max_length", "idle", "dropped", "hangup")) {
            val state = ended(VoiceCall.Event.HungUp(reason))
            assertEquals(reason, VoiceCall.Copy.Failed.DROPPED, state.toast)
            assertNull(reason, VoiceCall.callView(state))
        }
        for (reason in listOf(null, "")) {
            val state = ended(VoiceCall.Event.HungUp(reason))
            assertNull(state.toast)
            assertNull(VoiceCall.callView(state))
        }
    }

    /** Muted, every captured frame still goes to the encoder as silence, so the relay's idle timeout never fires. */
    @Test
    fun `muted uplink keeps the call alive`() {
        val n = VoiceCall.Uplink.FRAME_SAMPLES
        assertEquals(320, n)
        val speech = ShortArray(n) { 1200 }
        assertArrayEquals(speech, VoiceCall.Uplink.samples(speech, n, muted = false))
        // A whole idle window of muted capture: one silent frame per FRAME_MS.
        val frames = (0 until VoiceCall.Constants.IDLE_TIMEOUT_MS / VoiceCall.Constants.FRAME_MS)
            .map { VoiceCall.Uplink.samples(speech, n, muted = true) }
        assertEquals(1500, frames.size)
        assertTrue(frames.all { f -> f.size == n && f.all { it == 0.toShort() } })
        // The captured buffer itself is left alone.
        assertTrue(speech.all { it == 1200.toShort() })
    }

    /** `hangUpRules`: the fixture's rule is the phone's. */
    @Test
    fun `hang-up rules match the fixture`() {
        val rules = obj("hangUpRules").strings()
        assertEquals(mapOf("withReason" to "failed", "failedCode" to VoiceCall.HangUpRules.FAILED_CODE, "withoutReason" to "end"), rules)
    }

    /**
     * A call muted for 40 s on a fake clock against the relay's idle check
     * (steer-relay hub.ts: every min(idleTimeoutMs, 5 s), a pair without a
     * frame for idleTimeoutMs gets `call_end` idle = `HungUp("idle")`): the
     * muted capture keeps one frame per FRAME_MS going, so the call survives;
     * a client that sent nothing is dropped by the same check.
     */
    @Test
    fun `a call muted for 40 s outlives the relay's idle check`() {
        val k = VoiceCall.Constants
        val n = VoiceCall.Uplink.FRAME_SAMPLES
        val captured = ShortArray(n) { 900 }
        for (sends in listOf(true, false)) {
            var state = listOf(VoiceCall.Event.Start("mint"), VoiceCall.Event.Connected, VoiceCall.Event.Mute)
                .fold(VoiceCall.CALL_IDLE, VoiceCall::callStep)
            var lastFrameAt = 0
            var frames = 0
            val check = minOf(k.IDLE_TIMEOUT_MS, 5_000)
            for (now in k.FRAME_MS..40_000 step k.FRAME_MS) {
                if (sends) {
                    val frame = VoiceCall.Uplink.samples(captured, n, muted = state.muted)
                    assertTrue(frame.all { it == 0.toShort() })
                    frames += 1
                    lastFrameAt = now
                }
                if (now % check == 0 && state.phase != VoiceCall.Phase.Hidden && now - lastFrameAt >= k.IDLE_TIMEOUT_MS) {
                    state = VoiceCall.callStep(state, VoiceCall.Event.HungUp("idle"))
                }
            }
            if (sends) {
                assertEquals(40_000 / k.FRAME_MS, frames)
                assertEquals(VoiceCall.Phase.Listening, state.phase)
                assertTrue(state.muted)
                assertNull(state.toast)
            } else {
                assertEquals(VoiceCall.Phase.Hidden, state.phase)
                assertEquals(VoiceCall.Copy.Failed.DROPPED, state.toast)
            }
        }
    }

    @Test
    fun `callSpeech reads the voice device's speech row`() {
        fun doctor(vararg items: DeviceDoctorItem) = DeviceDoctor(items = items.toList())
        assertNull(VoiceCall.callSpeech(null))
        assertNull(VoiceCall.callSpeech(doctor()))
        assertNull(VoiceCall.callSpeech(doctor(DeviceDoctorItem(key = "speech", group = "agents", state = "ok"))))
        assertEquals(VoiceCall.Speech.Ok, VoiceCall.callSpeech(doctor(DeviceDoctorItem(key = "speech", group = "voice", state = "ok"))))
        assertEquals(VoiceCall.Speech.NoGpu, VoiceCall.callSpeech(doctor(DeviceDoctorItem(key = "speech", group = "voice", state = "missing"))))
        assertEquals(VoiceCall.Speech.Preparing, VoiceCall.callSpeech(doctor(DeviceDoctorItem(key = "speech", group = "voice", state = "unchecked"))))
        assertEquals(VoiceCall.Speech.Preparing, VoiceCall.callSpeech(doctor(DeviceDoctorItem(key = "speech", group = "voice", state = "downloading"))))
    }
}
