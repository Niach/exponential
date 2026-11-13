package com.exponential.app

import io.sentry.DateUtils
import io.sentry.Hint
import io.sentry.RequestDetails
import io.sentry.SentryEnvelope
import io.sentry.SentryItemType
import io.sentry.SentryLevel
import io.sentry.SentryOptions
import io.sentry.clientreport.DiscardReason
import io.sentry.hints.Cached
import io.sentry.hints.DiskFlushNotification
import io.sentry.hints.Enqueable
import io.sentry.hints.Retryable
import io.sentry.hints.SubmissionResult
import io.sentry.protocol.SentryId
import io.sentry.transport.ITransport
import io.sentry.transport.RateLimiter
import io.sentry.util.HintUtils
import java.io.IOException
import java.net.HttpURLConnection
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.zip.GZIPOutputStream

/**
 * #ERR-17: when an envelope sent through the tunnel is kept or deleted.
 * sentry-java's own transport deletes it on ANY answer of 400 or more, so a
 * crash sent while the tunnel (or anything in between) answered 5xx or 429
 * was gone. Same table as iOS (`Monitoring.TunnelRetry`).
 */
object TunnelRetry {
    enum class Outcome { Sent, Retry, Drop }

    const val BASE_DELAY_MS = 5_000L
    const val MAX_DELAY_MS = 10 * 60_000L
    const val MAX_RETRY_AFTER_MS = 60 * 60_000L

    /** null = no answer at all (offline, timeout). Other 4xx: resending the same bytes cannot help. */
    fun outcome(status: Int?): Outcome = when {
        status == null -> Outcome.Retry
        status in 200..299 -> Outcome.Sent
        status == 408 || status == 429 || status in 500..599 -> Outcome.Retry
        else -> Outcome.Drop
    }

    /** The wait after the `failures`-th failure in a row: 5 s doubling to 10 min, never under the server's ask (at most an hour). */
    fun delayMs(failures: Int, retryAfterMs: Long?): Long {
        val doubled = BASE_DELAY_MS shl (failures - 1).coerceIn(0, 7)
        return maxOf(minOf(doubled, MAX_DELAY_MS), minOf(retryAfterMs ?: 0L, MAX_RETRY_AFTER_MS))
    }

    /** `Retry-After: 120` (seconds) and `X-Sentry-Rate-Limits: 60:error:key, 2700::organization`: the longest ask. */
    fun retryAfterMs(retryAfter: String?, rateLimits: String?): Long? {
        val seconds = listOfNotNull(retryAfter?.trim()?.toLongOrNull()) +
            rateLimits.orEmpty().split(',').mapNotNull { it.trim().substringBefore(':').toLongOrNull() }
        return seconds.maxOrNull()?.takeIf { it > 0 }?.times(1000)
    }

    /** Only error and crash events (ANRs and native crashes included) wait for a retry; traces, logs and sessions are sampled telemetry. */
    fun keeps(envelope: SentryEnvelope): Boolean = envelope.items.any { it.header.type == SentryItemType.Event }
}

/** One POST through the tunnel: the status (null never: no answer throws) and how long the server asked us to wait. */
data class TunnelResponse(val status: Int, val retryAfterMs: Long? = null)

/** The transport's one worker thread (a fake in tests). */
interface TunnelScheduler {
    fun execute(task: () -> Unit): Boolean
    fun schedule(delayMs: Long, task: () -> Unit)
    fun awaitIdle(timeoutMs: Long)
    fun shutdown(timeoutMs: Long)
}

/**
 * #ERR-17: Sentry's transport for `<server>/api/monitoring`. An envelope is
 * written to the SDK's disk cache before any network work (a crash waits for
 * that write, not for the upload). On a transient failure (no answer, 408,
 * 429, 5xx) an error or crash envelope stays on disk and is retried with
 * backoff in this process, and from disk at the next launch. 2xx and the
 * other 4xx delete it.
 */
