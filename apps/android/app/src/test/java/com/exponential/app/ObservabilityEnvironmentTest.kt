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

    // #ERR-22: the Play pre-launch robot of 2026-11-13 19:06 UTC (GlitchTip
    // event 01a1d64e…, exponential-android@0.14.137+220): brand OnePlus, model
    // "OnePlus8Pro", Build.ID QKR1.191246.002, Android 11, 2 CPUs, 288×448 px at
    // density 0.6625, no `firebase.test.lab` setting. It reported as prod.
    private val robot = Observability.DeviceProbe(testLabSetting = null, model = "OnePlus8Pro", widthPixels = 288, heightPixels = 448)

    @Test
    fun `the OnePlus8Pro pre-launch robot without the Test Lab setting is detected`() {
        assertTrue(Observability.isPrelaunchDevice(robot))
        assertEquals(Observability.PRELAUNCH_ENVIRONMENT, Observability.environmentFor("prod", Observability.isPrelaunchDevice(robot)))
    }

    @Test
    fun `each telltale alone marks a robot`() {
        val phone = Observability.DeviceProbe(testLabSetting = null, model = "IN2023", widthPixels = 1440, heightPixels = 3168)
        assertFalse(Observability.isPrelaunchDevice(phone))
        assertTrue(Observability.isPrelaunchDevice(phone.copy(testLabSetting = "true")))
        assertTrue(Observability.isPrelaunchDevice(phone.copy(model = "OnePlus8Pro")))
        assertTrue(Observability.isPrelaunchDevice(phone.copy(widthPixels = 288, heightPixels = 448)))
        assertTrue(Observability.isPrelaunchDevice(phone.copy(widthPixels = 448, heightPixels = 288)))
        assertFalse(Observability.isPrelaunchDevice(phone.copy(testLabSetting = "false")))
    }

    @Test
    fun `only the OkHttp 5xx event counts as an HTTP client error`() {
        fun event(type: String) = SentryEvent().apply { exceptions = listOf(SentryException().apply { this.type = type }) }
        assertTrue(Observability.isHttpClientError(event("SentryHttpClientException")))
        assertFalse(Observability.isHttpClientError(event("OutOfMemoryError")))
        assertFalse(Observability.isHttpClientError(SentryEvent()))
    }
}
