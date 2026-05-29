package com.exponential.app.data.api

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
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

class TrpcException(message: String, val status: HttpStatusCode? = null) : RuntimeException(message)

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
        val inputJson = json.encodeToJsonElement(inputSerializer, input)
        val body = buildJsonObject { put("json", inputJson) }
        val response = client.post("$baseUrl/api/trpc/$path") {
            contentType(ContentType.Application.Json)
            setBody(body)
            if (token != null) header("Authorization", "Bearer $token")
        }
        val text = response.bodyAsText()
        if (!response.status.isSuccess()) {
            throw TrpcException("tRPC $path HTTP ${response.status.value}: $text", response.status)
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
            val wrapper = buildJsonObject { put("json", inputJson) }
            val encoded = URLEncoder.encode(json.encodeToString(JsonObject.serializer(), wrapper), "UTF-8")
            "$baseUrl/api/trpc/$path?input=$encoded"
        }
        val response = client.get(url) {
            if (token != null) header("Authorization", "Bearer $token")
        }
        val text = response.bodyAsText()
        if (!response.status.isSuccess()) {
            throw TrpcException("tRPC $path HTTP ${response.status.value}: $text", response.status)
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
        val data = (envelope["result"] as? JsonObject)
            ?.get("data") as? JsonObject
            ?: throw TrpcException("tRPC $path missing result.data")
        val payload: JsonElement = data["json"] ?: data
        return json.decodeFromJsonElement(outputSerializer, payload)
    }
}
