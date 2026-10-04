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
 * EXP-1128: a picture whose probed width/height is UNDER this is TALL (a
 * full-page capture; a phone shot at ~0.46 never is). A tall picture takes the
 * 4:3 frame top-cropped with a Tall badge instead of rendering as a sliver, and
 * opens fit-to-width in a vertical scroll. Strict: exactly 1:3 is not tall.
 * Fixture `session-results.json` `tiles` (×4).
 */
const val SESSION_RESULT_TALL_ASPECT = 1.0 / 3.0

/**
 * EXP-1172: an `exponential_sessions_show` picture's tile in the run
 * transcript, one base height lower than a Results tile so a shot sits in the
 * conversation without swallowing it. Fixture `session-inline.json`.
 */
const val SESSION_INLINE_TILE_HEIGHT = 240

/** EXP-1172: the collapsed band a Results group folds its inline pictures under (`Earlier · 3`). */
const val SESSION_RESULTS_EARLIER_LABEL = "Earlier"

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
    /** EXP-1172: filed by `exponential_sessions_show` while the run worked (true only for a JSON true). */
    val inline: Boolean = false,
    /** EXP-1172: the show call's `text`, trimmed; null when blank. */
    val caption: String? = null,
)

/**
 * A topic and the entries published under it, in publication order, plus
 * (EXP-933) the topic's GFM report [text] rendered ABOVE its pictures — null
 * without one.
 */
data class SessionResultGroup(
    val topic: String,
    val entries: List<SessionResultEntry>,
    val text: String? = null,
    /**
     * EXP-1172: the topic's INLINE pictures, folded under the `Earlier` band
     * (publish order); empty when the topic has a single picture.
     */
    val earlier: List<SessionResultEntry> = emptyList(),
    /**
     * EXP-1154: the repo paths the topic's report touched, off the SAME entry
     * its [text] came from (trimmed, deduped first-seen, capped at
     * [SESSION_RESULT_FILES_MAX]); empty without.
     */
    val files: List<String> = emptyList(),
)

/** EXP-1154: the most paths one topic lists (fixture `files.maxFiles`). */
const val SESSION_RESULT_FILES_MAX = 40

/**
 * EXP-1154: a text entry's `files`: strings only, trimmed, blanks and
 * duplicates dropped (first position kept), capped; a non-array is none.
 */
private fun JsonObject.resultFiles(): List<String> {
    val array = this["files"] as? JsonArray ?: return emptyList()
    val out = mutableListOf<String>()
    for (item in array) {
        val path = (item as? JsonPrimitive)?.takeIf { it.isString }?.content?.trim()
        if (path.isNullOrEmpty() || path in out) continue
        out += path
        if (out.size >= SESSION_RESULT_FILES_MAX) break
    }
    return out
}

/**
 * EXP-1172: a topic with more than one picture moves its inline ones into
 * [SessionResultGroup.earlier], so the final report leads; a topic's only
 * picture stays.
 */
