package com.exponential.app.data.steer

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

/** #EDE-96 (fixtures/steer-enet.json SEARCH): the most hits one answer
 *  carries; a viewer asks for that many. */
const val STEER_SEARCH_HITS_MAX = 50

/** How long a search waits for its answer: the fixture's `sendTimeoutMs`. */
const val STEER_SEARCH_TIMEOUT_MS = 20_000L

/** The hit kinds this build draws; any other is skipped. */
val STEER_SEARCH_HIT_KINDS = setOf("user_message", "narration", "tool", "tool_output")

/** One match the device found: the ROW's [seq] (a tool's output = its
 *  `tool` row), the row [kind] and a [snippet] with the match's offset and
 *  length in CODE POINTS of the snippet. */
data class SteerSearchHit(
    val seq: Long,
    val kind: String,
    val snippet: String,
    val matchStart: Int,
    val matchLen: Int,
)

/** The device's `search_result`. [done] false = ask again with
 *  `beforeSeq = scannedToSeq`; [total] only when the whole journal was
 *  searched in this answer. */
data class SteerSearchResult(
    val hits: List<SteerSearchHit>,
    val total: Int?,
    val done: Boolean,
    val scannedToSeq: Long,
)

/** The viewer's `search` frame (`beforeSeq` null = from the newest event). */
fun steerSearchFrame(requestId: String, query: String, beforeSeq: Long?): JsonObject = buildJsonObject {
    put("t", "search")
    put("requestId", requestId)
    put("query", query)
    if (beforeSeq != null) put("beforeSeq", beforeSeq)
    put("limit", STEER_SEARCH_HITS_MAX)
}

/** A `search_result` frame's body; null = not one (or malformed). Hits of
 *  a kind this build does not know are skipped. */
fun parseSteerSearchResult(obj: JsonObject): SteerSearchResult? {
    if ((obj["t"] as? JsonPrimitive)?.contentOrNull != "search_result") return null
    val done = (obj["done"] as? JsonPrimitive)?.booleanOrNull ?: return null
    val scannedToSeq = (obj["scannedToSeq"] as? JsonPrimitive)?.longOrNull ?: return null
    val hits = runCatching { obj["hits"]!!.jsonArray }.getOrNull() ?: return null
    return SteerSearchResult(
        hits = hits.mapNotNull { element ->
            val hit = element as? JsonObject ?: return@mapNotNull null
            val kind = (hit["kind"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
            if (kind !in STEER_SEARCH_HIT_KINDS) return@mapNotNull null
            SteerSearchHit(
                seq = (hit["seq"] as? JsonPrimitive)?.longOrNull ?: return@mapNotNull null,
                kind = kind,
                snippet = (hit["snippet"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null,
                matchStart = (hit["matchStart"] as? JsonPrimitive)?.intOrNull ?: return@mapNotNull null,
                matchLen = (hit["matchLen"] as? JsonPrimitive)?.intOrNull ?: return@mapNotNull null,
            )
        },
        total = (obj["total"] as? JsonPrimitive)?.intOrNull,
        done = done,
        scannedToSeq = scannedToSeq,
    )
}
