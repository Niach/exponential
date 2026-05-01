package com.exponential.app.data.electric

import com.exponential.app.data.db.ElectricOffsetDao
import com.exponential.app.data.db.ElectricOffsetEntity
import io.ktor.client.HttpClient
import io.ktor.client.request.get
import io.ktor.client.request.parameter
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsText
import io.ktor.http.HttpStatusCode
import io.ktor.http.isSuccess
import kotlinx.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
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
) {
    private val rawMessageSerializer = kotlinx.serialization.builtins.ListSerializer(
        kotlinx.serialization.serializer<RawMessage>()
    )

    suspend fun run() {
        var backoffMs = 500L
        while (coroutineContext.isActive) {
            try {
                val baseUrl = baseUrlProvider()
                val token = tokenProvider()
                if (baseUrl == null || token == null) {
                    delay(2_000)
                    continue
                }
                pollOnce(baseUrl)
                backoffMs = 500L
            } catch (cancel: CancellationException) {
                throw cancel
            } catch (error: Throwable) {
                android.util.Log.w("ShapeClient", "[$shapeName] error: ${error.message}", error)
                delay(backoffMs)
                backoffMs = min(backoffMs * 2, 30_000L)
            }
        }
    }

    private suspend fun pollOnce(baseUrl: String) {
        val saved = offsetDao.get(shapeName)
        val isInitial = saved == null
        val response: HttpResponse = withTimeoutOrNull(LIVE_TIMEOUT_MS + 30_000L) {
            client.get("$baseUrl$urlPath") {
                if (isInitial) {
                    parameter("offset", INITIAL_OFFSET)
                } else {
                    parameter("offset", saved!!.offset)
                    parameter("handle", saved.handle)
                    parameter("live", "true")
                }
            }
        } ?: throw IOException("Shape $shapeName request timed out")

        if (response.status == HttpStatusCode.Conflict || response.status.value == 409) {
            offsetDao.deleteShape(shapeName)
            onMessages(listOf(ShapeMessage.MustRefetch))
            return
        }
        if (response.status == HttpStatusCode.Unauthorized) {
            throw IOException("Unauthorized syncing $shapeName")
        }
        if (!response.status.isSuccess()) {
            throw IOException("Shape $shapeName HTTP ${response.status.value}")
        }

        val handle = response.headers["electric-handle"]
        val offset = response.headers["electric-offset"]
        val body = response.bodyAsText()
        val messages = decodeMessages(body)
        if (messages.isNotEmpty()) onMessages(messages)

        if (handle != null && offset != null) {
            offsetDao.upsert(ElectricOffsetEntity(shape = shapeName, handle = handle, offset = offset))
        }
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
            "insert" -> decodedValue?.let { ShapeMessage.Insert(key, it) }
            "update" -> decodedValue?.let { ShapeMessage.Update(key, it) }
            "delete" -> ShapeMessage.Delete(key, decodedValue)
            else -> null
        }
    }
}
