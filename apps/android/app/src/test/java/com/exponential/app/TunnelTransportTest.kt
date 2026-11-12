package com.exponential.app

import io.sentry.Hint
import io.sentry.SentryEnvelope
import io.sentry.SentryEnvelopeItem
import io.sentry.SentryEvent
import io.sentry.SentryOptions
import io.sentry.cache.IEnvelopeCache
import io.sentry.clientreport.ClientReport
import io.sentry.hints.Cached
import io.sentry.hints.DiskFlushNotification
import io.sentry.hints.Retryable
import io.sentry.hints.SubmissionResult
import io.sentry.protocol.SentryId
import io.sentry.util.HintUtils
import java.io.IOException
import java.util.Date
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** #ERR-17: a crash report survives a 5xx/429 from the tunnel. */
class TunnelTransportTest {
    private val retry = TunnelRetry.Outcome.Retry

    @Test
    fun `5xx, 429, 408 and no answer are retried, 2xx sent, other 4xx dropped`() {
        for (status in listOf(null, 408, 429, 500, 502, 503, 504)) assertEquals("$status", retry, TunnelRetry.outcome(status))
        for (status in listOf(200, 202)) assertEquals("$status", TunnelRetry.Outcome.Sent, TunnelRetry.outcome(status))
        for (status in listOf(400, 401, 403, 404, 413)) assertEquals("$status", TunnelRetry.Outcome.Drop, TunnelRetry.outcome(status))
    }

    @Test
    fun `backoff doubles from 5 s to 10 min and honours the server's ask up to an hour`() {
        assertEquals(listOf(5_000L, 10_000L, 20_000L, 40_000L, 600_000L, 600_000L), listOf(1, 2, 3, 4, 8, 30).map { TunnelRetry.delayMs(it, null) })
        assertEquals(120_000L, TunnelRetry.delayMs(1, 120_000L))
        assertEquals(10_000L, TunnelRetry.delayMs(2, 1_000L))
        assertEquals(3_600_000L, TunnelRetry.delayMs(1, 7_200_000L))
        assertEquals(2_700_000L, TunnelRetry.retryAfterMs("60", "60:error:key, 2700::organization"))
        assertEquals(30_000L, TunnelRetry.retryAfterMs("30", null))
        assertNull(TunnelRetry.retryAfterMs("Wed, 21 Oct 2026 07:28:00 GMT", null))
        assertNull(TunnelRetry.retryAfterMs(null, null))
    }

    @Test
    fun `a crash answered 504 stays on disk and goes out on the backed-off retry`() {
        val t = Harness(504, 200)
        val crash = t.event()
        t.transport.send(crash, t.hint())
        assertEquals(listOf(crash), t.cache.stored)
        assertTrue(t.cache.discarded.isEmpty())
        assertEquals(listOf(5_000L), t.scheduler.delays)

        t.clock += 5_000L
        t.scheduler.fire()
        assertEquals(2, t.posts)
        assertEquals(listOf(crash), t.cache.discarded)
    }

    @Test
    fun `a crash is written to disk before the POST`() {
        val t = Harness(200)
        val flush = FlushHint()
        t.onPost = { assertTrue(flush.flushed) }
        t.transport.send(t.event(), HintUtils.createWithTypeCheckHint(flush))
        assertEquals(1, t.posts)
        assertTrue(flush.result!!)
    }

    @Test
    fun `a 400 deletes it, a non-event envelope is not retried`() {
        val t = Harness(400, 503)
        val bad = t.event()
        t.transport.send(bad, t.hint())
        assertEquals(listOf(bad), t.cache.discarded)

        val report = t.clientReport()
        t.transport.send(report, t.hint())
        assertEquals(listOf(bad, report), t.cache.discarded)
        assertTrue(t.scheduler.delays.isEmpty())
    }

    @Test
    fun `while backing off nothing is posted, events wait on disk`() {
        val t = Harness(429, 200)
        t.retryAfterMs = 60_000L
        t.transport.send(t.event(), t.hint())
        val second = t.event()
        t.transport.send(second, t.hint())
        assertEquals(1, t.posts)
        assertFalse(t.transport.isHealthy)
        assertTrue(second in t.cache.stored)
        assertEquals(60_000L, t.scheduler.delays.single())

        t.clock += 60_000L
        t.scheduler.fire()
        assertEquals(3, t.posts)
        assertTrue(t.transport.isHealthy)
        assertEquals(2, t.cache.discarded.size)
    }

