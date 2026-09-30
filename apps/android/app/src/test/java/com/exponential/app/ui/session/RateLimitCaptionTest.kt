package com.exponential.app.ui.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** EXP-784/818: the rate-limit banner's message + relative countdown pair. */
class RateLimitCaptionTest {
    // 2023-11-14T22:13:20Z
    private val reset = 1_700_000_000_000L
    private val minute = 60_000L

    @Test
    fun messageAndRelativeCountdown() {
        val caption = rateLimitCaption(" 5-hour limit reached ", reset, reset - (2 * 60 + 57) * minute - 30_000L)
        assertEquals("5-hour limit reached", caption.message)
        assertEquals("resets in 2h 57m", caption.countdown)
        assertEquals(
            "resets in 3d 14h",
            rateLimitCaption("x", reset, reset - (3 * 24 + 14) * 60 * minute).countdown,
        )
        assertEquals("resets in 45m", rateLimitCaption("x", reset, reset - 45 * minute).countdown)
        assertEquals("resets soon", rateLimitCaption("x", reset, reset - 20_000L).countdown)
    }

    @Test
    fun fallbackWhenTheAgentNamedNothing() {
        val bare = rateLimitCaption(null, null, reset)
        assertEquals("Rate limit reached", bare.message)
        assertNull(bare.countdown)
        assertEquals(RateLimitCaption("Rate limit reached", null), rateLimitCaption("  ", 0L, reset))
        assertEquals("limited", rateLimitCaption(" limited ", null, reset).message)
    }

    @Test
    fun accessibilityLabelReadsBothSpaceJoined() {
        assertEquals(
            "Weekly limit reached resets in 45m",
            rateLimitCaption("Weekly limit reached", reset, reset - 45 * minute).accessibilityLabel,
        )
        assertEquals("Rate limit reached", rateLimitCaption(null, null, reset).accessibilityLabel)
    }
}
