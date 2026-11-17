package com.exponential.app.data.call

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** #EXP-1940: Cancel during the dial closes the socket that connects afterwards. */
class VoiceCallDialTest {
    @Test
    fun cancelDuringTheDialClosesTheSocketThatConnectsLater() = runBlocking {
        // Ktor finishes the handshake on the client's coroutine, not the caller's.
        val handshake = CompletableDeferred<String>()
        var current = true
        val closed = mutableListOf<String>()
        var dialled: String? = "unset"
        val job = launch(start = CoroutineStart.UNDISPATCHED) {
            dialled = dialCall({ handshake.await() }, { current }) { closed += it }
        }
        // Cancel: finish(run) clears `current` and cancels the run's job.
        current = false
        job.cancel()
        handshake.complete("socket")
        job.join()
        assertEquals(listOf("socket"), closed)
        assertEquals(null, dialled)
    }

    @Test
    fun aDialTheCallStillWantsKeepsItsSocket() = runBlocking {
        val closed = mutableListOf<String>()
        assertEquals("socket", dialCall({ "socket" }, { true }) { closed += it })
        assertEquals(emptyList<String>(), closed)
    }
}
