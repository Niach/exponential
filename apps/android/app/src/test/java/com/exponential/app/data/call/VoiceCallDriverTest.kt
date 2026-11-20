package com.exponential.app.data.call

import com.exponential.app.data.api.StartCallResult
import com.exponential.app.domain.VoiceCall
import com.exponential.app.domain.etalkFixtureJson
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
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
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1993: every `caller.cases` row of `fixtures/voice-call.json` through
 * the REAL [VoiceCallDriver] (what [VoiceCallController] runs) with a fake
 * Android, socket and seal line: each fixture event becomes what the app
 * would see (the tap, the hold's answer, `startCall`'s answer, a frame off
 * the socket, a failure Android reports, the line's failure, the ring's
 * timer), and the calls the driver makes for that event are asserted
 * against the effects the row expects (iOS: `VoiceCallDriverTests`).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class VoiceCallDriverTest {
    private class Timer(val label: String, val body: () -> Unit, val log: MutableList<String>) : CallTimer {
        var live = true
        override fun cancel() {
            if (!live) return
            live = false
            log += "cancel $label"
        }
    }

    private class FakeSocket(val log: MutableList<String>) : CallSocket {
        val frames = Channel<String>(Channel.UNLIMITED)
        override fun send(text: String) { log += "send $text" }
        override fun sendLast(text: String) { log += "sendLast $text" }
        override fun sendBinary(frame: ByteArray) { log += "sendBinary" }
        override suspend fun receive(onText: (String) -> Unit, onBinary: (ByteArray) -> Unit, onOther: () -> Unit) {
            log += "receive"
            for (text in frames) onText(text)
        }
        override fun close() {
            log += "socket.close"
            frames.close()
        }
    }

    private class FakeSeal(val log: MutableList<String>) : CallSeal {
        override fun ready() { log += "seal.ready" }
        /** A `sealed` event opens to the control frame inside it. */
        override fun event(t: String?, data: String?): String? = if (t == "sealed") data else null
        override fun frame(bytes: ByteArray): ByteArray? = bytes
        override fun uplink(frame: ByteArray, muted: Boolean): ByteArray? = frame
        override fun close() { log += "seal.close" }
    }

    private class Refusal(message: String) : Exception(message)

    private class FakeAndroid(val log: MutableList<String>) : CallPlatform {
        var answer: ((Boolean, String?) -> Unit)? = null
        var report: ((String) -> Unit)? = null
        var ticket: CompletableDeferred<StartCallResult>? = null
        var socket: FakeSocket? = null
        var sealFail: ((String) -> Unit)? = null
        val timers = mutableListOf<Timer>()
        var now = 0L

        override fun hold(speaker: Boolean, report: (code: String) -> Unit, answer: (ok: Boolean, code: String?) -> Unit) {
            log += "hold speaker $speaker"
            this.report = report
            this.answer = answer
        }
        override fun ring() { log += "ring" }
        override fun stopRing() { log += "stopRing" }
        override fun setSpeaker(on: Boolean) { log += "speaker $on" }
        override fun setMuted(muted: Boolean) { log += "mute $muted" }
        override fun flush() { log += "flush" }
        override fun startAudio(muted: Boolean, speaker: Boolean) { log += "startAudio" }
        override fun play(packet: ByteArray) { log += "play" }
        override fun toast(text: String) { log += "toast $text" }
        override fun release() { log += "release" }
        override suspend fun keysReady(): Boolean {
            log += "keysReady"
            return true
        }
        override suspend fun keyTarget(agentId: String) = CallKeyTarget(null, null)
        override suspend fun startCall(agentId: String, seal: Boolean): StartCallResult {
            log += "startCall $agentId seal $seal"
            return CompletableDeferred<StartCallResult>().also { ticket = it }.await()
        }
        override fun refusal(error: Throwable): String = error.message ?: "?"
        override suspend fun dial(url: String, wanted: () -> Boolean): CallSocket? {
            log += "dial $url"
            return FakeSocket(log).also { socket = it }
        }
        override fun makeSeal(ticket: StartCallResult, agentId: String, target: CallKeyTarget, socket: CallSocket, end: (String) -> Unit, fail: (String) -> Unit): CallSeal {
            log += "makeSeal ${ticket.deviceId}"
            sealFail = fail
            return FakeSeal(log)
        }
        override fun after(ms: Long, body: () -> Unit): CallTimer = Timer("after $ms", body, log).also {
            log += "after $ms"
            timers += it
        }
        override fun every(ms: Long, body: () -> Unit): CallTimer = Timer("every $ms", body, log).also {
            log += "every $ms"
            timers += it
        }
        override fun nowMs(): Long = now
        override fun log(message: String) {}
    }

    private val cases by lazy {
        Json.parseToJsonElement(etalkFixtureJson("voice-call.json")).jsonObject.getValue("caller").jsonObject.getValue("cases").jsonArray.map { it.jsonObject }
    }

    private fun JsonObject.str(key: String): String? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean? = this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean

    /** What Android and the socket report for one fixture event. */
    private fun stimulate(event: JsonObject, driver: VoiceCallDriver, android: FakeAndroid) {
        when (event.str("t")) {
            "start" -> driver.start("agent-1", event.str("device")!!, event.str("language"), event.bool("fallback") ?: false)
            "held" -> android.answer?.invoke(event.bool("ok")!!, event.str("code"))
            "ticket" -> {
                val sealed = event.bool("sealed") ?: false
                android.ticket?.complete(
                    StartCallResult(
                        callId = "call-1", ticket = "t", url = RELAY, deviceId = if (sealed) "d-mint" else null,
                        device = event.str("device"), fallback = event.bool("fallback"), seal = sealed,
                    ),
                )
            }
            "refused" -> android.ticket?.completeExceptionally(Refusal(event.str("message")!!))
            "frame" -> {
                val frame = event.getValue("frame").jsonObject
                // A sealed call's host events come sealed (its line opens them); the rest stays readable.
                val sealed = driver.caller.sealed && frame.str("t") == "call_event" && frame["event"]?.jsonObject?.str("t") != "failed"
                val wire = if (!sealed) frame else buildJsonObject {
                    put("t", "call_event")
                    putJsonObject("event") {
                        put("t", "sealed")
                        put("data", frame.toString())
                    }
                }
                android.socket?.frames?.trySend(wire.toString())
            }
            // The far side went: the reader ends.
            "closed" -> android.socket?.frames?.close()
            "failed" -> if (event.bool("told") == true) android.sealFail?.invoke(event.str("code")!!) else android.report?.invoke(event.str("code")!!)
            "ringTimeout" -> android.timers.firstOrNull { it.live && it.label.startsWith("after") }?.body?.invoke()
            "mute" -> driver.toggleMute()
            "speaker" -> driver.toggleSpeaker()
            "end" -> driver.end()
            else -> error("unknown input $event")
        }
    }

    /** What the call holds before an event's effects (the calls `release` makes). */
    private data class Holds(
        var ringTimer: Boolean = false,
        var watch: Boolean = false,
        var seal: Boolean = false,
        var socket: Boolean = false,
    )

    private fun calls(event: JsonObject, effects: List<JsonObject>, stage: VoiceCall.Caller.Stage, holds: Holds): List<String> {
        val calls = mutableListOf<String>()
        for (effect in effects) {
            when (effect.str("do")) {
                "hold" -> calls += "hold speaker true"
                "ring" -> {
                    calls += listOf("ring", "after ${effect.getValue("ms").jsonPrimitive.int}")
                    holds.ringTimer = true
                }
                "ticket" -> calls += listOf("keysReady", "startCall agent-1 seal true")
                "dial" -> {
                    val sealed = event.bool("sealed") ?: false
                    calls += "dial $RELAY"
                    if (sealed) calls += "makeSeal d-mint"
                    calls += listOf("every 1000", "receive")
                    holds.watch = true
                    holds.seal = sealed
                    holds.socket = true
                }
                "startAudio" -> {
                    calls += "startAudio"
                    // A sealed call's line offers its hello once the microphone runs.
                    if (holds.seal && stage == VoiceCall.Caller.Stage.Dialed) calls += "seal.ready"
                }
                "stopRing" -> {
                    if (holds.ringTimer) calls += "cancel after ${VoiceCall.Constants.MAX_RING_MS}"
                    holds.ringTimer = false
                    calls += "stopRing"
                }
                "flush" -> calls += "flush"
                "mute" -> calls += "mute ${effect.bool("muted")}"
                "speaker" -> calls += "speaker ${effect.bool("on")}"
                "send" -> if (holds.socket) {
                    val frame = effect.getValue("frame").jsonObject
                    if (frame.str("t") == "call_end") {
                        calls += "sendLast $frame"
                        holds.socket = false
                    } else {
                        calls += "send $frame"
                    }
                }
                "alive" -> Unit
                "release" -> {
                    if (holds.ringTimer) calls += "cancel after ${VoiceCall.Constants.MAX_RING_MS}"
                    if (holds.watch) calls += "cancel every 1000"
                    if (holds.seal) calls += "seal.close"
                    calls += "release"
                    if (holds.socket) calls += "socket.close"
                    holds.ringTimer = false
                    holds.watch = false
                    holds.seal = false
                    holds.socket = false
                }
                "toast" -> calls += "toast ${effect.str("text")}"
                else -> error("unknown effect $effect")
            }
        }
        return calls
    }

    @Test
    fun everyCaseRunsThroughTheDriver() = runTest {
        assertTrue(cases.size >= 31)
        for (entry in cases) {
            val name = entry.str("name")
            val events = entry.getValue("events").jsonArray.map { it.jsonObject }
            val effects = entry.getValue("effects").jsonArray.map { row -> row.jsonArray.map { it.jsonObject } }
            val log = mutableListOf<String>()
            val android = FakeAndroid(log)
            val driver = VoiceCallDriver(android, backgroundScope)
            val holds = Holds()
            events.forEachIndexed { index, event ->
                log.clear()
                val expected = calls(event, effects[index], driver.caller.stage, holds)
                stimulate(event, driver, android)
                runCurrent()
                assertEquals("$name: event $index $event", expected, log)
            }
            assertEquals(name, entry.str("stage"), driver.caller.stage.wire)
            assertEquals(name, entry.str("phase"), driver.caller.call.phase.name.lowercase())
            entry.str("device")?.let { assertEquals(name, it, driver.caller.call.device) }
            entry.bool("fallback")?.let { assertEquals(name, it, driver.caller.call.fallback) }
            entry.bool("muted")?.let { assertEquals(name, it, driver.caller.call.muted) }
            entry.bool("speaker")?.let { assertEquals(name, it, driver.caller.call.speaker) }
            if (driver.caller.stage == VoiceCall.Caller.Stage.Idle) {
                // Nothing of an ended call is left to fire.
                assertEquals(name, emptyList<String>(), android.timers.filter { it.live }.map { it.label })
            }
        }
    }

    /** EXP-1929 (`livenessCases`): once the relay beat, 30 s without a frame drops the call, and the host is told. */
    @Test
    fun aPathGoneSilentAfterTheBeatEndsTheCallDropped() = runTest {
        val log = mutableListOf<String>()
        val android = FakeAndroid(log)
        val driver = VoiceCallDriver(android, backgroundScope)
        answered(driver, android)
        val watch = android.timers.single { it.label == "every 1000" }
        android.now = VoiceCall.Constants.IDLE_TIMEOUT_MS.toLong()
        watch.body()
        assertEquals("no beat yet (an older relay): no deadline", VoiceCall.Caller.Stage.Paired, driver.caller.stage)
        android.now = 0L
        android.socket!!.frames.trySend("""{"t":"call_alive"}""")
        runCurrent()
        log.clear()
        android.now = VoiceCall.Constants.IDLE_TIMEOUT_MS.toLong() - 1
        watch.body()
        assertEquals(VoiceCall.Caller.Stage.Paired, driver.caller.stage)
        android.now = VoiceCall.Constants.IDLE_TIMEOUT_MS.toLong()
        watch.body()
        assertEquals(VoiceCall.Caller.Stage.Idle, driver.caller.stage)
        assertEquals(
            listOf("""sendLast {"t":"call_end","reason":"dropped"}""", "cancel every 1000", "release", "toast The call dropped"),
            log,
        )
    }

    /** The capture thread's frames: as they are on a clear call, nothing before the dial and after the end. */
    @Test
    fun theMicrophoneSendsOnlyOnTheCallsSocket() = runTest {
        val log = mutableListOf<String>()
        val android = FakeAndroid(log)
        val driver = VoiceCallDriver(android, backgroundScope)
        driver.uplink(byteArrayOf(1, 0, 0, 9))
        assertEquals(emptyList<String>(), log)
        answered(driver, android)
        log.clear()
        driver.uplink(byteArrayOf(1, 0, 0, 9))
        assertEquals(listOf("sendBinary"), log)
        driver.end()
        log.clear()
        driver.uplink(byteArrayOf(1, 0, 0, 9))
        assertEquals(emptyList<String>(), log)
    }

    /** A clear call up to the pickup. */
    private suspend fun TestScope.answered(driver: VoiceCallDriver, android: FakeAndroid) {
        driver.start("agent-1", "mint", "en", false)
        android.answer!!.invoke(true, null)
        runCurrent()
        android.ticket!!.complete(StartCallResult(callId = "call-1", ticket = "t", url = RELAY))
        runCurrent()
        for (frame in listOf("""{"t":"call_ready"}""", """{"t":"call_event","event":{"t":"rang"}}""", """{"t":"call_event","event":{"t":"ready"}}""")) {
            android.socket!!.frames.trySend(frame)
        }
        runCurrent()
        assertEquals(VoiceCall.Caller.Stage.Paired, driver.caller.stage)
        assertEquals(VoiceCall.Phase.Listening, driver.caller.call.phase)
    }

    private companion object {
        const val RELAY = "wss://relay.test/call?ticket=t"
    }
}
