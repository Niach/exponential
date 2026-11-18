package com.exponential.app.data.call

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-1942: the microphone service is held from the tap, before the call rings or dials. */
class VoiceCallStartTest {
    @Test
    fun theServiceIsStartedBeforeTheRingAndTheDial() {
        val order = mutableListOf<String>()
        val begun = beginCall(
            hold = { order += "service"; true },
            ring = { order += "ring" },
            dial = { order += "dial" },
        )
        assertTrue(begun)
        assertEquals(listOf("service", "ring", "dial"), order)
    }

    @Test
    fun aStartAndroidRefusesNeitherRingsNorDials() {
        val order = mutableListOf<String>()
        val begun = beginCall(
            hold = { order += "service"; false },
            ring = { order += "ring" },
            dial = { order += "dial" },
        )
        // The caller fails the call `dropped` ("The call dropped").
        assertFalse(begun)
        assertEquals(listOf("service"), order)
    }
}