private fun SessionResultGroup.foldInline(): SessionResultGroup {
    if (entries.size < 2) return this
    return copy(entries = entries.filterNot { it.inline }, earlier = entries.filter { it.inline })
}

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
private fun records(raw: String?): List<JsonObject> {
    val trimmed = raw?.trim()?.takeIf { it.isNotEmpty() } ?: return emptyList()
    val array = runCatching { resultsJson.parseToJsonElement(trimmed) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return array.mapNotNull { it as? JsonObject }
}

private fun picture(row: JsonObject): SessionResultEntry? {
    val topic = row.text("topic") ?: return null
    val label = row.text("label") ?: return null
    val attachmentId = row.text("attachmentId") ?: return null
    return SessionResultEntry(
        topic = topic,
        label = label,
        attachmentId = attachmentId,
        width = row.dimension("width"),
        height = row.dimension("height"),
        inline = (row["inline"] as? JsonPrimitive)?.let { !it.isString && it.content == "true" } == true,
        caption = row.text("caption"),
    )
}

fun parseSessionResults(raw: String?): List<SessionResultEntry> {
    val entries = mutableListOf<SessionResultEntry>()
    for (row in records(raw)) {
        entries += picture(row) ?: continue
        if (entries.size >= MAX_SESSION_RESULTS) break
    }
    return entries
}

/**
 * EXP-933: the Results face as a REPORT — pictures AND each topic's text
 * (`{topic, label: null, attachmentId: null, text}`), grouped in FIRST-SEEN
 * topic order whichever kind opened the topic. A topic's text is its FIRST
 * non-blank text entry, trimmed; pictures keep the 60 cap. Fixture:
 * `packages/domain-contract/fixtures/session-results.json` (×4).
 */
fun parseSessionResultGroups(raw: String?): List<SessionResultGroup> {
    val order = mutableListOf<String>()
    val entries = linkedMapOf<String, MutableList<SessionResultEntry>>()
    val texts = mutableMapOf<String, String>()
    val files = mutableMapOf<String, List<String>>()
    fun open(topic: String): MutableList<SessionResultEntry> =
        entries.getOrPut(topic) {
            order += topic
            mutableListOf()
        }
    var pictures = 0
    for (row in records(raw)) {
        val entry = picture(row)
        if (entry != null) {
            if (pictures >= MAX_SESSION_RESULTS) continue
            pictures += 1
            open(entry.topic) += entry
            continue
        }
        val topic = row.text("topic") ?: continue
        val body = row.text("text") ?: continue
        open(topic)
        if (topic !in texts) {
            texts[topic] = body
            files[topic] = row.resultFiles()
        }
    }
    return order.map { topic ->
        SessionResultGroup(
            topic = topic,
            entries = entries.getValue(topic).toList(),
            text = texts[topic],
            files = files[topic].orEmpty(),
        ).foldInline()
    }
}

// EXP-1154: the Results face as the GUIDE (Linear's shape): the `Summary`
// topic leads as a plain paragraph, every other topic is a numbered section
// (`01 / 04`) with its text, the files it touched and its pictures. Fixture
// `session-results.json` `guide` (x4, web `session-results.ts`).

/** The topic that leads the Guide unnumbered. */
const val SESSION_RESULTS_SUMMARY_TOPIC = "Summary"

/** True for the Summary topic: trimmed, case-insensitive. */
fun isSummaryTopic(topic: String): Boolean =
    topic.trim().equals(SESSION_RESULTS_SUMMARY_TOPIC, ignoreCase = true)

/** One numbered Guide section: [index] is 1-based, the lead never counts. */
data class GuideSection<G>(val group: G, val index: Int, val total: Int)

/** The lead (the FIRST Summary group wherever it sits) and the numbered rest. */
data class SessionResultsGuide<G>(val lead: G?, val sections: List<GuideSection<G>>)

/** Splits [groups] into the Guide's lead and its numbered sections, in order. */
fun <G> sessionResultsGuide(groups: List<G>, topic: (G) -> String): SessionResultsGuide<G> {
    val leadIndex = groups.indexOfFirst { isSummaryTopic(topic(it)) }
    val rest = groups.filterIndexed { index, _ -> index != leadIndex }
    return SessionResultsGuide(
        lead = groups.getOrNull(leadIndex),
        sections = rest.mapIndexed { index, group -> GuideSection(group, index + 1, rest.size) },
    )
}

/** The Results groups as the Guide. */
fun sessionResultsGuide(groups: List<SessionResultGroup>): SessionResultsGuide<SessionResultGroup> =
    sessionResultsGuide(groups) { it.topic }

/** `01 / 04`: both numbers two-digit zero-padded. */
fun guideSectionCaption(index: Int, total: Int): String =
    "${index.toString().padStart(2, '0')} / ${total.toString().padStart(2, '0')}"

/** One file row of a Guide section; [counts] null for a path the diff does not list. */
data class GuideFileRow(val path: String, val counts: GuideFileCounts?)

/** A matched file's `+N −M`. */
data class GuideFileCounts(val additions: Int, val deletions: Int)

/**
 * One row per path in order; `+N −M` from the diff file whose path matches
 * EXACTLY, else null (an unknown path, or no diff loaded).
 */
fun guideFileRows(paths: List<String>, files: List<Diff.File>?): List<GuideFileRow> =
    paths.map { path ->
        val file = files?.firstOrNull { it.path == path }
        GuideFileRow(path, file?.let { GuideFileCounts(it.additions, it.deletions) })
    }

/** True when the blob has anything for the Results face to show. */
fun hasSessionResults(raw: String?): Boolean = parseSessionResultGroups(raw).isNotEmpty()

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
    return order.map { topic -> SessionResultGroup(topic, byTopic.getValue(topic).toList()).foldInline() }
}

/**
 * EXP-1172: the picture an `exponential_sessions_show` call filed, by the
 * attachment id its answer carried (`preview.id`); null while the upload is
 * still in flight, once it was removed, or for a blank id.
 */
fun sessionResultPicture(raw: String?, attachmentId: String?): SessionResultEntry? {
    val id = attachmentId?.trim()?.takeIf { it.isNotEmpty() } ?: return null
    return parseSessionResults(raw).firstOrNull { it.attachmentId == id }
}

/** EXP-1172: the line under a transcript tile — the show call's caption, else the picture's label. */
fun sessionResultTileCaption(entry: SessionResultEntry): String = entry.caption ?: entry.label

/**
 * EXP-1128: true when the probed aspect is under [SESSION_RESULT_TALL_ASPECT];
 * an unmeasured picture is never tall.
 */
fun sessionResultIsTall(entry: SessionResultEntry): Boolean {
    val width = entry.width ?: return false
    val height = entry.height ?: return false
    return width.toDouble() / height.toDouble() < SESSION_RESULT_TALL_ASPECT
}

/**
 * The tile's width at a fixed [height] — the probed aspect, else 4:3 (a
 * desktop screenshot's shape, and the least surprising placeholder). A TALL
 * picture (EXP-1128) takes the 4:3 frame too: the tile shows its top, never a
 * sliver.
 */
fun sessionResultTileWidth(
    entry: SessionResultEntry,
    height: Int = SESSION_RESULT_TILE_HEIGHT,
): Int {
    val width = entry.width
    val entryHeight = entry.height
    val aspect = if (width != null && entryHeight != null && !sessionResultIsTall(entry)) {
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

/**
 * EXP-1128: a tall picture's decode STRIPS as `(y, rows)` source-pixel
 * ranges of at most [rows] each, the last one shorter — the viewer decodes
 * one region per strip so no bitmap crosses the texture limit. Zero or a
 * negative height is empty.
 */
fun tallImageStripRanges(height: Int, rows: Int = 4096): List<Pair<Int, Int>> {
    if (height <= 0 || rows <= 0) return emptyList()
    val ranges = mutableListOf<Pair<Int, Int>>()
    var y = 0
    while (y < height) {
        val chunk = minOf(rows, height - y)
        ranges += y to chunk
        y += chunk
    }
    return ranges
}
