package com.exponential.app.ui.session

import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-784: the rate-limit banner's one line. */
class RateLimitCaptionTest {
    private val utc = ZoneId.of("UTC")

    @Test
    fun messageAndLocalResetTime() {
        // 2023-11-14T22:13:20Z
        assertEquals(
            "5-hour limit reached · resets 22:13",
            rateLimitCaption("5-hour limit reached", 1_700_000_000_000L, utc),
        )
        assertEquals(
            "5-hour limit reached · resets 23:13",
            rateLimitCaption("5-hour limit reached", 1_700_000_000_000L, ZoneId.of("Europe/Vienna")),
        )
    }

    @Test
    fun fallbackWhenTheAgentNamedNothing() {
        assertEquals("Rate limit reached", rateLimitCaption(null, null, utc))
        assertEquals("Rate limit reached", rateLimitCaption("  ", 0L, utc))
        assertEquals("Rate limit reached · resets 22:13", rateLimitCaption(null, 1_700_000_000_000L, utc))
        assertEquals("limited", rateLimitCaption(" limited ", null, utc))
    }
}
