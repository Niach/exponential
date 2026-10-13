package com.exponential.app.domain

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Wave D parity mirror of the web fix M6 (an unobserved first-turn end).
class WaveDParityTest {

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
}
