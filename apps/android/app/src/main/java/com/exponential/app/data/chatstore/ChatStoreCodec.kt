package com.exponential.app.data.chatstore

import com.exponential.app.domain.TailEvent
import com.exponential.app.domain.WireTimestamps
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

/** #EDE-114: OPEN's answer: the stored events through the REPLAY TAIL, and whether older ones exist. */
data class StoredTail(val events: List<TailEvent>, val truncated: Boolean)

/** #EDE-114: one synced message as the store keeps it (`body` null = its chat key is not here yet). */
data class StoredMessageRow(
    val id: String,
    val rootId: String,
    val sessionId: String,
    val authorKind: String,
    val body: String?,
    val createdAtMs: Long,
)

/** #EDE-130 (find in chat): one `searchChat` hit (chat-store.json SEARCH):
 *  an event's names its [seq] (a tool output's = the tool row's), a synced
 *  message's its [messageId]. */
data class StoredChatHit(
    val sessionId: String,
    val kind: String,
    val seq: Long?,
    val messageId: String?,
)

/**
 * #EDE-114 (chat-store.json S3): the JSON the facade takes and gives
 * (`chat-store-ffi`): a feed's `[{seq, event}]`, OPEN's
 * `{events, truncated}`, a page's `{events, seqs}` and the
 * `StoredMessage` rows. Pure: no store, no I/O.
 */
object ChatStoreCodec {
    /** A feed for `putFeed`: only events with a wire seq (the store keys them by it). */
    fun feedJson(events: List<TailEvent>): String = buildJsonArray {
        for (tail in events) {
            val seq = tail.seq ?: continue
            if (seq < 0) continue
            add(
                buildJsonObject {
                    put("seq", seq)
                    put("event", tail.event)
                },
            )
        }
    }.toString()

    /** `openTail`'s answer; null = not one. */
    fun parseTail(text: String): StoredTail? = runCatching {
        val obj = Json.parseToJsonElement(text).jsonObject
        val events = (obj["events"] as? JsonArray).orEmpty().mapNotNull { row ->
            val entry = row as? JsonObject ?: return@mapNotNull null
            val event = entry["event"] as? JsonObject ?: return@mapNotNull null
            TailEvent(event, (entry["seq"] as? JsonPrimitive)?.longOrNull)
        }
        StoredTail(events, (obj["truncated"] as? JsonPrimitive)?.booleanOrNull == true)
    }.getOrNull()

    /** `page`'s answer (oldest first), the events paired with their seqs. */
    fun parsePage(text: String): List<TailEvent> = runCatching {
        val obj = Json.parseToJsonElement(text).jsonObject
        val events = (obj["events"] as? JsonArray).orEmpty()
        val seqs = (obj["seqs"] as? JsonArray).orEmpty()
        events.mapIndexedNotNull { index, element ->
            val event = element as? JsonObject ?: return@mapIndexedNotNull null
            TailEvent(event, (seqs.getOrNull(index) as? JsonPrimitive)?.longOrNull)
        }
    }.getOrDefault(emptyList())

    /** #EDE-130: `searchChat`'s `{hits, total}` (newest first); empty = none or unreadable. */
    fun parseHits(text: String): List<StoredChatHit> = runCatching {
        val obj = Json.parseToJsonElement(text).jsonObject
        (obj["hits"] as? JsonArray).orEmpty().mapNotNull { row ->
            val hit = row as? JsonObject ?: return@mapNotNull null
            val sessionId = (hit["sessionId"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
            StoredChatHit(
                sessionId = sessionId,
                kind = (hit["kind"] as? JsonPrimitive)?.contentOrNull.orEmpty(),
                seq = (hit["seq"] as? JsonPrimitive)?.longOrNull,
                messageId = (hit["messageId"] as? JsonPrimitive)?.contentOrNull,
            )
        }
    }.getOrDefault(emptyList())

    /** #EDE-99: `searchAll`'s `{chats, total}` (newest chat first); empty = none or unreadable. */
    fun parseAll(text: String): List<com.exponential.app.domain.SearchList.ChatMatch> = runCatching {
        val obj = Json.parseToJsonElement(text).jsonObject
        (obj["chats"] as? JsonArray).orEmpty().mapNotNull { row ->
            val chat = row as? JsonObject ?: return@mapNotNull null
            val rootId = (chat["rootId"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
            val hit = chat["hit"] as? JsonObject ?: return@mapNotNull null
            fun str(key: String) = (hit[key] as? JsonPrimitive)?.contentOrNull
            fun long(key: String) = (hit[key] as? JsonPrimitive)?.longOrNull
            com.exponential.app.domain.SearchList.ChatMatch(
                rootId = rootId,
                count = ((chat["count"] as? JsonPrimitive)?.longOrNull ?: 1L).toInt(),
                hit = com.exponential.app.domain.SearchList.ChatHit(
                    rootId = str("rootId") ?: rootId,
                    sessionId = str("sessionId") ?: return@mapNotNull null,
                    deviceId = str("deviceId"),
                    kind = str("kind").orEmpty(),
                    at = long("at") ?: 0L,
                    seq = long("seq"),
                    messageId = str("messageId"),
                    snippet = str("snippet").orEmpty(),
                    matchStart = (long("matchStart") ?: 0L).toInt(),
                    matchLen = (long("matchLen") ?: 0L).toInt(),
                    system = (hit["system"] as? JsonPrimitive)?.booleanOrNull == true,
                ),
            )
        }
    }.getOrDefault(emptyList())

    /** `putMessages`' input (`StoredMessage`, camelCase, `createdAt` in ms). */
    fun messagesJson(rows: List<StoredMessageRow>): String = buildJsonArray {
        for (row in rows) {
            add(
                buildJsonObject {
                    put("id", row.id)
                    put("rootId", row.rootId)
                    put("sessionId", row.sessionId)
                    put("authorKind", row.authorKind)
                    put("body", row.body?.let(::JsonPrimitive) ?: JsonNull)
                    put("createdAt", row.createdAtMs)
                },
            )
        }
    }.toString()

    /** A synced timestamp in ms (ISO or Postgres text), null = unreadable. */
    fun epochMs(value: String?): Long? = value?.takeIf { it.isNotBlank() }?.let(WireTimestamps::parseEpochMs)

    /** A frame's `t`. */
    fun tag(frame: JsonObject): String? = (frame["t"] as? JsonPrimitive)?.contentOrNull

    /** A frame's `requestId`. */
    fun requestId(frame: JsonObject): String? = (frame["requestId"] as? JsonPrimitive)?.contentOrNull
}
