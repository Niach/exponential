package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// Wave D parity mirrors of the web fixes: M12 (computerUse rides the start only
// after an explicit flip) and M6 (an unobserved first-turn end has no duration).
class WaveDParityTest {

    @Test
    fun `an untouched computer use toggle sends nothing`() {
        assertNull(ComposerMenu.computerUseWire(pick = null, canToggle = true))
        assertTrue(ComposerMenu.computerUseShown(pick = null, deviceDefault = true))
    }

    @Test
    fun `an explicit flip rides the wire`() {
        assertEquals(false, ComposerMenu.computerUseWire(pick = false, canToggle = true))
        // Any explicit pick rides, even one equal to the device default.
        assertEquals(true, ComposerMenu.computerUseWire(pick = true, canToggle = true))
    }

    @Test
    fun `a device that cannot read the flag never gets it`() {
        assertNull(ComposerMenu.computerUseWire(pick = true, canToggle = false))
    }

    @Test
    fun `the first turn end is known only after an observed started edge`() {
        val start = 1_000L
        val synthetic = SessionTurnEvent.Turn(started = true, at = start)
        // Next fact = an observed `started`: the first turn's end is that edge.
        assertTrue(
            firstTurnEndKnown(listOf(synthetic, SessionTurnEvent.Turn(started = true, at = 5_000L)), start),
        )
        // Next fact = a message cutting in: the end was never observed.
        assertFalse(
            firstTurnEndKnown(listOf(synthetic, SessionTurnEvent.UserMessage(at = 4_000L, text = "hi")), start),
        )
        assertFalse(
            firstTurnEndKnown(listOf(synthetic, SessionTurnEvent.Turn(started = false, at = 3_000L)), start),
        )
        // Nothing after the start, or no synthetic start at all: known.
        assertTrue(firstTurnEndKnown(listOf(synthetic), start))
        assertTrue(firstTurnEndKnown(emptyList(), start))
        assertTrue(firstTurnEndKnown(listOf(SessionTurnEvent.Turn(started = true, at = 2_000L)), start))
    }

    @Test
    fun `an unknown end drops the duration`() {
        val caption = turnRowCaption(
            startedAt = 0L,
            endedAt = 65_000L,
            state = RunRowState.Ended,
            device = "Mac",
            runEndedAt = null,
            nowMs = 70_000L,
            endKnown = false,
        )
        assertEquals("Done on Mac", caption?.text)
        val known = turnRowCaption(0L, 65_000L, RunRowState.Ended, "Mac", null, 70_000L)
        assertTrue(known!!.text.startsWith("Done on Mac · "))
    }
}
