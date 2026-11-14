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

    /**
     * Removing the [removedIndex]-th image renumbers the draft: its own
     * markers go, and every higher one slides down one. Only a line that LOST
     * a marker gets the gap it left tidied; untouched lines keep their spacing.
     */
    fun renumber(text: String, removedIndex: Int): String = renumberWithCaret(text, removedIndex, 0).text

    /** Collapses each run of 2+ spaces/tabs to one space, mapping [at]: a
     *  caret inside a run (past its first character) lands behind the space. */
    private fun collapseRuns(line: String, at: Int): Placed {
        val out = StringBuilder()
        var mapped = if (at >= line.length) -1 else at
        var i = 0
        while (i < line.length) {
            val ch = line[i]
            if (ch == ' ' || ch == '\t') {
                var j = i
                while (j < line.length && (line[j] == ' ' || line[j] == '\t')) j++
                if (j - i >= 2) {
                    if (at == i) mapped = out.length
                    else if (at > i && at <= j) mapped = out.length + 1
                    out.append(' ')
                    i = j
                    continue
                }
            }
            if (at == i) mapped = out.length
            out.append(ch)
            i++
        }
        return Placed(out.toString(), if (mapped == -1) out.length else mapped)
    }

    /**
     * [renumber] with the caret carried along (the X keeps it where it was):
     * it moves left by what the removal took out before it, so a caret right
     * behind the removed placeholder lands where it began. A caret INSIDE a
     * marker counts as at that marker's start. Mirrors `renumberWithCaret`.
     */
    fun renumberWithCaret(text: String, removedIndex: Int, caret: Int): Placed {
        var at = caret.coerceIn(0, text.length)
        for (match in IMAGE_MARKER_REGEX.findAll(text)) {
            val start = match.range.first
            if (at > start && at < match.range.last + 1) at = start
        }
        val out = mutableListOf<String>()
        var lineStart = 0
        var mappedCaret = 0
        var outOffset = 0
        for (line in text.split("\n")) {
            val local = if (at >= lineStart && at <= lineStart + line.length) at - lineStart else -1
            var dropped = false
            val next = StringBuilder()
            var cursor = 0
            var mapped = -1
            for (match in IMAGE_MARKER_REGEX.findAll(line)) {
                val start = match.range.first
                if (local in cursor..start && mapped == -1) mapped = next.length + (local - cursor)
                next.append(line, cursor, start)
                val raw = imageMarkerNumber(match)
                when {
                    // Not a number this platform can carry: prose, verbatim.
                    raw == null -> next.append(match.value)
                    raw == removedIndex.toLong() -> dropped = true
                    raw > removedIndex -> next.append(imageMarker(raw - 1))
                    else -> next.append(match.value)
                }
                cursor = match.range.last + 1
                if (local == cursor && mapped == -1) mapped = next.length
            }
            if (local >= cursor && mapped == -1) mapped = next.length + (local - cursor)
            next.append(line, cursor, line.length)
            var result = next.toString()
            if (dropped) {
                val collapsed = collapseRuns(result, mapped)
                result = collapsed.text.trimEnd(' ', '\t')
                mapped = minOf(collapsed.caret, result.length)
                if (line.startsWith(imageMarker(removedIndex))) {
                    val trimmed = result.trimStart(' ', '\t')
                    mapped = maxOf(0, mapped - (result.length - trimmed.length))
                    result = trimmed
                }
            }
            if (local != -1 && mapped != -1) mappedCaret = outOffset + mapped
            out.add(result)
            outOffset += result.length + 1
            lineStart += line.length + 1
        }
        return Placed(out.joinToString("\n"), mappedCaret)
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

    /** The strip after the X, the draft, and the caret (where it was,
     *  moved left by what left before it). */
    data class Dropped(val strip: List<Kind>, val text: String, val caret: Int)

    /** The strip's X: an image's markers leave the draft and every higher one
     *  slides down; a file has none. Out of range = no-op. The caret stays
     *  where it was ([renumberWithCaret]). */
    fun drop(strip: List<Kind>, at: Int, text: String, caret: Int): Dropped {
        val kept = caret.coerceIn(0, text.length)
        val kind = strip.getOrNull(at) ?: return Dropped(strip, text, kept)
        val next = strip.filterIndexed { index, _ -> index != at }
        if (kind != Kind.Image) return Dropped(next, text, kept)
        val number = imageCount(strip.take(at)) + 1
        val placed = renumberWithCaret(text, number, kept)
        return Dropped(next, placed.text, placed.caret)
    }

    /** The toasts a stage owes, in order. */
    fun toasts(rejected: Int, overflow: Int, fileOverflow: Int): List<String> = buildList {
        if (rejected > 0) add(REJECTED_TOAST)
        if (overflow > 0) add(IMAGE_CAP_TOAST)
        if (fileOverflow > 0) add(FILE_CAP_TOAST)
    }
}
