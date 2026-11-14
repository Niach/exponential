package com.exponential.app.domain

/**
 * #EXP-1841 (fixtures/composer-attachments.json): THE composer's attachment
 * strip and its `[Image #N]` placeholders (EXP-698), the same in EVERY
 * composer ×4 (a new chat's, a session's, an agent chat's, the Assistant's,
 * an app editor's). Pure: the strip is a list of kinds; classifying a picked
 * file ([classify]) is the platform's, so [stage] takes picks already
 * classified. Mirrors the TS reference `src/composer-attachments.ts`.
 */
object ComposerAttachments {

    enum class Kind { Image, File }

    enum class PickKind { Image, File, Rejected }

    /** Images and files each cap at four per message. */
    const val MAX_IMAGES: Int = 4
    const val MAX_FILES: Int = 4

    /** The rejection toasts ×4. */
    const val REJECTED_TOAST = "Images up to 10 MB and files up to 50 MB can be attached"
    const val IMAGE_CAP_TOAST = "Up to $MAX_IMAGES images per message"
    const val FILE_CAP_TOAST = "Up to $MAX_FILES files per message"

    /** An accepted image type up to 10 MB = image, any other file up to
     *  50 MB = file, empty or over its cap = rejected. */
    fun classify(isImage: Boolean, sizeBytes: Long): PickKind = when {
        sizeBytes <= 0L -> PickKind.Rejected
        isImage -> if (sizeBytes > MAX_IMAGE_UPLOAD_BYTES) PickKind.Rejected else PickKind.Image
        else -> if (sizeBytes > MAX_FILE_UPLOAD_BYTES) PickKind.Rejected else PickKind.File
    }

    /** A draft and the caret in it. */
    data class Placed(val text: String, val caret: Int)

    /** Drops `[Image #index]` at [caret], space-separated from whatever it
     *  lands against; the caret moves behind the insertion. */
    fun insert(text: String, caret: Int, index: Int): Placed {
        val at = caret.coerceIn(0, text.length)
        val before = text.substring(0, at)
        val after = text.substring(at)
        val marker = imageMarker(index)
        val lead = if (before.isNotEmpty() && !before.last().isWhitespace()) " " else ""
        val trail = if (after.isNotEmpty() && !after.first().isWhitespace()) " " else ""
        return Placed("$before$lead$marker$trail$after", at + lead.length + marker.length + trail.length)
    }

    /** Runs of spaces/tabs a removed marker leaves behind. */
    private val SPACE_RUN = Regex("""[ \t]{2,}""")

    /**
     * Removing the [removedIndex]-th image renumbers the draft: its own
     * markers go, and every higher one slides down one. Only a line that LOST
     * a marker gets the gap it left tidied; untouched lines keep their spacing.
     */
    fun renumber(text: String, removedIndex: Int): String =
        text.split("\n").joinToString("\n") { line ->
            var dropped = false
            val next = StringBuilder()
            var cursor = 0
            for (match in IMAGE_MARKER_REGEX.findAll(line)) {
                next.append(line, cursor, match.range.first)
                cursor = match.range.last + 1
                val raw = imageMarkerNumber(match)
                when {
                    // Not a number this platform can carry: prose, verbatim
                    // (the same call every marker walker makes).
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

    data class Staged(
        /** The strip after the add. */
        val strip: List<Kind>,
        /** For each picked entry: true when it joined the strip. */
        val taken: List<Boolean>,
        /** The draft with one marker per TAKEN image at the caret. */
        val text: String,
        /** Behind the last inserted marker (unchanged without one). */
        val caret: Int,
        val rejected: Int,
        val overflow: Int,
        val fileOverflow: Int,
    ) {
        /** The toasts this stage owes, in order. */
        val toasts: List<String> get() = toasts(rejected, overflow, fileOverflow)
    }

    fun imageCount(strip: List<Kind>): Int = strip.count { it == Kind.Image }

    /** Stages a pick onto the strip, in pick order. The image count BEFORE
     *  the add is the numbering base (files never count). */
    fun stage(strip: List<Kind>, add: List<PickKind>, text: String, caret: Int): Staged {
        val base = imageCount(strip)
        var imageRoom = maxOf(0, MAX_IMAGES - base)
        var fileRoom = maxOf(0, MAX_FILES - (strip.size - base))
        var rejected = 0
        var overflow = 0
        var fileOverflow = 0
        val next = strip.toMutableList()
        val taken = mutableListOf<Boolean>()
        var placed = Placed(text, caret.coerceIn(0, text.length))
        var number = base
        for (kind in add) {
            when (kind) {
                PickKind.Rejected -> {
                    rejected++
                    taken.add(false)
                }
                PickKind.Image -> if (imageRoom == 0) {
                    overflow++
                    taken.add(false)
                } else {
                    imageRoom--
                    next.add(Kind.Image)
                    taken.add(true)
                    number++
                    placed = insert(placed.text, placed.caret, number)
                }
                PickKind.File -> if (fileRoom == 0) {
                    fileOverflow++
                    taken.add(false)
                } else {
                    fileRoom--
                    next.add(Kind.File)
                    taken.add(true)
                }
            }
        }
        return Staged(next, taken, placed.text, placed.caret, rejected, overflow, fileOverflow)
    }

    data class Dropped(val strip: List<Kind>, val text: String)

    /** The strip's X: an image's markers leave the draft and every higher one
     *  slides down; a file has none. Out of range = no-op. */
    fun drop(strip: List<Kind>, at: Int, text: String): Dropped {
        val kind = strip.getOrNull(at) ?: return Dropped(strip, text)
        val next = strip.filterIndexed { index, _ -> index != at }
        if (kind != Kind.Image) return Dropped(next, text)
        val number = imageCount(strip.take(at)) + 1
        return Dropped(next, renumber(text, number))
    }

    /** The toasts a stage owes, in order. */
    fun toasts(rejected: Int, overflow: Int, fileOverflow: Int): List<String> = buildList {
        if (rejected > 0) add(REJECTED_TOAST)
        if (overflow > 0) add(IMAGE_CAP_TOAST)
        if (fileOverflow > 0) add(FILE_CAP_TOAST)
    }
}
