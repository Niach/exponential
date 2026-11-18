package com.exponential.app.data.call

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-1940, #ERR-11: the microphone service starts, or the call hears that it did not. */
class VoiceCallServiceTest {
    private class Android(var visible: Boolean = true, var microphone: Boolean = true, var refusesStart: Boolean = false) : CallServiceLink.Host {
        val calls = mutableListOf<String>()
        override fun visible() = visible
        override fun microphoneGranted() = microphone
        override fun startService(title: String) {
            if (refusesStart) throw IllegalStateException("app is in background")
            calls += "start $title"
        }

        override fun stopService() {
            calls += "stop"
        }
    }

    @Test
    fun aVisibleAppStartsTheService() {
        val android = Android()
        assertTrue(CallServiceLink(android).start("Call Assistant") {})
        assertEquals(listOf("start Call Assistant"), android.calls)
    }

    @Test
    fun aStartAndroidWouldRefuseIsNeverMadeAndSaysSo() {
        for (android in listOf(Android(visible = false), Android(microphone = false))) {
            assertFalse(CallServiceLink(android).start("Call Assistant") {})
            assertEquals(emptyList<String>(), android.calls)
        }
        // The app left the foreground just now: startService itself throws.
        assertFalse(CallServiceLink(Android(refusesStart = true)).start("Call Assistant") {})
    }

    @Test
    fun aRefusedStartForegroundReachesTheCallOnce() {
        var refused = 0
        val link = CallServiceLink(Android())
        link.start("Call Assistant") { refused++ }
        link.refused()
        link.refused()
        assertEquals(1, refused)
    }

    @Test
    fun aRefusalAfterTheCallEndedReachesNobody() {
        var refused = 0
        val android = Android()
        val link = CallServiceLink(android)
        link.start("Call Assistant") { refused++ }
        // The call ended before onStartCommand ran: a plain stop, no crash contract.
        link.stop()
        assertEquals(listOf("start Call Assistant", "stop"), android.calls)
        link.refused()
        assertEquals(0, refused)
    }

    /** #EXP-1942: the service is held from the tap, so Cancel during the ring is its stop. */
    @Test
    fun aCallCancelledDuringTheRingStopsTheServiceAndTheNextCallStartsItAgain() {
        val android = Android()
        val link = CallServiceLink(android)
        assertTrue(link.start("Call Assistant") {})
        link.stop()
        assertTrue(link.start("Call Assistant") {})
        assertEquals(listOf("start Call Assistant", "stop", "start Call Assistant"), android.calls)
    }

    @Test
    fun aSecondCallStartsNoSecondService() {
        var first = 0
        var second = 0
        val android = Android()
        val link = CallServiceLink(android)
        assertTrue(link.start("Call Assistant") { first++ })
        assertFalse(link.start("Call Other") { second++ })
        assertEquals(listOf("start Call Assistant"), android.calls)
        // The service still belongs to the first call.
        link.refused()
        assertEquals(1 to 0, first to second)
    }

    @Test
    fun aRefusedServiceIsHeldNoLonger() {
        val android = Android()
        val link = CallServiceLink(android)
        link.start("Call Assistant") {}
        link.refused()
        assertTrue(link.start("Call Assistant") {})
    }
}