class TunnelTransport internal constructor(
    private val options: SentryOptions,
    private val post: (SentryEnvelope) -> TunnelResponse,
    private val scheduler: TunnelScheduler,
    private val now: () -> Long,
) : ITransport {
    constructor(options: SentryOptions, details: RequestDetails) :
        this(options, TunnelPost(options, details)::send, ExecutorTunnelScheduler(options), System::currentTimeMillis)

    // Kept for the SDK's API only: a 429 backs off here instead of dropping.
    private val rateLimiter = RateLimiter(options)
    private val queued = AtomicInteger()

    // Worker-thread state, except the backoff deadline (isHealthy reads it).
    private val pending = ArrayDeque<SentryEnvelope>()
    private val pendingIds = HashSet<SentryId>()
    private var failures = 0
    private var retryScheduled = false
    @Volatile private var nextAttemptAt = 0L

    override fun send(envelope: SentryEnvelope, hint: Hint) {
        val accepted = queued.incrementAndGet() <= options.maxQueueSize &&
            scheduler.execute {
                try {
                    deliver(envelope, hint)
                } finally {
                    queued.decrementAndGet()
                }
            }
        if (!accepted) {
            queued.decrementAndGet()
            overflow(envelope, hint)
            return
        }
        HintUtils.runIfHasType(hint, Enqueable::class.java) { it.markEnqueued() }
    }

    override fun flush(timeoutMillis: Long) = scheduler.awaitIdle(timeoutMillis)

    override fun getRateLimiter(): RateLimiter = rateLimiter

    override fun isHealthy(): Boolean = !backingOff()

    override fun close() = close(false)

    override fun close(isRestarting: Boolean) {
        rateLimiter.close()
        // Whatever still waits is on disk: the next launch sends it.
        scheduler.shutdown(if (isRestarting) 0 else options.flushTimeoutMillis)
    }

    private fun backingOff() = now() < nextAttemptAt

    /** The queue is full: on disk for the next launch, nothing sent now. */
    private fun overflow(envelope: SentryEnvelope, hint: Hint) {
        val sdkHint = HintUtils.getSentrySdkHint(hint)
        if (sdkHint !is Cached) options.envelopeDiskCache.storeEnvelope(envelope, hint)
        markFlushed(sdkHint, envelope)
        (sdkHint as? Retryable)?.isRetry = true
        (sdkHint as? SubmissionResult)?.setResult(false)
        options.logger.log(SentryLevel.WARNING, "Tunnel queue full, envelope left on disk")
    }

    private fun deliver(envelope: SentryEnvelope, hint: Hint) {
        val sdkHint = HintUtils.getSentrySdkHint(hint)
        val cached = sdkHint is Cached
        // The cache sender found our own copy of one waiting here: leave it.
        if (cached && envelope.header.eventId in pendingIds) {
            (sdkHint as? Retryable)?.isRetry = true
            (sdkHint as? SubmissionResult)?.setResult(false)
            return
        }
        envelope.header.sentAt = null
        if (!cached) options.envelopeDiskCache.storeEnvelope(envelope, hint)
        markFlushed(sdkHint, envelope)

        val outcome = if (backingOff()) TunnelRetry.Outcome.Retry else attempt(envelope)
        when (outcome) {
            TunnelRetry.Outcome.Sent, TunnelRetry.Outcome.Drop ->
                // A cached file is deleted by its sender (no retry marked).
                if (!cached) options.envelopeDiskCache.discard(envelope)
            TunnelRetry.Outcome.Retry ->
                if (TunnelRetry.keeps(envelope)) {
                    // A cached file is replaced by our own copy, so this
                    // process can delete it once it is through.
                    if (cached) options.envelopeDiskCache.storeEnvelope(envelope, Hint())
                    hold(envelope)
                } else {
                    if (!cached) options.envelopeDiskCache.discard(envelope)
                    options.clientReportRecorder.recordLostEnvelope(DiscardReason.NETWORK_ERROR, envelope)
                }
        }
        (sdkHint as? SubmissionResult)?.setResult(outcome == TunnelRetry.Outcome.Sent)
        scheduleRetry()
    }

    private fun markFlushed(sdkHint: Any?, envelope: SentryEnvelope) {
        val flush = sdkHint as? DiskFlushNotification ?: return
        if (flush.isFlushable(envelope.header.eventId)) flush.markFlushed()
    }

    /** One POST; a failure moves the backoff deadline. */
    private fun attempt(envelope: SentryEnvelope): TunnelRetry.Outcome {
        val response = if (options.transportGate.isConnected) {
            val withReport = options.clientReportRecorder.attachReportToEnvelope(envelope)
            withReport.header.sentAt = DateUtils.nanosToDate(options.dateProvider.now().nanoTimestamp())
            try {
                post(withReport)
            } catch (e: IOException) {
                options.logger.log(SentryLevel.WARNING, e, "Tunnel unreachable")
                null
            }
        } else {
            null
        }
        val outcome = TunnelRetry.outcome(response?.status)
        when (outcome) {
            TunnelRetry.Outcome.Sent -> {
                failures = 0
                nextAttemptAt = 0
            }
            TunnelRetry.Outcome.Retry -> {
                failures++
                nextAttemptAt = now() + TunnelRetry.delayMs(failures, response?.retryAfterMs)
                options.logger.log(SentryLevel.WARNING, "Tunnel answered %s, retrying in %d ms", response?.status, nextAttemptAt - now())
            }
            TunnelRetry.Outcome.Drop ->
                options.clientReportRecorder.recordLostEnvelope(DiscardReason.SEND_ERROR, envelope)
        }
        return outcome
    }

    private fun hold(envelope: SentryEnvelope) {
        pending.addLast(envelope)
        pendingIds.add(envelope.header.eventId ?: return)
        // Past the queue size the oldest waits on disk only (the next launch).
        while (pending.size > options.maxQueueSize) pendingIds.remove(pending.removeFirst().header.eventId)
    }

    private fun scheduleRetry() {
        if (retryScheduled || pending.isEmpty()) return
        retryScheduled = true
        scheduler.schedule(maxOf(0L, nextAttemptAt - now())) {
            retryScheduled = false
            retryPending()
        }
    }

    private fun retryPending() {
        while (pending.isNotEmpty() && !backingOff()) {
            val next = pending.first()
            if (attempt(next) == TunnelRetry.Outcome.Retry) break
            pending.removeFirst()
            pendingIds.remove(next.header.eventId)
            options.envelopeDiskCache.discard(next)
        }
        scheduleRetry()
    }
}

