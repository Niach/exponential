package com.exponential.app.domain

// The steer message composed from typed text plus attached images (EXP-511):
// one string, byte-identical across web (lib/steer-image-message.ts), iOS
// (SteerImageMessage.swift) and here. The host rewrites each embed token to a
// local file path before the agent sees it and reverse-rewrites the echoed
// transcript back, so drift in this format breaks echo dedupe.

const val MAX_STEER_IMAGES = 4

/** Wave D ×4: non-image files ride the same message, capped separately
 *  (4 × [MAX_FILE_UPLOAD_BYTES]); images keep their own four. */
const val MAX_STEER_FILES = 4

/**
 * Wave D ×4: one non-image attachment on the wire — the upload's id and the
 * server's sanitized `filename` (the upload response), which becomes the link
 * text of its `[<name>](/api/attachments/<id>)` line.
 */
data class SteerFile(val id: String, val name: String)

/** The ONE rejection copy ×4 for a pick the steer/start composers refuse. */
const val STEER_ATTACHMENT_REJECTED = "Images up to 10 MB and files up to 50 MB can be attached"

/** The file cap toast ×4. */
const val STEER_FILES_CAP = "Up to 4 files per message"

/** The image cap copy (unchanged). */
const val STEER_IMAGES_CAP = "Up to $MAX_STEER_IMAGES images per message"

/**
 * The device cap a host (desktop IDE + CLI daemon 0.14.66+) advertises when
 * it localizes non-image file attachments in start prompts and steer
 * messages (release train 2026-10-10, F6). A device without a caps field
 * lacks it.
 */
const val DEVICE_CAP_STEER_FILES = "steer-files"

fun deviceAcceptsSteerFiles(caps: List<String>?): Boolean = caps?.contains(DEVICE_CAP_STEER_FILES) == true

/**
 * Why a pick cannot join the pending set FOR THE CHOSEN DEVICE, or null when
 * it can: a non-image file bound for a device without [DEVICE_CAP_STEER_FILES]
 * is refused with the server's own sentence (contract
 * `composerUi.filesNeedNewerDevice`); images always pass. Both the start and
 * the steer composer apply it before the size/count caps.
 */
fun steerFilePickRefusal(isImage: Boolean, deviceCaps: List<String>?): String? =
    if (isImage || deviceAcceptsSteerFiles(deviceCaps)) null else DomainContract.composerUiFilesNeedNewerDevice

/**
 * Wave D ×4: why a pick cannot join the pending set, or null when it can.
 * An inline image counts against [MAX_STEER_IMAGES] × [MAX_IMAGE_UPLOAD_BYTES];
 * anything else is a file against [MAX_STEER_FILES] × [MAX_FILE_UPLOAD_BYTES].
 * Size refusals share ONE copy ([STEER_ATTACHMENT_REJECTED]).
 */
fun steerAttachmentRefusal(
    imageCount: Int,
    fileCount: Int,
    isImage: Boolean,
    sizeBytes: Long,
): String? = if (isImage) {
    when {
        sizeBytes > MAX_IMAGE_UPLOAD_BYTES -> STEER_ATTACHMENT_REJECTED
        imageCount >= MAX_STEER_IMAGES -> STEER_IMAGES_CAP
        else -> null
    }
} else {
    when {
        sizeBytes > MAX_FILE_UPLOAD_BYTES -> STEER_ATTACHMENT_REJECTED
        fileCount >= MAX_STEER_FILES -> STEER_FILES_CAP
        else -> null
    }
}

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
private val EMBED_LINE = Regex("""(?U)^!\[image]\(/api/attachments/([^)\s]+)\)$""")

/**
 * One file line, exactly as [buildSteerMessage] writes it: a plain link (no
 * `!`), its text the filename with `]` and `\` backslash-escaped.
 */
private val FILE_LINE = Regex("""(?U)^\[((?:[^\]\\]|\\.)*)]\(/api/attachments/([^)\s]+)\)$""")

/** Runs of spaces/tabs a removed marker leaves behind. */
private val SPACE_RUN = Regex("""[ \t]{2,}""")

fun buildSteerImageMessage(text: String, attachmentIds: List<String>): String {
    val trimmed = text.trim()
    if (attachmentIds.isEmpty()) return trimmed
    val embeds = attachmentIds.joinToString("\n") { "![image](/api/attachments/$it)" }
    if (trimmed.isEmpty()) return embeds
    return "$trimmed\n\n$embeds"
}

/** A filename as link text: `\` and `]` backslash-escaped (×4 contract). */
fun escapeSteerFileName(name: String): String =
    name.replace("\\", "\\\\").replace("]", "\\]")

/** The inverse of [escapeSteerFileName]: any `\x` reads as `x`. */
private fun unescapeSteerFileName(text: String): String {
    val out = StringBuilder(text.length)
    var i = 0
    while (i < text.length) {
        val c = text[i]
        if (c == '\\' && i + 1 < text.length) {
            out.append(text[i + 1])
            i += 2
        } else {
            out.append(c)
            i++
        }
    }
    return out.toString()
}

/**
 * Wave D ×4: prose, blank line, the image embed block exactly as
 * [buildSteerImageMessage] writes it (frozen), then one
 * `[<filename>](/api/attachments/<id>)` line per file. Files carry no
 * positional marker. With no files this IS [buildSteerImageMessage].
 */
fun buildSteerMessage(text: String, imageIds: List<String>, files: List<SteerFile>): String {
    val withImages = buildSteerImageMessage(text, imageIds)
    if (files.isEmpty()) return withImages
    val links = files.joinToString("\n") { "[${escapeSteerFileName(it.name)}](/api/attachments/${it.id})" }
    if (withImages.isEmpty()) return links
    return if (imageIds.isEmpty()) "$withImages\n\n$links" else "$withImages\n$links"
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
    /** Wave D: the trailing file lines, in order (peeled before the images). */
    val files: List<SteerFile> = emptyList(),
)

fun parseSteerMessage(message: String): ParsedSteerMessage {
    val lines = message.split("\n")
    var end = lines.size
    while (end > 0 && lines[end - 1].isBlank()) end--
    val files = ArrayDeque<SteerFile>()
    while (end > 0) {
        val match = FILE_LINE.matchEntire(lines[end - 1].trim()) ?: break
        files.addFirst(SteerFile(id = match.groupValues[2], name = unescapeSteerFileName(match.groupValues[1])))
        end--
    }
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
        files = files.toList(),
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
