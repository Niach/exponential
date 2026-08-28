package com.exponential.app.data.electric

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-656: when coming back to the foreground has to drop the pooled
 * connections first.
 *
 * OkHttp does not health-check a pooled connection before reusing it for a GET,
 * so 19 resumed shape polls happily ride a socket the radio killed while the
 * phone slept — that is the "stale for 5-10s after opening the app" report.
 * Evicting is only worth its 19 fresh handshakes when we were actually away
 * long enough for that to have happened.
 */
class SyncForegroundPolicyTest {

    @Test
    fun `a short trip keeps its warm connection`() {
        // A share sheet, the photo picker, a permission dialog, a rotation.
        assertFalse(shouldDropPooledConnections(0L))
        assertFalse(shouldDropPooledConnections(1_500L))
        assertFalse(shouldDropPooledConnections(CONNECTION_STALE_AFTER_BACKGROUND_MS - 1))
    }

    @Test
    fun `at and past the window the pool is suspect`() {
        assertTrue(shouldDropPooledConnections(CONNECTION_STALE_AFTER_BACKGROUND_MS))
        assertTrue(shouldDropPooledConnections(5 * 60_000L))
        // Deep sleep is measured on elapsedRealtime, so an overnight trip is
        // an ordinary large number here rather than a clock that stood still.
        assertTrue(shouldDropPooledConnections(8 * 60 * 60_000L))
    }

    @Test
    fun `a cold launch has never backgrounded and evicts nothing`() {
        assertFalse(shouldDropPooledConnections(null))
    }
}