/** The POST itself, as sentry-java's HttpConnection sends it (gzip envelope, the SDK's auth header). */
internal class TunnelPost(private val options: SentryOptions, private val details: RequestDetails) {
    fun send(envelope: SentryEnvelope): TunnelResponse {
        val connection = details.url.openConnection() as HttpURLConnection
        try {
            details.headers.forEach { (name, value) -> connection.setRequestProperty(name, value) }
            connection.requestMethod = "POST"
            connection.doOutput = true
            connection.setRequestProperty("Content-Encoding", "gzip")
            connection.setRequestProperty("Content-Type", "application/x-sentry-envelope")
            connection.setRequestProperty("Accept", "application/json")
            connection.setRequestProperty("Connection", "close")
            connection.connectTimeout = options.connectionTimeoutMillis
            connection.readTimeout = options.readTimeoutMillis
            connection.outputStream.use { out -> GZIPOutputStream(out).use { options.serializer.serialize(envelope, it) } }
            val status = connection.responseCode
            return TunnelResponse(
                status,
                TunnelRetry.retryAfterMs(connection.getHeaderField("Retry-After"), connection.getHeaderField("X-Sentry-Rate-Limits")),
            )
        } finally {
            runCatching { (connection.errorStream ?: connection.inputStream)?.close() }
            connection.disconnect()
        }
    }
}

internal class ExecutorTunnelScheduler(private val options: SentryOptions) : TunnelScheduler {
    private val executor = ScheduledThreadPoolExecutor(1) { r -> Thread(r, "SentryTunnelTransport").apply { isDaemon = true } }.apply {
        // close() must not wait out a backoff timer.
        executeExistingDelayedTasksAfterShutdownPolicy = false
    }

    private fun guarded(task: () -> Unit) = Runnable {
        try {
            task()
        } catch (e: Throwable) {
            options.logger.log(SentryLevel.ERROR, e, "Tunnel transport task failed")
        }
    }

    override fun execute(task: () -> Unit): Boolean = try {
        executor.execute(guarded(task))
        true
    } catch (_: RejectedExecutionException) {
        false
    }

    override fun schedule(delayMs: Long, task: () -> Unit) {
        try {
            executor.schedule(guarded(task), delayMs, TimeUnit.MILLISECONDS)
        } catch (_: RejectedExecutionException) {
        }
    }

    override fun awaitIdle(timeoutMs: Long) {
        runCatching { executor.submit {}.get(timeoutMs, TimeUnit.MILLISECONDS) }
    }

    override fun shutdown(timeoutMs: Long) {
        executor.shutdown()
        try {
            if (timeoutMs <= 0 || !executor.awaitTermination(timeoutMs, TimeUnit.MILLISECONDS)) executor.shutdownNow()
        } catch (_: InterruptedException) {
            executor.shutdownNow()
            Thread.currentThread().interrupt()
        }
    }
}
