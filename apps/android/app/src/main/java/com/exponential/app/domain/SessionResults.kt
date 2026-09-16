package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlin.math.floor
import kotlinx.serialization.json.intOrNull

// EXP-879: a coding run's published RESULTS — the screenshots the agent filed
// with `exponential_sessions_results` while it worked, read off the synced
// `coding_sessions.results` jsonb (kept here as the raw column TEXT).
//
// The blob is a FLAT, ORDERED list: `{topic, label, attachmentId, width,
// height}`. Grouping is DERIVED, never stored, so an agent that publishes
// `web` then `ios` under one topic and later a second topic keeps the order it
// chose. `(topic, label)` is the upsert key server-side (it replaces in place),
// so a client only ever renders what it reads.
//
// These are the PURE rules every client mirrors byte for byte: web
// `lib/session-results.ts` (the spec and its tests), desktop
// `crates/ui/src/session_results.rs`, iOS `ExpCore/Domain/SessionResults.swift`
// — same names, same order, same test names (`SessionResultsTest`).
//
// Compose-free on purpose, like every other file in this package: the tile
// height is a plain Int the renderer turns into `dp`.

/** The cap the server enforces on a single run's list. */
const val MAX_SESSION_RESULTS = 60

/**
 * Every tile renders at ONE height (density-independent pixels); the probed
 * aspect gives its width, so a row of an iOS, an Android and a web shot reads
 * as one strip.
 */
const val SESSION_RESULT_TILE_HEIGHT = 320

/**
 * One published screenshot. [width]/[height] are probed at upload and null
 * whenever the image could not be measured — the renderer then falls back to
 * 4:3 rather than guessing.
 */
data class SessionResultEntry(
    val topic: String,
    val label: String,
    val attachmentId: String,
    val width: Int? = null,
    val height: Int? = null,
)

/** A topic and the entries published under it, in publication order. */
data class SessionResultGroup(
    val topic: String,
    val entries: List<SessionResultEntry>,
)

private val resultsJson = Json { ignoreUnknownKeys = true; isLenient = true }

/** A trimmed non-blank string field, else null. */
private fun JsonObject.text(name: String): String? =
    (this[name] as? JsonPrimitive)
        ?.takeIf { it.isString }
        ?.content
        ?.trim()
        ?.takeIf { it.isNotEmpty() }

/**
 * A probed dimension, or null: a zero, a negative, a non-number and a
 * quoted `"800"` all mean "unknown" and fall back to the 4:3 default. Matching
 * the web's `typeof value === number` check, a JSON string is NOT a dimension.
 */
private fun JsonObject.dimension(name: String): Int? {
    val primitive = this[name] as? JsonPrimitive ?: return null
    if (primitive.isString) return null
    return primitive.intOrNull?.takeIf { it > 0 }
}

/**
 * Tolerant reader for the jsonb blob's raw text. Null, blank, unparseable or
 * simply not an array all read as "nothing published"; a malformed entry is
 * DROPPED rather than rendered as a broken tile; the list is capped like the
 * writer caps it.
 */
fun parseSessionResults(raw: String?): List<SessionResultEntry> {
    val trimmed = raw?.trim()?.takeIf { it.isNotEmpty() } ?: return emptyList()
    val array = runCatching { resultsJson.parseToJsonElement(trimmed) }.getOrNull() as? JsonArray
        ?: return emptyList()
    val entries = mutableListOf<SessionResultEntry>()
    for (element in array) {
        val row = element as? JsonObject ?: continue
        val topic = row.text("topic") ?: continue
        val label = row.text("label") ?: continue
        val attachmentId = row.text("attachmentId") ?: continue
        entries += SessionResultEntry(
            topic = topic,
            label = label,
            attachmentId = attachmentId,
            width = row.dimension("width"),
            height = row.dimension("height"),
        )
        if (entries.size >= MAX_SESSION_RESULTS) break
    }
    return entries
}

/**
 * Groups by topic in FIRST-SEEN order, keeping each group's entries in the
 * order the agent published them.
 */
fun groupSessionResults(entries: List<SessionResultEntry>): List<SessionResultGroup> {
    val order = mutableListOf<String>()
    val byTopic = linkedMapOf<String, MutableList<SessionResultEntry>>()
    for (entry in entries) {
        val bucket = byTopic.getOrPut(entry.topic) {
            order += entry.topic
            mutableListOf()
        }
        bucket += entry
    }
    return order.map { topic -> SessionResultGroup(topic, byTopic.getValue(topic).toList()) }
}

/**
 * The tile's width at a fixed [height] — the probed aspect, else 4:3 (a
 * desktop screenshot's shape, and the least surprising placeholder).
 */
fun sessionResultTileWidth(
    entry: SessionResultEntry,
    height: Int = SESSION_RESULT_TILE_HEIGHT,
): Int {
    val width = entry.width
    val entryHeight = entry.height
    val aspect = if (width != null && entryHeight != null) {
        width.toDouble() / entryHeight.toDouble()
    } else {
        4.0 / 3.0
    }
    return Math.round(height * aspect).toInt()
}

/**
 * The tile height that makes the page FIT: on a phone a landscape shot is
 * 480dp wide at the 320dp base and a 390dp screen clips it, so the whole page
 * scales down by ONE factor — the widest tile's overflow — instead of letting
 * a row clip or each row pick its own size. One factor keeps every tile's
 * aspect (`sessionResultTileWidth(entry, thatHeight)`) AND the equal-height
 * strip, which is the point of a fixed height: an iOS, an Android and a web
 * shot of one screen still read as one row. Never scales UP: a wide page keeps
 * the base so shots never look blown out.
 *
 * [availableWidth] is the tiles container's content width in dp; a zero or a
 * negative one means "not measured yet" and renders at the base.
 */
fun sessionResultTileHeightFitting(
    entries: List<SessionResultEntry>,
    availableWidth: Int,
    base: Int = SESSION_RESULT_TILE_HEIGHT,
): Int {
    if (availableWidth <= 0) return base
    var widest = 0
    for (entry in entries) {
        val width = sessionResultTileWidth(entry, base)
        if (width > widest) widest = width
    }
    if (widest <= availableWidth) return base
    return maxOf(1, floor(base.toDouble() * availableWidth.toDouble() / widest.toDouble()).toInt())
}
