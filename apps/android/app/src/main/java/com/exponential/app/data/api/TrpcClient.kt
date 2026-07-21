package com.exponential.app.data.api

import android.util.Log
import com.exponential.app.data.auth.AuthRepository
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
 * Extract the user-presentable `message` from a tRPC error body
 * (`{"error":{"message":…}}`, tolerating the nested `error.json` payload).
 * Plan-cap messages are replaced with neutral copy — the server's wording is
 * written for the web, where billing lives. Null when nothing extractable.
 */
fun trpcUserMessageFromBody(body: String): String? {
    val message = runCatching {
        val err = Json.parseToJsonElement(body).jsonObject["error"]?.jsonObject
        val payload = (err?.get("json") as? JsonObject) ?: err
        (payload?.get("message") as? JsonPrimitive)?.contentOrNull
    }.getOrNull()
    if (message.isNullOrBlank()) return null
    return if (message.startsWith(PLAN_LIMIT_MESSAGE_PREFIX)) PLAN_LIMIT_NEUTRAL_MESSAGE else message
}

@Singleton
class TrpcClient @Inject constructor(
    private val client: HttpClient,
    private val auth: AuthRepository,
    private val json: Json,
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
