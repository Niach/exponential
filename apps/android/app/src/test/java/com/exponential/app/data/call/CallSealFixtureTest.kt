package com.exponential.app.data.call

import at.exponential.net.ffi.EnetCallFinish
import at.exponential.net.ffi.EnetCallHandshake
import at.exponential.net.ffi.EnetCallRole
import at.exponential.net.ffi.EnetCallSeal
import at.exponential.net.ffi.EnetException
import at.exponential.net.ffi.callSealPad
import at.exponential.net.ffi.callSealSelftest
import com.exponential.app.domain.etalkFixtureJson
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1941: packages/etalk/fixtures/voice-call.json `seal` through the UniFFI
 * binding (the host build of exponential-net-ffi, loaded by JNA from
 * `jna.library.path`). No fixed eph secret crosses the binding, so the
 * vectors replay inside Rust (`callSealSelftest`); one live round trip goes
 * through the two objects. The same replay runs in TS, Rust and Swift.
 */
class CallSealFixtureTest {
    private val root = "11111111-1111-4111-8111-111111111111"

    private fun finish(peerHello: ByteArray, rootId: String = root) = EnetCallFinish(
        secret = ByteArray(32) { 7 },
        callId = "22222222-2222-4222-8222-222222222222",
        agentId = "33333333-3333-4333-8333-333333333333",
        deviceId = "desk-7f3a",
        peerHello = peerHello,
        rootId = rootId,
        epoch = 3u,
        pad = callSealPad(),
    )

    /** A caller that knew no root, and the host that answered its hello. */
    private fun call(): Triple<EnetCallHandshake, EnetCallSeal, ByteArray> {
        val caller = EnetCallHandshake(EnetCallRole.CALLER, null, 0u)
        val host = EnetCallHandshake(EnetCallRole.HOST, root, 3u).finish(finish(caller.hello()))
        return Triple(caller, host, host.hostHello()!!)
    }

    private fun reason(run: () -> Unit): String = try {
        run()
        "accepted"
    } catch (e: EnetException.Crypto) {
        e.reason
    }

    @Test
    fun theSelftestReplaysTheFixtureAndNamesWhatFails() {
        val fixture = etalkFixtureJson("voice-call.json")
        callSealSelftest(fixture)
        val failed = reason { callSealSelftest(fixture.replace("021df2d28ad2384e35f042fbb30b425d", "031df2d28ad2384e35f042fbb30b425d")) }
        assertTrue(failed, failed.contains("hostHelloTag"))
    }

    @Test
    fun aLiveRoundTripThroughTheTwoObjects() {
        val (callerSide, host, hostHello) = call()
        assertEquals(54 to 70, callerSide.hello().size to hostHello.size)
        // A host seals nothing before a caller frame opened.
        assertEquals("call seal: unproven", reason { host.sealAudio(0u, 0u, byteArrayOf(1, 2, 3)) })
        assertEquals("call seal: unproven", reason { host.sealControl("{}") })
        val caller = callerSide.finish(finish(hostHello))
        assertEquals("call seal: finished", reason { callerSide.finish(finish(hostHello)) })

        val packet = byteArrayOf(0xf8.toByte(), 0xff.toByte(), 0xfe.toByte())
        val up = caller.sealAudio(9u, 1u, packet)
        assertEquals(27 + callSealPad().audioUp.toInt(), up.size)
        val opened = host.openAudio(up)
        assertArrayEquals(packet, opened.packet)
        assertEquals(listOf(9, 1, 0), listOf(opened.seq.toInt(), opened.flags.toInt(), opened.missing.toInt()))
        val caption = """{"t":"call_event","event":{"t":"said","text":"hello"}}"""
        assertEquals(caption, caller.openControl(host.sealControl(caption)).trimEnd())
        // A packet above the step is refused; the empty frame in its place is one missing.
        assertEquals("call seal: too_large", reason { caller.sealAudio(10u, 0u, ByteArray(121)) })
        assertEquals(1, host.openAudio(caller.sealAudio(10u, 0u, ByteArray(0))).missing.toInt())
    }

    @Test
    fun aHostHelloWithAFlippedTagBitIsRefusedForGood() {
        val (caller, _, hostHello) = call()
        val flipped = hostHello.copyOf().also { it[69] = (it[69].toInt() xor 1).toByte() }
        assertEquals("call seal: tag", reason { caller.finish(finish(flipped)) })
        assertEquals("call seal: finished", reason { caller.finish(finish(hostHello)) })
        val (other, _, otherHello) = call()
        assertEquals("call seal: root", reason { other.finish(finish(otherHello, rootId = "11111111-1111-4111-8111-111111111112")) })
    }
}
