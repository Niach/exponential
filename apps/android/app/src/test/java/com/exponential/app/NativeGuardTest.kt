package com.exponential.app

import kotlinx.coroutines.CancellationException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** The Engine's `.so` failing to load (0.14.125's crash after 30 s) degrades the outbox, never kills the process. */
class NativeGuardTest {
    private val reports = mutableListOf<Pair<String, Throwable>>()
    private val guard = NativeGuard("Engine") { message, error -> reports += message to error }

    @Test
    fun aLoadErrorTurnsTheLibraryOffAndIsReportedOnce() {
        val missing = UnsatisfiedLinkError("dlopen failed: library \"libeengine_ffi.so\" not found")
        assertNull(guard.call("flush") { throw missing })
        assertFalse(guard.available)
        assertSame(missing, guard.off)

        var touched = false
        assertNull(guard.call("flush") { touched = true })
        assertFalse("an off library is never called again", touched)
        assertEquals(1, reports.size)
        assertSame(missing, reports.single().second)
    }

    @Test
    fun aFailedUniffiInitIsALoadError() {
        assertNull(guard.call("store") { throw NoClassDefFoundError("Could not initialize class at.exponential.engine.ffi.UniffiLib") })
        assertFalse(guard.available)
    }

    @Test
    fun anErrorOnARetryIsReportedAndTheNextCallTriesAgain() {
        assertNull(guard.call("flush") { throw AssertionError("retry blew up") })
        assertTrue(guard.available)
        assertEquals(1, reports.size)
        assertEquals("landed", guard.call("flush") { "landed" })
    }

    @Test
    fun anExceptionIsLoggedOnly() {
        assertNull(guard.call("flush") { throw IllegalStateException("database closed") })
        assertTrue(guard.available)
        assertTrue(reports.isEmpty())
    }

    @Test
    fun cancellationStillPropagates() {
        try {
            guard.call("flush") { throw CancellationException("left") }
            fail("cancellation swallowed")
        } catch (_: CancellationException) {
        }
        assertTrue(guard.available)
    }
}
