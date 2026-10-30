package com.exponential.app.data.call

import java.util.Collections
import java.util.concurrent.CountDownLatch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** #NEW-89: the call's routing goes back only after its audio threads exit. */
class VoiceCallAudioTest {
    @Test
    fun restoresRoutingOnlyAfterTheAudioThreadsExit() {
        val events = Collections.synchronizedList(mutableListOf<String>())
        val started = CountDownLatch(2)
        val threads = listOf("capture", "play").map { name ->
            Thread {
                started.countDown()
                // Still freeing its recorder / track after the interrupt.
                Thread.sleep(40)
                events += "$name exited"
            }.apply { start() }
        }
        started.await()
        VoiceCallAudio.joinThenRestore(threads, timeoutMs = 2_000) { events += "routing restored" }
        assertEquals("routing restored", events.last())
        assertEquals(3, events.size)
    }

    @Test
    fun restoresRoutingWhenNoThreadEverStarted() {
        var restored = false
        VoiceCallAudio.joinThenRestore(emptyList(), timeoutMs = 2_000) { restored = true }
        assertEquals(true, restored)
    }

    @Test
    fun aSingleCallPutsBackTheStateBeforeIt() {
        val handoff = RoutingHandoff<String>()
        val call = Any()
        handoff.claim(call) { "normal" }
        assertEquals("normal", handoff.restore(call) {})
        assertNull(handoff.restore(call) {})
    }

    @Test
    fun aCallStartedBeforeThePreviousRestoredKeepsItsRouting() {
        val handoff = RoutingHandoff<String>()
        val first = Any()
        val second = Any()
        handoff.claim(first) { "normal" }
        // The second call routes while the first still joins its threads:
        // the device reads "in call" now, which must never be put back.
        handoff.claim(second) { "in call" }
        assertNull(handoff.restore(first) {})
        assertEquals("normal", handoff.restore(second) {})
    }

    @Test
    fun aCallClaimingDuringARestoreKeepsTheOriginalMode() {
        val handoff = RoutingHandoff<String>()
        var mode = "normal"
        val first = Any()
        val second = Any()
        handoff.claim(first) { mode }
        mode = "in call"
        var claimer: Thread? = null
        handoff.restore(first) { previous ->
            // The next call claims while the old mode is being written back:
            // it must wait for the write, never save "in call" as previous.
            claimer = Thread { handoff.claim(second) { mode } }.also { it.start() }
            while (claimer!!.state != Thread.State.BLOCKED) Thread.yield()
            mode = previous
        }
        claimer!!.join(2_000)
        mode = "in call"
        handoff.restore(second) { mode = it }
        assertEquals("normal", mode)
    }
}
