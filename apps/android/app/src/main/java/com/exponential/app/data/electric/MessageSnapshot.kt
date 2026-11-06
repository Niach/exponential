package com.exponential.app.data.electric

import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.MessageEntity
import io.ktor.client.HttpClient
import io.ktor.client.request.get
import io.ktor.client.request.header
import io.ktor.client.request.parameter
import io.ktor.client.statement.bodyAsText
import io.ktor.http.isSuccess
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.io.IOException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

/**
 * #EXP-1641: ONE message read off the `messages` shape proxy on demand, the
 * web's on-demand read (#EXP-1380: an Electric subset snapshot, `subset__*`
 * over the shape's own columns, the same member-only visibility), for a
 * notification action that answers a card this install has not synced yet.
 * The row lands in the local store raw (sealed as synced), exactly as the
 * shape loop would write it; the shape loop's own cursor is untouched.
 */
@Singleton
class MessageSnapshot @Inject constructor(
    private val client: HttpClient,
    private val json: Json,
    private val auth: AuthRepository,
    private val dbs: DatabaseHolder,
) {
    /** The message [messageId] stored locally, null when it is not visible. Throws on a failed read. */
    suspend fun fetch(accountId: String, messageId: String): MessageEntity? {
        val account = auth.accounts.value.firstOrNull { it.id == accountId } ?: return null
        val token = account.token ?: return null
        val response = client.get("${account.instanceUrl.trimEnd('/')}$PATH") {
            header("Authorization", "Bearer $token")
            parameter("offset", INITIAL_OFFSET)
            parameter("log", "changes_only")
            parameter("subset__where", "\"id\" = $1")
            parameter("subset__params", """{"1":${Json.encodeToString(kotlinx.serialization.json.JsonPrimitive(messageId))}}""")
            parameter("subset__limit", "1")
        }
        if (!response.status.isSuccess()) throw IOException("message snapshot ${response.status.value}")
        val row = rows(json, response.bodyAsText()).firstOrNull { it.id == messageId } ?: return null
        dbs.database(accountId).messageDao().upsert(row)
        return row
    }

    internal companion object {
        const val PATH = "/api/shapes/messages"

        /** The inserts of a subset snapshot body (`{metadata, data: [...]}`, or a bare message array). */
        fun rows(json: Json, body: String): List<MessageEntity> {
            val parsed = runCatching { json.parseToJsonElement(body) }.getOrNull()
                ?: throw IOException("message snapshot body")
            val data = when (parsed) {
                is JsonObject -> parsed["data"] as? JsonArray
                is JsonArray -> parsed
                else -> null
            } ?: throw IOException("message snapshot body")
            return data.mapNotNull { element ->
                val msg = element as? JsonObject ?: return@mapNotNull null
                val operation = (msg["headers"] as? JsonObject)?.get("operation")?.jsonPrimitive?.contentOrNull
                if (operation != null && operation != "insert") return@mapNotNull null
                val value = msg["value"] as? JsonObject ?: return@mapNotNull null
                runCatching { json.decodeFromJsonElement(MessageEntity.serializer(), value) }.getOrNull()
            }
        }
    }
}
