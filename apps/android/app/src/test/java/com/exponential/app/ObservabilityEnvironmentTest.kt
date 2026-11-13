package com.exponential.app

import io.sentry.SentryEvent
import io.sentry.protocol.SentryException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Play pre-launch report devices (Firebase Test Lab) never report as prod. */
class ObservabilityEnvironmentTest {
    @Test
    fun `a Test Lab device reports as prelaunch whatever the server asks`() {
        assertEquals(Observability.PRELAUNCH_ENVIRONMENT, Observability.environmentFor("prod", testLab = true))
        assertEquals(Observability.PRELAUNCH_ENVIRONMENT, Observability.environmentFor(null, testLab = true))
    }

    @Test
    fun `any other device keeps the server's environment`() {
        assertEquals("prod", Observability.environmentFor("prod", testLab = false))
        assertEquals("staging", Observability.environmentFor("staging", testLab = false))
    }

    @Test
    fun `only the OkHttp 5xx event counts as an HTTP client error`() {
        fun event(type: String) = SentryEvent().apply { exceptions = listOf(SentryException().apply { this.type = type }) }
        assertTrue(Observability.isHttpClientError(event("SentryHttpClientException")))
        assertFalse(Observability.isHttpClientError(event("OutOfMemoryError")))
        assertFalse(Observability.isHttpClientError(SentryEvent()))
    }
}
