package com.exponential.app.data.api

import com.exponential.app.data.auth.AuthRepository
import io.ktor.client.HttpClient
import io.ktor.client.request.get
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
    suspend fun <I, O> mutation(
        path: String,
        input: I,
        inputSerializer: KSerializer<I>,
        outputSerializer: KSerializer<O>,
    ): O {
        val baseUrl = auth.instanceUrl.value
            ?: throw TrpcException("No instance URL configured")
        val inputJson = json.encodeToJsonElement(inputSerializer, input)
        val body = buildJsonObject { put("json", inputJson) }
        val response = client.post("$baseUrl/api/trpc/$path") {
            contentType(ContentType.Application.Json)
            setBody(body)
        }
        val text = response.bodyAsText()
        if (!response.status.isSuccess()) {
            throw TrpcException("tRPC $path HTTP ${response.status.value}: $text", response.status)
        }
        return decodePayload(path, text, outputSerializer)
    }

    /**
     * Calls a tRPC `.query` procedure. tRPC rejects POST against query procedures
     * with METHOD_NOT_SUPPORTED, so reads MUST go through GET with the input
     * encoded as a single `?input=<json>` query string parameter (`{"json": …}`).
     * Pass [IntegrationsEmptyInput] (or any unit-like serializer) for procedures
     * without input — empty inputs are omitted from the URL entirely.
     */
    suspend fun <I, O> query(
        path: String,
        input: I,
        inputSerializer: KSerializer<I>,
        outputSerializer: KSerializer<O>,
        omitInputIfEmpty: Boolean = true,
    ): O {
        val baseUrl = auth.instanceUrl.value
            ?: throw TrpcException("No instance URL configured")
        val inputJson = json.encodeToJsonElement(inputSerializer, input)
        val isEmpty = inputJson is JsonObject && inputJson.isEmpty()
        val url = if (omitInputIfEmpty && isEmpty) {
            "$baseUrl/api/trpc/$path"
        } else {
            val wrapper = buildJsonObject { put("json", inputJson) }
            val encoded = URLEncoder.encode(json.encodeToString(JsonObject.serializer(), wrapper), "UTF-8")
            "$baseUrl/api/trpc/$path?input=$encoded"
        }
        val response = client.get(url)
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
