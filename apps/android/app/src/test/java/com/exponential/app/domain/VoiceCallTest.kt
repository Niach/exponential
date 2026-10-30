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
import kotlinx.serialization.json.long
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
        Json.parseToJsonElement(etalkFixtureJson("voice-call.json")).jsonObject
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
            "calling" to c.CALLING,
            "gettingReady" to c.GETTING_READY,
            "noSound" to c.NO_SOUND,
            "listening" to c.LISTENING,
            "thinking" to c.THINKING,
            "speaking" to c.SPEAKING,
            "muted" to c.MUTED,
            "mute" to c.MUTE,
            "unmute" to c.UNMUTE,
            "speaker" to c.SPEAKER,
            "speakerOff" to c.SPEAKER_OFF,
            "endCall" to c.END_CALL,
            "cancel" to c.CANCEL,
            "queued" to c.QUEUED,
        )
        // A key added to the fixture must land here too.
        assertEquals(copy.keys - setOf("reasons", "failed", "greeting"), flat.keys)
        assertEquals(copy.getValue("greeting").jsonObject.strings(), c.greeting)
        flat.forEach { (key, value) -> assertEquals(key, copy.str(key), value) }
        val reasons = mapOf(
            "noAgent" to VoiceCall.Copy.Reasons.NO_AGENT,
            "noVoiceDevice" to VoiceCall.Copy.Reasons.NO_VOICE_DEVICE,
            "offline" to VoiceCall.Copy.Reasons.OFFLINE,
            "notHere" to VoiceCall.Copy.Reasons.NOT_HERE,
            "preparing" to VoiceCall.Copy.Reasons.PREPARING,
            "voiceFailed" to VoiceCall.Copy.Reasons.VOICE_FAILED,
            "noGpu" to VoiceCall.Copy.Reasons.NO_GPU,
            "busy" to VoiceCall.Copy.Reasons.BUSY,
            "meeting" to VoiceCall.Copy.Reasons.MEETING,
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
            "speaker" to VoiceCall.TestIds.SPEAKER,
            "panel" to VoiceCall.TestIds.PANEL,
            "pill" to VoiceCall.TestIds.PILL,
            "timer" to VoiceCall.TestIds.TIMER,
        )
        assertEquals(obj("testIds").strings(), ids)
        val i = VoiceCall.Icons
        assertEquals(
            obj("icons").strings(),
            mapOf("call" to i.CALL, "mute" to i.MUTE, "unmute" to i.UNMUTE, "speaker" to i.SPEAKER, "speakerOff" to i.SPEAKER_OFF, "end" to i.END),
        )
        val pill = obj("pill")
        assertEquals(
            pill.keys.associateWith { pill.getValue(it).jsonPrimitive.int },
            mapOf("width" to VoiceCall.Pill.WIDTH, "height" to VoiceCall.Pill.HEIGHT, "marginPx" to VoiceCall.Pill.MARGIN_PX),
        )
        val k = VoiceCall.Constants
        val constants = mapOf<String, Number>(
            "minRingMs" to k.MIN_RING_MS,
            "ringJitterMs" to k.RING_JITTER_MS,
            "ringToneMs" to k.RING_TONE_MS,
            "ringGapMs" to k.RING_GAP_MS,
            "ringLowHz" to k.RING_LOW_HZ,
            "ringHighHz" to k.RING_HIGH_HZ,
            "maxRingMs" to k.MAX_RING_MS,
            "noSoundLevel" to k.NO_SOUND_LEVEL,
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
        assertEquals(fixtureConstants.keys, constants.keys)
        for ((key, value) in constants) {
            assertEquals(key, fixtureConstants.getValue(key).jsonPrimitive.double, value.toDouble(), 0.0)
        }
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
                name = s.str("name")!!,
                agent = s.str("agent")!!,
                timer = s.str("timer"),
                level = s.getValue("level").jsonPrimitive.double,
                bands = s.getValue("bands").jsonArray.map { it.jsonPrimitive.double },
                view = view(s),
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
                    agent = input.getValue("agent").jsonPrimitive.boolean,
                    platform = VoiceCall.Platform.entries.single { it.wire == input.str("platform") },
                    device = input.str("device"),
                    online = input.getValue("online").jsonPrimitive.boolean,
                    here = input.getValue("here").jsonPrimitive.boolean,
                    speech = input.str("speech")?.let { s -> VoiceCall.Speech.entries.single { it.wire == s } },
                    busy = input.getValue("busy").jsonPrimitive.boolean,
                    meeting = input["meeting"]?.jsonPrimitive?.boolean ?: false,
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
            val platform = VoiceCall.Platform.entries.single { it.wire == case.str("platform") }
            val view = VoiceCall.callView(state, platform)
            val expected = case["view"]?.takeUnless { it is JsonNull }?.jsonObject
            if (expected == null) {
                assertNull(name, view)
            } else {
                assertEquals(name, view(expected), view)
            }
            assertEquals(name, case.str("toast"), state.toast)
        }
    }

    private fun view(v: JsonObject) = VoiceCall.View(
        activity = VoiceCall.Activity.entries.single { it.wire == v.str("activity") },
        status = v.str("status"),
        caption = v.str("caption")!!,
        muted = v.getValue("muted").jsonPrimitive.boolean,
        speaker = v.getValue("speaker").jsonPrimitive.boolean,
        buttons = v.getValue("buttons").jsonArray.map { b -> VoiceCall.Button.entries.single { it.wire == b.jsonPrimitive.content } },
        end = v.str("end")!!,
    )

    @Test
    fun `every timer case matches the fixture`() {
        for (case in cases("timerCases")) {
            val ms = case.getValue("ms").jsonPrimitive.long
            assertEquals("$ms", case.str("text"), VoiceCall.callTimer(ms))
        }
        assertEquals("00:00", VoiceCall.callTimer(-5))
    }

    @Test
    fun `EXP-1423 the ring matches the fixture`() {
        for (case in cases("ringCases")) {
            val random = case.getValue("random").jsonPrimitive.double
            assertEquals("$random", case.getValue("ringMs").jsonPrimitive.int, VoiceCall.callRingMs(random))
        }
        for (case in cases("answerCases")) {
            val random = case.getValue("random").jsonPrimitive.double
            val ready = case.str("readyMs")?.toInt()
            assertEquals(case.str("name"), case.str("answeredMs")?.toInt(), VoiceCall.callAnswerMs(random, ready))
        }
        for (case in cases("ringToneCases")) {
            val ms = case.getValue("ms").jsonPrimitive.long
            assertEquals("$ms", case.getValue("on").jsonPrimitive.boolean, VoiceCall.callRingToneOn(ms))
        }
    }

    @Test
    fun `the greeting falls back to English`() {
        assertEquals("Hallo?", VoiceCall.callGreeting("de"))
        assertEquals("Hello?", VoiceCall.callGreeting(null))
        assertEquals("Hello?", VoiceCall.callGreeting("fr"))
    }

    /** A code the fixture has no copy for (the relay's `no_such_call`) reads "dropped". */
    @Test
    fun `an unknown failure code reads dropped`() {
        val started = VoiceCall.callStep(VoiceCall.CALL_IDLE, VoiceCall.Event.Start("mint"))
        assertEquals(VoiceCall.Copy.Failed.DROPPED, VoiceCall.callStep(started, VoiceCall.Event.Failed("no_such_call")).toast)
        assertEquals("mint is offline", VoiceCall.callStep(started, VoiceCall.Event.Failed("device_offline")).toast)
    }

    /** A host's `call_event {failed {code}}` is a host event and fails the call with that code's toast; an unknown code reads "dropped". */
    @Test
    fun `a host failed event fails the call with its toast`() {
        fun hostEvent(json: String) = VoiceCall.Event.of(Json.parseToJsonElement(json).jsonObject)
        val ringing = listOf(VoiceCall.Event.Start("mint"), VoiceCall.Event.Rang).fold(VoiceCall.CALL_IDLE, VoiceCall::callStep)
        val expected = mapOf(
            "unavailable" to "mint couldn't get its voice ready",
            "dropped" to "The call dropped",
            "no_microphone" to "No microphone",
            "not_granted" to "Microphone not allowed",
            "meeting" to "mint is transcribing a meeting",
            "worker_died" to "The call dropped",
            "" to "The call dropped",
        )
        for ((code, toast) in expected) {
            val event = hostEvent("""{"t":"failed","code":"$code"}""")
            assertEquals(code, VoiceCall.Event.Failed(code), event)
            assertTrue(code, event!!.isHostEvent)
            val state = VoiceCall.callStep(ringing, event)
            assertEquals(code, toast, state.toast)
            assertNull(code, VoiceCall.callView(state))
        }
        // The caller's own lifecycle never comes from the host.
        for (t in listOf("start", "mute", "unmute", "speaker", "end", "hungUp")) {
            assertFalse(t, hostEvent("""{"t":"$t"}""")!!.isHostEvent)
        }
        for (t in listOf("rang", "ready", "noSound", "partial", "said", "queued", "replyStarted", "replyFinished", "speechStarted")) {
            assertTrue(t, hostEvent("""{"t":"$t"}""")!!.isHostEvent)
        }
    }

    /** The relay's `busy` error and its `max_length` / `idle` `call_end`s end a running call with their toast. */
    @Test
    fun `relay ends toast`() {
        fun ended(last: VoiceCall.Event) = listOf(VoiceCall.Event.Start("mint"), VoiceCall.Event.Rang, VoiceCall.Event.Ready, VoiceCall.Event.ReplyFinished, last)
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
            var state = listOf(VoiceCall.Event.Start("mint"), VoiceCall.Event.Rang, VoiceCall.Event.Ready, VoiceCall.Event.ReplyFinished, VoiceCall.Event.Mute)
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
        assertEquals(VoiceCall.Speech.Failed, VoiceCall.callSpeech(doctor(DeviceDoctorItem(key = "speech", group = "voice", state = "error"))))
    }

    @Test
    fun `every speech case matches the fixture`() {
        val rows = cases("speechCases")
        assertTrue(rows.isNotEmpty())
        for (case in rows) {
            val expected = case.str("expected")?.let { e -> VoiceCall.Speech.entries.single { it.wire == e } }
            assertEquals(case.str("name"), expected, VoiceCall.callSpeechOf(case.str("state")))
        }
    }
}
