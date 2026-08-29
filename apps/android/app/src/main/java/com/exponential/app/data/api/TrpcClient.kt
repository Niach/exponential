package com.exponential.app.data.api

import android.util.Log
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.auth.SessionInvalidator
import io.ktor.client.HttpClient
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.statement.bodyAsText
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.contentType
import io.ktor.http.isSuccess
import java.net.URLEncoder
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject

/**
 * The message is user-presentable — many surfaces render it directly, so
 * TrpcClient sanitizes at the throw site (EXP-219) and raw response bodies
 * only go to logcat.
 */
class TrpcException(message: String, val status: HttpStatusCode? = null) : RuntimeException(message)

/**
 * Prefix every plan-limit throw in the server's lib/billing.ts uses — kept in
 * sync with the web's `PLAN_LIMIT_MESSAGE_PREFIX` (apps/web/src/lib/plan-limit-error.ts).
 */
const val PLAN_LIMIT_MESSAGE_PREFIX = "Your plan allows"

/**
 * Neutral plan-cap copy shown instead of the server's message, which carries
 * purchase language ("Add seats or upgrade…") the native apps must not render
 * (store billing policy — EXP-216).
 */
const val PLAN_LIMIT_NEUTRAL_MESSAGE = "This team has reached its plan limit."

/**
 * Leading clause of the server's team-delete billing gate (REV2-55):
 * `teams.delete` / `admin.deleteTeam` refuse a team whose subscription is
 * still live. Kept in sync with `TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE`
 * (apps/web/src/lib/billing/billing-handover.ts) — matched on this stable
 * clause only, because the server's trailing pointer names a web-only screen.
 */
const val TEAM_DELETE_SUBSCRIPTION_MESSAGE_PREFIX = "This team has an active subscription"

/**
 * Native copy for that gate. The server's wording sends the owner to "team
 * settings → Billing", which exists on the web ONLY (this app ships no
 * billing UI — EXP-216 / store policy), so the refusal names the web instead
 * of a screen the user cannot reach here.
 */
const val TEAM_DELETE_SUBSCRIPTION_MESSAGE =
    "This team has an active subscription. Cancel the subscription on the web before deleting the team."

/**
 * EXP-533: the ONE sentence every client shows when a request never reached
 * the server. Byte-identical to the web `OFFLINE_ERROR_MESSAGE`, iOS
 * `offlineErrorMessage` and desktop `api::error::OFFLINE_MESSAGE` — a
 * transport failure must read as "you are offline", never as the platform's
 * raw text (the Android leak this issue opened on was
 * `Unable to resolve host "app.exponential.at"`).
 */
const val OFFLINE_MESSAGE = "You're offline. Check your connection and try again."

/**
 * Whether [error] is a transport failure — the request never got an answer.
 * Walks the cause chain, since ktor wraps engine exceptions (same shape as
 * `ShapeClient.isNetworkUnready`).
 *
 * A [TrpcException] is NEVER offline: it is thrown only after a response came
 * back, which proves the server was reachable.
 */
fun isOfflineError(error: Throwable?): Boolean {
    var t: Throwable? = error
    var depth = 0
    while (t != null && depth < 16) {
        if (t is TrpcException) return false
        val offline = when (t) {
            is java.nio.channels.UnresolvedAddressException,
            is java.net.UnknownHostException,
            is java.net.ConnectException,
            is java.net.NoRouteToHostException,
            is java.net.PortUnreachableException,
            // ktor's SocketTimeoutException is a JVM typealias for java.net's;
            // only ConnectTimeoutException is a distinct class.
            is java.net.SocketTimeoutException,
            is io.ktor.client.network.sockets.ConnectTimeoutException,
            -> true
            // Everything else the engine can raise on a dead radio, a dropped
            // VPN or a closed socket mid-body is an IOException.
            else -> t is java.io.IOException
        }
        if (offline) return true
        val next = t.cause
        t = if (next === t) null else next
        depth++
    }
    return false
}

/**
 * Whether [error] is a REAL merge conflict — the only failure the builtin
 * "Fix merge conflicts" recovery run can do anything about. Everything else a
 * merge can fail with (branch protection, a stale base, an unreachable server)
 * must not offer it.
 */
fun isConflictError(error: Throwable?): Boolean {
    val trpc = error as? TrpcException ?: return false
    if (trpc.status == HttpStatusCode.Conflict) return true
    // TRANSITIONAL (EXP-533): remove once every server answers a real conflict with 409
    return trpc.status == HttpStatusCode.PreconditionFailed &&
        trpc.message?.contains("has merge conflicts with") == true
}

/**
 * The tRPC failure's user-presentable message, or [fallback] for anything
 * that isn't a [TrpcException]. Sanitization (server `message` extraction +
 * EXP-216 plan-cap neutralization) happens at the throw site in TrpcClient
 * (EXP-219), so the exception message is already safe to render.
 *
 * A transport failure answers [OFFLINE_MESSAGE] before anything else: its
 * `message` is the engine's own text, which is never presentable.
 */
fun trpcErrorMessage(error: Throwable, fallback: String): String {
    if (isOfflineError(error)) return OFFLINE_MESSAGE
    return (error as? TrpcException)?.message?.takeIf { it.isNotBlank() } ?: fallback
}

/**
 * Extract the user-presentable `message` from a tRPC error body
 * (`{"error":{"message":…}}`, tolerating the nested `error.json` payload).
 * Plan-cap messages are replaced with neutral copy and the team-delete
 * billing gate with its native twin — the server's wording is written for the
 * web, where billing lives. Null when nothing extractable.
 */
