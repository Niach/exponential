package com.exponential.app.data.electric

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
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
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

private const val INITIAL_OFFSET = "-1"
private const val LIVE_TIMEOUT_MS = 60_000L
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
    // Wipe this shape's offset + rows so the next poll refetches a snapshot.
    private val onReset: suspend () -> Unit = {},
) {
    private val rawMessageSerializer = kotlinx.serialization.builtins.ListSerializer(
        kotlinx.serialization.serializer<RawMessage>()
    )

    suspend fun run() {
        var backoffMs = 500L
        var consecutiveSchemaErrors = 0
        var didAutoReset = false
        while (coroutineContext.isActive) {
            try {
                val baseUrl = baseUrlProvider()
                val token = tokenProvider()
                if (baseUrl == null || token == null) {
                    delay(2_000)
                    continue
                }
                val shouldPause = pollOnce(baseUrl, token)
                onSuccess()
                consecutiveSchemaErrors = 0
                backoffMs = 500L
                // Pace the loop when a non-live poll made no progress, so a
                // response that never reaches up-to-date can't spin-request.
                if (shouldPause) delay(500)
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
                delay(backoffMs)
                backoffMs = min(backoffMs * 2, 30_000L)
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
                delay(backoffMs)
                backoffMs = min(backoffMs * 2, 30_000L)
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
                delay(backoffMs)
                backoffMs = min(backoffMs * 2, 30_000L)
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

    /** Returns true when the run loop should pause before the next poll. */
    private suspend fun pollOnce(baseUrl: String, token: String): Boolean {
        val saved = offsetDao.get(shapeName)
        val isInitial = saved == null
        // Only long-poll live once the snapshot completed (up-to-date seen);
        // catch-up polls stay non-live per the Electric protocol. Sending
        // live=true from a mid-snapshot offset is rejected by Electric.
        val wasLive = saved?.isLive ?: false
        onPhase(if (isInitial) "initial" else if (wasLive) "live" else "catchup")
        val response: HttpResponse = withTimeoutOrNull(REQUEST_TIMEOUT_MS + 30_000L) {
            client.get("$baseUrl$urlPath") {
                // Long-poll budget — overrides the client-wide 30s default,
                // which would kill the idle live hold (see REQUEST_TIMEOUT_MS).
                timeout {
                    requestTimeoutMillis = REQUEST_TIMEOUT_MS
                    socketTimeoutMillis = REQUEST_TIMEOUT_MS
                }
                // Authenticate the shape request so the server scopes data to
                // this user (not just public workspaces). Mirrors TrpcClient /
                // AuthApi / IssueImagesApi. Without this, every shape polled
                // anonymously and only public rows synced — the root cause of
                // missing workspaces/projects and the 401 on workspace_invites.
                header("Authorization", "Bearer $token")
                if (isInitial) {
                    parameter("offset", INITIAL_OFFSET)
                } else {
                    parameter("offset", saved!!.offset)
                    parameter("handle", saved.handle)
                    if (wasLive) parameter("live", "true")
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
            offsetDao.deleteShape(shapeName)
            onMessages(listOf(ShapeMessage.MustRefetch))
            return false
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
        val messages = decodeMessages(body)
        val sawUpToDate = messages.any { it is ShapeMessage.UpToDate }
        if (messages.isNotEmpty()) {
            onMessages(messages)
            // Count only data ops (insert/update/partial/delete), not control msgs.
            val dataOps = messages.count {
                it is ShapeMessage.Insert || it is ShapeMessage.Update ||
                    it is ShapeMessage.PartialUpdate || it is ShapeMessage.Delete
            }
            onApplied(dataOps)
        }

        if (handle != null && offset != null) {
            offsetDao.upsert(
                ElectricOffsetEntity(
                    shape = shapeName,
                    handle = handle,
                    offset = offset,
                    isLive = sawUpToDate || wasLive,
                )
            )
        }

        return !wasLive && !sawUpToDate && messages.isEmpty()
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
