package com.exponential.app.data.electric

import android.os.SystemClock
import com.exponential.app.data.db.ElectricOffsetDao
import com.exponential.app.data.db.ElectricOffsetEntity
import io.ktor.client.HttpClient
import io.ktor.client.plugins.timeout
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsText
import io.ktor.http.HttpStatusCode
import io.ktor.http.isSuccess
import kotlinx.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.coroutines.coroutineContext
import kotlin.math.min

internal const val INITIAL_OFFSET = "-1"
private const val LIVE_TIMEOUT_MS = 60_000L
// Per-request connect budget. The client-wide default is 10s (HttpClientProvider)
// and a per-request `timeout {}` block does NOT inherit-and-override just the
// fields it sets — an unset connectTimeoutMillis here left the 10s default in
// place, which is exactly the "app shows stale state for ~10s, then everything
// updates at once" hang on open (EXP-264): a pooled connection killed by the
// radio going idle blocks the whole poll until the connect budget expires. 5s
// still covers an LTE radio wake while failing fast into the 500ms-backoff
// retry loop. Deliberately per-request: the client-wide value stays 10s for
// ordinary tRPC calls.
private const val CONNECT_TIMEOUT_MS = 5_000L
// A shape that succeeded this recently is already as fresh as a kick could make
// it — its brand-new live long-poll IS the newest state — so kicks inside this
// window are dropped. Anti-herd: an app-foreground kick on a healthy app costs
// zero requests. Also the slack term in SyncManager.refresh's satisfaction test.
internal const val KICK_FRESHNESS_MS = 1_500L
// Per-request HTTP budget for shape polls. MUST exceed the server's live
// long-poll hold window (~20s on prod Electric, up to ~60s per the
// long-poll-canary.md contract) or every idle live poll times out client-side
// and the loop degrades into error/backoff churn. iOS uses liveTimeout + 30s
// (ShapeClient.swift), desktop 90s (sync/client.rs LIVE_READ_TIMEOUT) — same
// figure here. The socket timeout must match: an idle hold sends zero bytes.
private const val REQUEST_TIMEOUT_MS = LIVE_TIMEOUT_MS + 30_000L
// Consecutive schema-class apply errors before a one-shot per-shape reset.
private const val SCHEMA_RESET_THRESHOLD = 3

/** Thrown on HTTP 401/403 so the run loop can report it as an *auth* failure. */
private class ShapeAuthException(message: String) : Exception(message)

/**
 * Thrown on HTTP 426 (client below the server's minimum version, EXP-104). The
 * shared HTTP client's response validator has already latched the app-wide
 * update gate, so the run loop only needs to STOP: the blocking "Update
 * required" screen is up and retrying would just hammer the server.
 */
private class ShapeUpgradeException(message: String) : Exception(message)

/**
 * Thrown when [ShapeClient.kick] interrupts an in-flight poll. Not an error:
 * the run loop re-polls immediately from the saved offset (nothing was applied
 * and no offset was written, so the cancelled request loses nothing).
 */
private class KickException : Exception("kicked")

/**
 * Thrown when the app-lifecycle gate closes during a poll (REV2-38): the
 * process went to the background, so the in-flight long-poll is dropped instead
 * of holding a socket open for the rest of its 90s budget. Not an error — the
 * run loop parks at the top of the loop and re-polls from the saved offset once
 * the app is visible again.
 */
private class PausedException : Exception("paused")

/** Default gate for call sites (and tests) with no lifecycle to observe. */
private val alwaysActive: StateFlow<Boolean> = MutableStateFlow(true)

