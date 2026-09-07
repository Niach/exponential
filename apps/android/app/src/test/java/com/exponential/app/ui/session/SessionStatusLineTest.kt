package com.exponential.app.ui.session

import com.exponential.app.domain.AgentPhase
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * FEED-26: the session header's caption. A live run whose feed has been quiet
 * for [STALE_ACTIVITY_AFTER_MS] stops reading as a healthy "Live" — the same
 * rule, and the same strings, on all four clients.
 */
class SessionStatusLineTest {

    @Test
    fun `live run names its device`() {
        assertEquals(
            "Live · macbook",
            sessionStatusLine(AgentPhase.Live, "macbook", awaiting = false, paused = false),
        )
    }

    @Test
    fun `a blocked run outranks a quiet one`() {
        assertEquals(
            "Needs your input · macbook",
            sessionStatusLine(
                AgentPhase.Live,
                "macbook",
                awaiting = true,
                paused = false,
                staleMinutes = 27,
            ),
        )
    }

    @Test
    fun `a quiet live run counts its minutes`() {
        assertEquals(
            "No activity for 27 min · macbook",
            sessionStatusLine(
                AgentPhase.Live,
                "macbook",
                awaiting = false,
                paused = false,
                staleMinutes = 27,
            ),
        )
    }

    @Test
    fun `a quiet run with no device label drops the suffix`() {
        assertEquals(
            "No activity for 10 min",
            sessionStatusLine(
                AgentPhase.Live,
                null,
                awaiting = false,
                paused = false,
                staleMinutes = 10,
            ),
        )
    }

    @Test
    fun `a paused run stays paused`() {
        assertEquals(
            "Paused · macbook",
            sessionStatusLine(
                AgentPhase.Live,
                "macbook",
                awaiting = false,
                paused = true,
                staleMinutes = 27,
            ),
        )
    }

    @Test
    fun `quiet minutes only start at the threshold`() {
        val now = 1_784_289_600_000L
        assertNull(staleActivityMinutes(null, now))
        assertNull(staleActivityMinutes(now - STALE_ACTIVITY_AFTER_MS + 1L, now))
        assertEquals(10, staleActivityMinutes(now - STALE_ACTIVITY_AFTER_MS, now))
        // Whole minutes only: 27m30s reads as 27.
        assertEquals(27, staleActivityMinutes(now - 27L * 60_000L - 30_000L, now))
    }
}
