package com.exponential.app.domain

// The steer message composed from typed text plus attached images (EXP-511):
// one string, byte-identical across web (lib/steer-image-message.ts), iOS
// (SteerImageMessage.swift) and here. The host rewrites each embed token to a
// local file path before the agent sees it and reverse-rewrites the echoed
// transcript back, so drift in this format breaks echo dedupe.

const val MAX_STEER_IMAGES = 4

// EXP-698: a POSITIONAL reference to one of the message's images. The composer
// drops `[Image #k]` at the caret when the k-th image is attached, so the agent
// reads "crop [Image #2]" instead of guessing which embed a sentence means. The
// marker is plain text on the wire — the embeds below the text stay the only
// image payload — and the viewer renders it as a chip.
val IMAGE_MARKER_REGEX = Regex("""\[Image #(\d+)\]""")

fun imageMarker(index: Long): String = "[Image #$index]"

fun imageMarker(index: Int): String = imageMarker(index.toLong())

/**
 * The number inside a matched marker, or null when the digits do not fit a
 * [Long]. EXP-698: ALL THREE walkers below go through this one parse, so a
 * marker is either a marker everywhere or prose everywhere — the three used to
 * disagree (`markers` dropped an oversize one, `renumber` kept it verbatim),
 * which is exactly how a draft and its rendering drift apart. Web parses with
 * `Number()`, which never fails; 19 digits is past anything a composer can
 * produce, and beyond it every walker agrees the token is plain text.
 */
private fun markerNumber(match: MatchResult): Long? = match.groupValues[1].toLongOrNull()

/** One embed line, exactly as [buildSteerImageMessage] writes it. */
private val EMBED_LINE = Regex("""^!\[image]\(/api/attachments/([^)\s]+)\)$""")

/** Runs of spaces/tabs a removed marker leaves behind. */
private val SPACE_RUN = Regex("""[ \t]{2,}""")

fun buildSteerImageMessage(text: String, attachmentIds: List<String>): String {
    val trimmed = text.trim()
    if (attachmentIds.isEmpty()) return trimmed
    val embeds = attachmentIds.joinToString("\n") { "![image](/api/attachments/$it)" }
    if (trimmed.isEmpty()) return embeds
    return "$trimmed\n\n$embeds"
}

/**
 * The inverse of [buildSteerImageMessage]: the prose without its trailing embed
 * block, the attachment ids in embed order (image #1 is `attachmentIds[0]`),
 * and the `[Image #N]` numbers the prose carries — 1-based, in text order,
 * deduped. A number with no matching embed is still reported; the viewer
 * decides what to do with a dangling reference.
 */
data class ParsedSteerMessage(
    val text: String,
    val attachmentIds: List<String>,
    val markers: List<Long>,
)

fun parseSteerMessage(message: String): ParsedSteerMessage {
    val lines = message.split("\n")
    var end = lines.size
    while (end > 0 && lines[end - 1].isBlank()) end--
    val attachmentIds = ArrayDeque<String>()
    while (end > 0) {
        val match = EMBED_LINE.matchEntire(lines[end - 1].trim()) ?: break
        attachmentIds.addFirst(match.groupValues[1])
        end--
    }
    val text = lines.subList(0, end).joinToString("\n").trimEnd()
    return ParsedSteerMessage(
        text = text,
        attachmentIds = attachmentIds.toList(),
        markers = steerImageMarkers(text),
    )
}

/** The `[Image #N]` numbers a draft carries, 1-based, in text order, deduped. */
fun steerImageMarkers(text: String): List<Long> {
    val found = mutableListOf<Long>()
    for (match in IMAGE_MARKER_REGEX.findAll(text)) {
        val index = markerNumber(match) ?: continue
        if (index !in found) found.add(index)
    }
    return found
}

/**
 * The prose split on its `[Image #N]` markers, in order — what a viewer walks
 * to render each marker as a chip inline with the words around it. Empty text
 * runs are dropped; the marker numbers are NOT deduped here (each occurrence is
 * its own chip).
 */
sealed interface SteerMessageSegment {
    data class Text(val text: String) : SteerMessageSegment
    data class Marker(val index: Long) : SteerMessageSegment
}

fun steerMessageSegments(text: String): List<SteerMessageSegment> {
    val result = mutableListOf<SteerMessageSegment>()
    var cursor = 0
    for (match in IMAGE_MARKER_REGEX.findAll(text)) {
        // Unparseable digits are PROSE: leaving the cursor where it is folds
        // the literal token into the following text run, which is what the
        // other two walkers do with it too.
        val index = markerNumber(match) ?: continue
        if (match.range.first > cursor) {
            result.add(SteerMessageSegment.Text(text.substring(cursor, match.range.first)))
        }
        result.add(SteerMessageSegment.Marker(index))
        cursor = match.range.last + 1
    }
    if (cursor < text.length) result.add(SteerMessageSegment.Text(text.substring(cursor)))
    return result
}

/**
 * Drops `[Image #index]` at [caret], space-separated from whatever it lands
 * against. Returns the new draft and the caret behind the insertion.
 */
fun insertImageMarker(text: String, caret: Int, index: Int): Pair<String, Int> {
    val at = caret.coerceIn(0, text.length)
    val before = text.substring(0, at)
    val after = text.substring(at)
    val marker = imageMarker(index)
    val lead = if (before.isNotEmpty() && !before.last().isWhitespace()) " " else ""
    val trail = if (after.isNotEmpty() && !after.first().isWhitespace()) " " else ""
    return "$before$lead$marker$trail$after" to (at + lead.length + marker.length + trail.length)
}

/**
 * Removing the [removedIndex]-th pending image renumbers the draft: its own
 * markers go, and every higher one slides down one. Only a line that LOST a
 * marker gets the gap it left tidied — untouched lines keep their spacing.
 */
fun renumberImageMarkers(text: String, removedIndex: Int): String =
    text.split("\n").joinToString("\n") { line ->
        var dropped = false
        val next = StringBuilder()
        var cursor = 0
        for (match in IMAGE_MARKER_REGEX.findAll(line)) {
            next.append(line, cursor, match.range.first)
            cursor = match.range.last + 1
            val raw = markerNumber(match)
            when {
                // Not a number this platform can carry: prose, left verbatim
                // — the same call the other two walkers make.
                raw == null -> next.append(match.value)
                raw == removedIndex.toLong() -> dropped = true
                raw > removedIndex -> next.append(imageMarker(raw - 1))
                else -> next.append(match.value)
            }
        }
        next.append(line, cursor, line.length)
        if (!dropped) {
            next.toString()
        } else {
            val tidied = SPACE_RUN.replace(next, " ").trimEnd(' ', '\t')
            if (line.startsWith(imageMarker(removedIndex))) tidied.trimStart(' ', '\t') else tidied
        }
    }