class ShapeClient<T : Any>(
    private val client: HttpClient,
    private val baseUrlProvider: () -> String?,
    private val tokenProvider: () -> String?,
    private val shapeName: String,
    private val urlPath: String,
    private val valueSerializer: KSerializer<T>,
    private val offsetDao: ElectricOffsetDao,
    private val json: Json,
    private val onMessages: suspend (List<ShapeMessage<T>>) -> Unit,
    // Diagnostics hooks (no-ops by default).
    private val onPhase: (String) -> Unit = {},
    private val onApplied: (Int) -> Unit = {},
    // Reports a failed poll: (authFailure, message, schemaError). authFailure is
    // true for HTTP 401/403; schemaError is true for "no such column/table"
    // class SQLite failures during apply.
    private val onError: (Boolean, String?, Boolean) -> Unit = { _, _, _ -> },
    // Reports a successful poll, so current-health error state can be cleared.
    private val onSuccess: () -> Unit = {},
    // A full-row insert was dropped because its payload failed to decode.
    private val onDecodeDrop: (String) -> Unit = {},
    // An auto-reset of this shape has begun (rows briefly empty until refetch).
    private val onRecovering: () -> Unit = {},
    // Mark this shape for an atomic refetch, so the next poll re-snapshots.
    private val onReset: suspend () -> Unit = {},
    // Monotonic clock (SystemClock.elapsedRealtime in production). Injected so
    // JVM unit tests can drive the kick freshness window — the framework class
    // returns a constant 0 under `unitTests.isReturnDefaultValues`.
    private val nowMs: () -> Long = { SystemClock.elapsedRealtime() },
    // App-lifecycle gate (REV2-38): false once the process has been in the
    // background past SyncManager's grace window. While closed the loop parks
    // instead of long-polling, and a poll already in flight is cancelled — a
    // cached (not yet frozen) process must not keep 15 shape connections per
    // account alive for data nobody can see.
    private val active: StateFlow<Boolean> = alwaysActive,
) {
    private val rawMessageSerializer = kotlinx.serialization.builtins.ListSerializer(
        kotlinx.serialization.serializer<RawMessage>()
    )

    // Wake-ups from [kick]. CONFLATED: several kicks arriving while the loop is
    // busy collapse into one — the loop only ever needs to know that it should
    // poll again now.
    private val kicks = Channel<Unit>(Channel.CONFLATED)

    // Seeded to NOW, not 0: the run loop always polls immediately on start, so
    // a kick inside the freshness window of construction is redundant by
    // definition. Seeding with 0 let the cold-launch ON_START kick cancel the
    // very first snapshot requests it raced (construction-to-first-success is
    // longer than the window on a cold start).
    @Volatile private var lastSuccessAtMs = nowMs()

    /**
     * Ask this shape to poll NOW: interrupts a backoff/pacing wait, and cancels
     * an in-flight long-poll so the request restarts against the current
     * network. Safe to call from any thread (a binder thread from the
     * connectivity callback, the main thread from the lifecycle observer).
     *
     * A shape that succeeded within [KICK_FRESHNESS_MS] is skipped: its live
     * long-poll is already the freshest possible state, so kicking it would
     * only throw away a good connection.
     */
    fun kick() {
        if (nowMs() - lastSuccessAtMs < KICK_FRESHNESS_MS) return
        kicks.trySend(Unit)
    }

    /** Waits [ms], or returns early when kicked. True = it was a kick. */
    private suspend fun delayOrKick(ms: Long): Boolean =
        withTimeoutOrNull(ms) { kicks.receive() } != null

    suspend fun run() {
        var backoffMs = 500L
        var consecutiveSchemaErrors = 0
        var didAutoReset = false
        while (coroutineContext.isActive) {
            // Park while the app is backgrounded (REV2-38). The per-shape
            // offsets in Room make the resume a cheap catch-up poll, not a
            // re-snapshot, so nothing is lost by not polling meanwhile —
            // background awareness is what push notifications are for.
            if (!active.value) {
                active.first { it }
                backoffMs = 500L
                continue
            }
            try {
                val baseUrl = baseUrlProvider()
                val token = tokenProvider()
                if (baseUrl == null || token == null) {
                    delayOrKick(2_000)
                    continue
                }
                val outcome = pollOnce(baseUrl, token)
                if (outcome.completed) {
                    // A refetch-marker write is NOT a completed poll: the data
                    // on disk is still the stale pre-rotation set, so it must
                    // not count as "caught up" (SyncManager.refresh waits on
                    // the stats stamp) nor arm the kick freshness window.
                    lastSuccessAtMs = nowMs()
                    onSuccess()
                }
                consecutiveSchemaErrors = 0
                backoffMs = 500L
                // Pace the loop when a non-live poll made no progress, so a
                // response that never reaches up-to-date can't spin-request.
                if (outcome.pause) delayOrKick(500)
            } catch (cancel: CancellationException) {
                // Only exit for a REAL cancellation of this loop's own job
                // (sign-out / pipeline reconcile). HTTP engines can surface
                // request-level failures as CancellationExceptions too (ktor
                // CIO's engine timeout cancels the call job) — before the
                // HttpTimeout plugin was installed, that silently killed this
                // loop forever and froze sync (EXP-61). Treat any cancellation
                // that arrives while our job is still active as a transient
                // transport error: report, back off, keep polling.
                coroutineContext.ensureActive()
                android.util.Log.w("ShapeClient", "[$shapeName] request cancelled: ${cancel.message}", cancel)
                onError(false, describe(cancel.cause ?: cancel), false)
                consecutiveSchemaErrors = 0
                backoffMs = if (delayOrKick(backoffMs)) 500L else min(backoffMs * 2, 30_000L)
            } catch (kick: KickException) {
                // Not a failure: someone (app foreground, network return, push,
                // pull-to-refresh) asked for fresh data mid-request. Re-poll
                // immediately from the saved offset, no error reported.
                backoffMs = 500L
            } catch (paused: PausedException) {
                // The app went to the background mid-poll: not a failure and
                // not reported. The loop head parks until it comes back.
                backoffMs = 500L
            } catch (upgrade: ShapeUpgradeException) {
                // Client is below the server minimum: the update gate is already
                // latched (via the HTTP response validator) and the blocking
                // screen is up. Exit the loop entirely — nothing this build can
                // sync until the user updates, and retrying only hammers the
                // server. A fresh app version restarts sync from scratch.
                android.util.Log.w("ShapeClient", "[$shapeName] upgrade required: ${upgrade.message}")
                return
            } catch (auth: ShapeAuthException) {
                android.util.Log.w("ShapeClient", "[$shapeName] auth error: ${auth.message}")
                onError(true, auth.message, false)
                consecutiveSchemaErrors = 0
                // Keep backing off (don't hammer) — an auth failure on a
                // requireAuth shape won't fix itself by retrying immediately.
                backoffMs = if (delayOrKick(backoffMs)) 500L else min(backoffMs * 2, 30_000L)
            } catch (error: Throwable) {
                val schema = isSchemaError(error)
                android.util.Log.w("ShapeClient", "[$shapeName] error: ${describe(error)}", error)
                onError(false, describe(error), schema)
                if (schema) {
                    consecutiveSchemaErrors++
                    // A local table that drifted past what tolerant-apply can
                    // absorb: reset this shape once per run so a fresh snapshot
                    // repopulates it instead of the batch refailing forever.
                    if (consecutiveSchemaErrors >= SCHEMA_RESET_THRESHOLD && !didAutoReset) {
                        didAutoReset = true
                        onRecovering()
                        android.util.Log.w("ShapeClient", "[$shapeName] auto-resetting after repeated schema errors")
                        runCatching { onReset() }
                        consecutiveSchemaErrors = 0
                    }
                } else {
                    consecutiveSchemaErrors = 0
                }
                backoffMs = if (delayOrKick(backoffMs)) 500L else min(backoffMs * 2, 30_000L)
            }
        }
    }

    // Some transport exceptions carry no message at all (e.g. a DNS
    // UnresolvedAddressException) — the diagnostics row then read "null".
    // Always fall back to the exception's class name.
    private fun describe(error: Throwable): String =
        error.message ?: error.javaClass.simpleName

    private fun isSchemaError(error: Throwable): Boolean {
        var t: Throwable? = error
        while (t != null) {
            val msg = t.message?.lowercase()
            if (msg != null &&
                ("no such column" in msg || "no such table" in msg || "has no column named" in msg)
            ) {
                return true
            }
            t = t.cause
        }
        return false
    }

    /**
     * One poll's effect on the run loop. [completed] is false when the poll
     * only wrote a refetch marker (409/400, inline must-refetch): the local
     * data is still the stale pre-rotation set, so the loop must not stamp it
     * as a success. [pause] paces no-progress polls (see run()).
     */
    private class PollOutcome(val completed: Boolean, val pause: Boolean)

    private suspend fun pollOnce(baseUrl: String, token: String): PollOutcome {
        // A kick that arrived BEFORE this poll started is satisfied by this
        // poll — draining first keeps it from cancelling the very request it
        // asked for.
        while (kicks.tryReceive().isSuccess) { /* drain */ }

        val saved = offsetDao.get(shapeName)
        // The shape is marked for an atomic refetch (409/400, an inline
        // must-refetch, or a schema auto-reset): re-snapshot from offset=-1 but
        // leave the stale rows in place — they stay visible until the refetch
        // batch replaces them in one transaction.
        val refetching = saved?.needsRefetch == true
        val isSnapshot = saved == null || refetching
        // Only long-poll live once the snapshot completed (up-to-date seen);
        // catch-up polls stay non-live per the Electric protocol. Sending
        // live=true from a mid-snapshot offset is rejected by Electric, and a
        // refetch must re-earn live from scratch.
        val wasLive = !refetching && (saved?.isLive ?: false)
        onPhase(if (saved == null) "initial" else if (wasLive) "live" else "catchup")
        val response: HttpResponse = withTimeoutOrNull(REQUEST_TIMEOUT_MS + 30_000L) {
            // Race the request against a kick. Cancelling mid-request loses
            // nothing: the offset is written only after a successful response
            // and the batch is applied in one db.withTransaction, so an
            // interrupted poll leaves the cursor exactly where it was.
            coroutineScope {
                val kicked = java.util.concurrent.atomic.AtomicBoolean(false)
                val paused = java.util.concurrent.atomic.AtomicBoolean(false)
                val call = async {
                    client.get("$baseUrl$urlPath") {
                        // Long-poll budget — overrides the client-wide 30s
                        // default, which would kill the idle live hold (see
                        // REQUEST_TIMEOUT_MS). connect is set explicitly: this
                        // block replaces the whole per-request timeout config
                        // rather than overriding single fields (EXP-264).
                        timeout {
                            requestTimeoutMillis = REQUEST_TIMEOUT_MS
                            connectTimeoutMillis = CONNECT_TIMEOUT_MS
                            socketTimeoutMillis = REQUEST_TIMEOUT_MS
                        }
                        // Authenticate the shape request so the server scopes data to
                        // this user (not just public teams). Mirrors TrpcClient /
                        // AuthApi / IssueImagesApi. Without this, every shape polled
                        // anonymously and only public rows synced — the root cause of
                        // missing teams/boards and the 401 on team_invites.
                        header("Authorization", "Bearer $token")
                        if (isSnapshot) {
                            parameter("offset", INITIAL_OFFSET)
                            // Electric wants the rotated handle back on a
                            // must-refetch snapshot; a fresh shape has none.
                            if (refetching && saved!!.handle.isNotEmpty()) {
                                parameter("handle", saved.handle)
                            }
                        } else {
                            parameter("offset", saved!!.offset)
                            parameter("handle", saved.handle)
                            if (wasLive) parameter("live", "true")
                        }
                    }
                }
                val watcher = launch {
                    kicks.receive()
                    kicked.set(true)
                    call.cancel()
                }
                // A live hold outlives the ON_STOP that parked us by up to the
                // full 90s budget, so drop the socket the moment the gate
                // closes instead of leaving a connection dangling server-side.
                val pauseWatcher = launch {
                    active.first { !it }
                    paused.set(true)
                    call.cancel()
                }
                try {
                    call.await()
                } catch (cancelled: CancellationException) {
                    // Our own job dying (sign-out, pipeline reconcile) must
                    // still exit the loop.
                    coroutineContext.ensureActive()
                    // Only OUR cancellation is a kick: the HTTP engine also
                    // reports request-level failures as CancellationException
                    // (ktor CIO's engine timeout), and those are real errors
                    // the run loop has to report and back off from (EXP-61).
                    if (paused.get()) throw PausedException()
                    if (!kicked.get()) throw cancelled
                    throw KickException()
                } finally {
                    watcher.cancel()
                    pauseWatcher.cancel()
                }
            }
        } ?: throw IOException("Shape $shapeName request timed out")

        // 409 = Electric must-refetch (stale/rotated handle). 400 = a
        // deterministic definition error — most notably "shape definition and
        // handle do not match" after the server-derived where clause rotated
        // under a persisted handle (membership change). Neither will EVER
        // succeed by retrying the identical request; both recover by dropping
        // the cursor and re-snapshotting from scratch.
        if (response.status == HttpStatusCode.Conflict ||
            response.status == HttpStatusCode.BadRequest
        ) {
            if (response.status == HttpStatusCode.BadRequest) {
                android.util.Log.w(
                    "ShapeClient",
                    "[$shapeName] HTTP 400 — resetting shape: ${response.bodyAsText().take(300)}"
                )
            }
            // Mark, don't wipe: deleting the rows here emptied the local table
            // for as long as the fresh snapshot took to arrive, so every list
            // on screen blanked mid-session. The marker makes the NEXT poll a
            // snapshot whose batch carries the wipe, so DELETE + INSERT land in
            // one transaction and Room's flows never observe an empty table.
            offsetDao.upsert(
                ElectricOffsetEntity(
                    shape = shapeName,
                    handle = response.headers["electric-handle"] ?: "",
                    offset = INITIAL_OFFSET,
                    isLive = false,
                    needsRefetch = true,
                )
            )
            return PollOutcome(completed = false, pause = false)
        }
        // 426 = client below the server's minimum version (EXP-104). Stop the
        // run loop; the shared client's validator already raised the update gate.
        if (response.status.value == 426) {
            throw ShapeUpgradeException("Upgrade required syncing $shapeName (HTTP 426)")
        }
        if (response.status == HttpStatusCode.Unauthorized ||
            response.status == HttpStatusCode.Forbidden
        ) {
            throw ShapeAuthException("Unauthorized syncing $shapeName (HTTP ${response.status.value})")
        }
        if (!response.status.isSuccess()) {
            throw IOException("Shape $shapeName HTTP ${response.status.value}")
        }

        val handle = response.headers["electric-handle"]
        val offset = response.headers["electric-offset"]
        val body = response.bodyAsText()
        val decoded = decodeMessages(body)

        // An inline must-refetch inside a 200 body (iOS ShapeClient.swift
        // parity): strip it rather than forwarding the wipe, and mark the shape
        // so the next poll re-snapshots atomically. Rows stay put meanwhile.
        if (decoded.any { it is ShapeMessage.MustRefetch }) {
            val rest = decoded.filterNot { it is ShapeMessage.MustRefetch }
            if (rest.isNotEmpty()) {
                onMessages(rest)
                onApplied(countDataOps(rest))
            }
            offsetDao.upsert(
                ElectricOffsetEntity(
                    shape = shapeName,
                    handle = "",
                    offset = INITIAL_OFFSET,
                    isLive = false,
                    needsRefetch = true,
                )
            )
            return PollOutcome(completed = false, pause = true)
        }

        val sawUpToDate = decoded.any { it is ShapeMessage.UpToDate }
        // The refetch batch: the wipe rides IN FRONT of the snapshot rows, so
        // SyncManager's single transaction does DELETE-then-INSERT atomically.
        val messages = if (refetching) listOf(ShapeMessage.MustRefetch) + decoded else decoded
        if (messages.isNotEmpty()) {
            onMessages(messages)
            onApplied(countDataOps(messages))
        }

        if (handle != null && offset != null) {
            offsetDao.upsert(
                ElectricOffsetEntity(
                    shape = shapeName,
                    handle = handle,
                    offset = offset,
                    // wasLive is forced false while refetching, so a refetch
                    // only goes live once it has seen its own up-to-date.
                    isLive = sawUpToDate || wasLive,
                    needsRefetch = false,
                )
            )
        }

        return PollOutcome(
            completed = true,
            pause = !wasLive && !sawUpToDate && decoded.isEmpty(),
        )
    }

    /** Data ops (insert/update/partial/delete) in a batch — control msgs excluded. */
    private fun countDataOps(messages: List<ShapeMessage<T>>): Int = messages.count {
        it is ShapeMessage.Insert || it is ShapeMessage.Update ||
            it is ShapeMessage.PartialUpdate || it is ShapeMessage.Delete
    }

    private fun decodeMessages(body: String): List<ShapeMessage<T>> {
        if (body.isBlank()) return emptyList()
        val raw = try {
            json.decodeFromString(rawMessageSerializer, body)
        } catch (error: SerializationException) {
            android.util.Log.w("ShapeClient", "[$shapeName] decode failed: ${error.message}")
            return emptyList()
        }
        return raw.mapNotNull { msg -> mapMessage(msg) }
    }

    private fun mapMessage(msg: RawMessage): ShapeMessage<T>? {
        val control = (msg.headers["control"] as? JsonElement)?.jsonPrimitive?.contentOrNull
        if (control != null) {
            return when (control) {
                "up-to-date" -> ShapeMessage.UpToDate
                "must-refetch" -> ShapeMessage.MustRefetch
                // Chunk boundary of a multi-response snapshot — carries no
                // data; liveness is gated on up-to-date, never snapshot-end.
                "snapshot-end" -> null
                else -> null
            }
        }
        val operation = (msg.headers["operation"] as? JsonElement)?.jsonPrimitive?.contentOrNull
        val key = msg.key ?: return null
        val valueJson = msg.value
        val decodedValue: T? = if (valueJson is JsonObject) {
            try {
                json.decodeFromJsonElement(valueSerializer, valueJson)
            } catch (error: SerializationException) {
                android.util.Log.w("ShapeClient", "[$shapeName] value decode failed: ${error.message}")
                null
            }
        } else null

        return when (operation) {
            // A full-row insert that won't decode used to vanish silently; surface
            // it (the row still arrives on the next refetch). Updates that fail to
            // decode fall through to PartialUpdate, which tolerant-apply absorbs.
            "insert" -> decodedValue?.let { ShapeMessage.Insert(key, it) }
                ?: run { onDecodeDrop(key); null }
            "update" -> decodedValue?.let { ShapeMessage.Update(key, it) }
                ?: (valueJson as? JsonObject)?.let { ShapeMessage.PartialUpdate(key, it.toString()) }
            "delete" -> ShapeMessage.Delete(key, decodedValue)
            else -> null
        }
    }
}
