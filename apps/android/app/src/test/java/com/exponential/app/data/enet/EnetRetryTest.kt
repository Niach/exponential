package com.exponential.app.data.enet

import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-1590: a cold start's failed node pass retries within seconds, not a minute. */
class EnetRetryTest {
    @Test
    fun aPassThatWentThroughWaitsForTheRefresh() {
        assertEquals(600_000L, EnetRetry.waitMs(0, 600_000L))
    }

    @Test
    fun theFirstFailureRetriesInASecond() {
        assertEquals(1_000L, EnetRetry.waitMs(1, 600_000L))
    }

    @Test
    fun failuresBackOffThenSettleOnAMinute() {
        assertEquals(listOf(1_000L, 3_000L, 10_000L, 30_000L, 60_000L, 60_000L, 60_000L), (1..7).map { EnetRetry.waitMs(it, 600_000L) })
    }

    @Test
    fun aRegisterTimeoutPlusTheFirstRetryStaysFarUnderTheOld80Seconds() {
        // The old path: a 20 s call timeout, then a flat 60 s retry wait.
        assertEquals(true, 8_000L + EnetRetry.waitMs(1, 600_000L) < 10_000L)
    }
}
