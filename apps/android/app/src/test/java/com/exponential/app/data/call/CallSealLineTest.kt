package com.exponential.app.data.call

import at.exponential.net.ffi.EnetCallFinish
import at.exponential.net.ffi.EnetCallHandshake
import at.exponential.net.ffi.EnetCallRole
import at.exponential.net.ffi.EnetCallSeal
import at.exponential.net.ffi.callSealPad
import com.exponential.app.domain.VoiceCall
import java.util.Base64
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1941: the phone's sealed call through its CAPTURE PATH ([uplinkFrame],
 * what the capture thread's `onUplink` runs) against a scripted host built on
 * the binding. The microphone leaves only through a `CallSeal`, which exists
 * only after `finish` opened the host's tag.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class CallSealLineTest {
    private val root = "11111111-1111-4111-8111-111111111111"
    private val other = "44444444-4444-4444-8444-444444444444"
    private val callId = "22222222-2222-4222-8222-222222222222"
    private val agentId = "33333333-3333-4333-8333-333333333333"
    private val deviceId = "desk-7f3a"
    private val secret = ByteArray(32) { (it + 1).toByte() }
    private val packet = byteArrayOf(0xf8.toByte(), 1, 2)

    private class Keys(var targets: MutableList<CallKeyTarget>, val secret: ByteArray?) : CallKeySource {
        var asked = 0
        var secrets = 0
        override suspend fun ready() = true
        override suspend fun target(agentId: String): CallKeyTarget {
            asked += 1
            return if (targets.size > 1) targets.removeAt(0) else targets[0]
        }
        override suspend fun secret(rootId: String, epoch: Int): ByteArray? {
            secrets += 1
            return secret?.copyOf()
        }
        override fun highestEpoch(rootId: String): Int? = null
    }

    private inner class Call(scope: TestScope, val keys: Keys, target: CallKeyTarget) {
        val sent = mutableListOf<ByteArray>()
        val ends = mutableListOf<String>()
        val fails = mutableListOf<String>()
        var host: EnetCallSeal? = null
        val line = CallSealLine(callId, agentId, deviceId, keys, target, scope.backgroundScope, sent::add, ends::add, fails::add)
        var seq = 0

        /** One microphone packet off the capture thread; what leaves the socket, if anything. */
        fun capture(bytes: ByteArray = packet, muted: Boolean = false): ByteArray? =
            uplinkFrame(line, VoiceCall.Wire.frame(VoiceCall.Wire.KIND_UP, seq++, bytes), muted)?.also(sent::add)

        fun audioFrames() = sent.filter { it[1].toInt() == VoiceCall.Seal.TYPE_AUDIO }

        /** The host's hello for the caller's (the first frame sent), as the `hello` event. */
        fun answer(hostRoot: String = root, epoch: Int = 3, tamper: (ByteArray) -> ByteArray = { it }) {
            val callerHello = sent.first().copyOfRange(2, 2 + VoiceCall.Seal.CALLER_HELLO_BYTES)
            val seal = EnetCallHandshake(EnetCallRole.HOST, hostRoot, epoch.toUInt()).finish(
                EnetCallFinish(secret, callId, agentId, deviceId, callerHello, hostRoot, epoch.toUInt(), callSealPad()),
            )
            host = seal
            val hello = tamper(seal.hostHello()!!)
            line.event("hello", Base64.getUrlEncoder().withoutPadding().encodeToString(hello))
        }
    }

    @Test
    fun theWholeCallSealsTheMicrophoneInOneSizeWithMuteInside() = runTest {
        val call = Call(this, Keys(mutableListOf(CallKeyTarget(root, 3)), secret), CallKeyTarget(root, 3))
        // Before call_ready and before the host's proof nothing of the microphone leaves.
        assertNull(call.capture())
        call.line.ready()
        assertEquals(listOf(56, 1, 1), call.sent.single().let { listOf(it.size, it[0].toInt(), it[1].toInt()) })
        advanceTimeBy(VoiceCall.Seal.HELLO_EVERY_MS * 2 + 1)
        assertEquals(3, call.sent.size)
        assertNull(call.capture())
        call.answer()
        runCurrent()
        // Its answer agreed: never asked again, one secret.
        assertEquals(0 to 1, call.keys.asked to call.keys.secrets)

        call.capture()
        call.capture(byteArrayOf(0xf8.toByte(), 3), muted = true)
        call.capture(ByteArray(callSealPad().audioUp.toInt() + 1))
        val frames = call.audioFrames()
        assertEquals(List(3) { 27 + callSealPad().audioUp.toInt() }, frames.map { it.size })
        val heard = frames.map { call.host!!.openAudio(it) }
        assertEquals(
            listOf(listOf(2, 0, 3, 0), listOf(3, 1, 2, 0), listOf(4, 0, 0, 1)),
            heard.map { listOf(it.seq.toInt(), it.flags.toInt(), it.packet.size, it.missing.toInt()) },
        )
        assertArrayEquals(packet, heard[0].packet)
        // The hello stopped once the host's was taken.
        advanceTimeBy(VoiceCall.Seal.HELLO_EVERY_MS * 4)
        assertEquals(3 + 3, call.sent.size)

        val caption = """{"t":"call_event","event":{"t":"said","text":"hello"}}"""
        assertEquals(caption, call.line.event("sealed", call.host!!.sealControl(caption))?.trimEnd())
        val down = call.line.frame(call.host!!.sealAudio(0u, 0u, byteArrayOf(9, 9)))
        assertArrayEquals(byteArrayOf(9, 9), down!!.packet)
        assertEquals(emptyList<String>() to emptyList<String>(), call.ends to call.fails)
    }

    @Test
    fun aHostHelloWhoseTagDoesNotOpenLetsNoFrameLeave() = runTest {
        val call = Call(this, Keys(mutableListOf(CallKeyTarget(root, 3)), secret), CallKeyTarget(root, 3))
        call.line.ready()
        call.answer { hello -> hello.copyOf().also { it[69] = (it[69].toInt() xor 1).toByte() } }
        runCurrent()
        assertNull(call.capture())
        assertEquals(emptyList<ByteArray>(), call.audioFrames())
        assertEquals(listOf(VoiceCall.Seal.END_SEAL) to listOf("dropped"), call.ends to call.fails)
    }

    @Test
    fun aHostHelloNamingAnotherRootThisKeyringHoldsLetsNoFrameLeave() = runTest {
        val call = Call(this, Keys(mutableListOf(CallKeyTarget(root, 3)), secret), CallKeyTarget(root, 3))
        call.line.ready()
        call.answer(hostRoot = other)
        runCurrent()
        assertNull(call.capture())
        assertEquals(emptyList<ByteArray>(), call.audioFrames())
        // Asked the server again, once; never the keyring for the hello's root.
        assertEquals(1 to 0, call.keys.asked to call.keys.secrets)
        assertEquals(listOf(VoiceCall.Seal.END_NO_KEY) to listOf("dropped"), call.ends to call.fails)
    }

    @Test
    fun aFirstCallSendsZerosAndTakesTheRootTheServerNamesAgain() = runTest {
        val keys = Keys(mutableListOf(CallKeyTarget(root, 3)), secret)
        val call = Call(this, keys, CallKeyTarget(null, null))
        call.line.ready()
        assertArrayEquals(ByteArray(20), call.sent.first().copyOfRange(4, 24))
        call.answer()
        runCurrent()
        assertNotNull(call.capture())
        assertArrayEquals(packet, call.host!!.openAudio(call.audioFrames().single()).packet)
        assertEquals(1, keys.asked)
    }

    @Test
    fun aRotationStillDueAfterAskingAgainFailsTheCall() = runTest {
        val due = CallKeyTarget(root, 3, rotationDue = true)
        val call = Call(this, Keys(mutableListOf(due), secret), due)
        call.line.ready()
        call.answer()
        runCurrent()
        assertNull(call.capture())
        assertEquals(1 to 0, call.keys.asked to call.keys.secrets)
        assertEquals(listOf(VoiceCall.Seal.END_NO_KEY) to listOf("dropped"), call.ends to call.fails)
    }

    @Test
    fun noHelloWithinHandshakeMsEndsNoKey() = runTest {
        val call = Call(this, Keys(mutableListOf(CallKeyTarget(root, 3)), secret), CallKeyTarget(root, 3))
        call.line.ready()
        call.line.ready()
        advanceTimeBy(VoiceCall.Seal.HANDSHAKE_MS + 1)
        assertNull(call.capture())
        assertEquals(listOf(VoiceCall.Seal.END_NO_KEY) to listOf("dropped"), call.ends to call.fails)
    }
}