    @Test
    fun `a cached crash answered 503 is re-stored as our own copy, its old file let go`() {
        val t = Harness(503)
        val cached = CachedHint()
        val crash = t.event()
        t.transport.send(crash, HintUtils.createWithTypeCheckHint(cached))
        assertEquals(listOf(crash), t.cache.stored)
        assertFalse(cached.retry)
        assertFalse(cached.result!!)

        // The cache sender meets that copy again: it is ours, left alone.
        val again = CachedHint()
        t.transport.send(crash, HintUtils.createWithTypeCheckHint(again))
        assertEquals(1, t.posts)
        assertTrue(again.retry)
    }

    @Test
    fun `a cached envelope sent fine is left to its sender to delete`() {
        val t = Harness(200)
        val cached = CachedHint()
        t.transport.send(t.event(), HintUtils.createWithTypeCheckHint(cached))
        assertTrue(t.cache.stored.isEmpty())
        assertTrue(t.cache.discarded.isEmpty())
        assertFalse(cached.retry)
        assertTrue(cached.result!!)
    }

    @Test
    fun `no answer at all is a retry`() {
        val t = Harness(-1)
        t.transport.send(t.event(), t.hint())
        assertTrue(t.cache.discarded.isEmpty())
        assertEquals(1, t.scheduler.delays.size)
    }

    private class Harness(vararg statuses: Int) {
        val options = SentryOptions()
        val cache = RecordingCache().also { options.envelopeDiskCache = it }
        val scheduler = ManualScheduler()
        var clock = 1_000_000L
        var posts = 0
        var retryAfterMs: Long? = null
        var onPost: () -> Unit = {}
        private val answers = ArrayDeque(statuses.toList())
        val transport = TunnelTransport(
            options,
            post = {
                posts++
                onPost()
                val status = answers.removeFirstOrNull() ?: 200
                if (status < 0) throw IOException("offline")
                TunnelResponse(status, retryAfterMs)
            },
            scheduler = scheduler,
            now = { clock },
        )

        fun hint() = Hint()
        fun event() = SentryEnvelope(SentryId(), null, SentryEnvelopeItem.fromEvent(options.serializer, SentryEvent()))
        fun clientReport() = SentryEnvelope(SentryId(), null, SentryEnvelopeItem.fromClientReport(options.serializer, ClientReport(Date(), emptyList())))
    }

    /** Runs work inline; keeps timers until [fire]. */
    private class ManualScheduler : TunnelScheduler {
        val delays = mutableListOf<Long>()
        private val timers = ArrayDeque<() -> Unit>()

        override fun execute(task: () -> Unit): Boolean {
            task()
            return true
        }

        override fun schedule(delayMs: Long, task: () -> Unit) {
            delays += delayMs
            timers.addLast(task)
        }

        fun fire() = timers.removeFirst()()

        override fun awaitIdle(timeoutMs: Long) {}

        override fun shutdown(timeoutMs: Long) {}
    }

    private class RecordingCache : IEnvelopeCache {
        val stored = mutableListOf<SentryEnvelope>()
        val discarded = mutableListOf<SentryEnvelope>()

        @Deprecated("store is the SDK's old entry point")
        override fun store(envelope: SentryEnvelope, hint: Hint) {
            stored += envelope
        }

        override fun storeEnvelope(envelope: SentryEnvelope, hint: Hint): Boolean {
            stored += envelope
            return true
        }

        override fun discard(envelope: SentryEnvelope) {
            discarded += envelope
        }

        override fun iterator(): MutableIterator<SentryEnvelope> = stored.iterator()
    }

    private class CachedHint : Cached, Retryable, SubmissionResult {
        var retry = false
        var result: Boolean? = null
        override fun isRetry() = retry
        override fun setRetry(retry: Boolean) {
            this.retry = retry
        }
        override fun setResult(success: Boolean) {
            result = success
        }
        override fun isSuccess() = result == true
    }

    private class FlushHint : DiskFlushNotification, SubmissionResult {
        var flushed = false
        var result: Boolean? = null
        override fun markFlushed() {
            flushed = true
        }
        override fun isFlushable(eventId: SentryId?) = true
        override fun setFlushable(eventId: SentryId) {}
        override fun setResult(success: Boolean) {
            result = success
        }
        override fun isSuccess() = result == true
    }
}