fun trpcUserMessageFromBody(body: String): String? {
    val message = runCatching {
        val err = Json.parseToJsonElement(body).jsonObject["error"]?.jsonObject
        val payload = (err?.get("json") as? JsonObject) ?: err
        (payload?.get("message") as? JsonPrimitive)?.contentOrNull
    }.getOrNull()
    if (message.isNullOrBlank()) return null
    return when {
        message.startsWith(PLAN_LIMIT_MESSAGE_PREFIX) -> PLAN_LIMIT_NEUTRAL_MESSAGE
        message.startsWith(TEAM_DELETE_SUBSCRIPTION_MESSAGE_PREFIX) -> TEAM_DELETE_SUBSCRIPTION_MESSAGE
        else -> message
    }
}

@Singleton
class TrpcClient @Inject constructor(
    private val client: HttpClient,
    private val auth: AuthRepository,
    private val json: Json,
    private val sessionInvalidator: SessionInvalidator,
) {
    private fun accountUrl(accountId: String): String =
        auth.accounts.value.firstOrNull { it.id == accountId }?.instanceUrl
            ?: throw TrpcException("No instance URL for account $accountId")

    private fun accountToken(accountId: String): String? =
        auth.accounts.value.firstOrNull { it.id == accountId }?.token

    suspend fun <I, O> mutation(
        accountId: String,
        path: String,
        input: I,
        inputSerializer: KSerializer<I>,
        outputSerializer: KSerializer<O>,
    ): O {
        val baseUrl = accountUrl(accountId)
        val token = accountToken(accountId)
        // The server runs tRPC with NO transformer, so the body is the raw
        // input JSON — never the superjson `{"json": ...}` envelope (the
        // server would see it as the literal input and fail Zod validation).
        val inputJson = json.encodeToJsonElement(inputSerializer, input)
        val response = client.post("$baseUrl/api/trpc/$path") {
            contentType(ContentType.Application.Json)
            setBody(json.encodeToString(JsonElement.serializer(), inputJson))
            if (token != null) header("Authorization", "Bearer $token")
        }
        val text = response.bodyAsText()
        if (!response.status.isSuccess()) {
            // Keep the raw body diagnosable in logcat; the thrown message is
            // user-presentable (EXP-219).
            Log.w("TrpcClient", "tRPC $path HTTP ${response.status.value}: $text")
            // Classified on the STATUS, never the message text: a 401 answering
            // a bearer we hold means that session is gone, so the account is
            // signed out locally and the app routes to login instead of every
            // screen 401ing forever.
            sessionInvalidator.reportStatus(
                accountId = accountId,
                statusCode = response.status.value,
                tokenPresented = token != null,
            )
            throw TrpcException(
                trpcUserMessageFromBody(text) ?: "Request failed (HTTP ${response.status.value})",
                response.status,
            )
        }
        return decodePayload(path, text, outputSerializer)
    }

    /** A mutation whose response payload is ignored (no output type needed). */
    suspend fun <I> mutationUnit(
        accountId: String,
        path: String,
        input: I,
        inputSerializer: KSerializer<I>,
    ) {
        mutation(accountId, path, input, inputSerializer, JsonElement.serializer())
    }

    suspend fun <I, O> query(
        accountId: String,
        path: String,
        input: I,
        inputSerializer: KSerializer<I>,
        outputSerializer: KSerializer<O>,
        omitInputIfEmpty: Boolean = true,
    ): O {
        val baseUrl = accountUrl(accountId)
        val token = accountToken(accountId)
        val inputJson = json.encodeToJsonElement(inputSerializer, input)
        val isEmpty = inputJson is JsonObject && inputJson.isEmpty()
        val url = if (omitInputIfEmpty && isEmpty) {
            "$baseUrl/api/trpc/$path"
        } else {
            // No transformer on the server: `?input=` carries the raw input
            // JSON, not the superjson `{"json": ...}` wrapper.
            val encoded = URLEncoder.encode(json.encodeToString(JsonElement.serializer(), inputJson), "UTF-8")
            "$baseUrl/api/trpc/$path?input=$encoded"
        }
        val response = client.get(url) {
            if (token != null) header("Authorization", "Bearer $token")
        }
        val text = response.bodyAsText()
        if (!response.status.isSuccess()) {
            // Keep the raw body diagnosable in logcat; the thrown message is
            // user-presentable (EXP-219).
            Log.w("TrpcClient", "tRPC $path HTTP ${response.status.value}: $text")
            // Classified on the STATUS, never the message text: a 401 answering
            // a bearer we hold means that session is gone, so the account is
            // signed out locally and the app routes to login instead of every
            // screen 401ing forever.
            sessionInvalidator.reportStatus(
                accountId = accountId,
                statusCode = response.status.value,
                tokenPresented = token != null,
            )
            throw TrpcException(
                trpcUserMessageFromBody(text) ?: "Request failed (HTTP ${response.status.value})",
                response.status,
            )
        }
        return decodePayload(path, text, outputSerializer)
    }

    private fun <O> decodePayload(
        path: String,
        text: String,
        outputSerializer: KSerializer<O>,
    ): O {
        val envelope = json.parseToJsonElement(text) as? JsonObject
            ?: throw TrpcException("tRPC $path returned non-object")
        val data = (envelope["result"] as? JsonObject)?.get("data")
            ?: throw TrpcException("tRPC $path missing result.data")
        // result.data is the raw output value (no transformer); it may be an
        // object, array, or scalar.
        return json.decodeFromJsonElement(outputSerializer, data)
    }
}
