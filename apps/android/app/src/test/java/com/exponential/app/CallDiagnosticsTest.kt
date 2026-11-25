package com.exponential.app

import com.exponential.app.domain.VoiceCall
import io.sentry.Sentry
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** EXP-2111: what a failed call's one event holds once the scrubber ran, and nothing while diagnostics are off. */
class CallDiagnosticsTest {
    private val sent = mutableListOf<SentryEvent>()
    private val end = VoiceCall.CallLog.End(
        tag = "4410cd11", by = "here", reason = "dropped", stage = "paired", phase = "listening", answered = true, sealed = true,
        lastedMs = 30_700, upFrames = 1, downFrames = 1, lastDownMs = null,
    )

    private fun sentry() = Sentry.init { options ->
        options.dsn = "https://key@example.invalid/1"
        options.setBeforeSend { event, _ ->
            sent += SentryScrubber.scrub(event)
            null
        }
        options.setBeforeBreadcrumb { breadcrumb, _ -> SentryScrubber.scrub(breadcrumb) }
    }

    @After
    fun close() = Sentry.close()

    @Test
    fun theEventCarriesTheEndAndTheLines() {
        sentry()
        Observability.addCallLine("call 4410cd11: answered after 400 ms")
        Observability.sendCallFailed(end)
        val event = sent.single()
        assertEquals("Voice call ended: dropped", event.message?.formatted)
        assertEquals(SentryLevel.WARNING, event.level)
        assertEquals(listOf("voice-call-end", "dropped"), event.fingerprints)
        assertEquals("4410cd11", event.getTag("call_tag"))
        assertEquals("paired", event.getTag("call_stage"))
        assertEquals("30700", event.getExtra("lastedMs"))
        assertEquals("never", event.getExtra("lastDownMs"))
        assertEquals(listOf("call 4410cd11: answered after 400 ms"), event.breadcrumbs?.map { it.message })
        assertNull(event.user)
    }

    @Test
    fun nothingLeavesWhileDiagnosticsAreOff() {
        sentry()
        // Observability never started: the consent or the server says off.
        Observability.callLine("call 4410cd11: answered after 400 ms")
        Observability.callFailed(end)
        assertEquals(emptyList<SentryEvent>(), sent)
    }
}
