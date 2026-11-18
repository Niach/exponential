package at.exponential.ui.primitives

import androidx.compose.ui.graphics.toArgb
import java.text.Normalizer
import java.util.Locale

// #EDE-95 (fixtures/text-highlight.json): search-match marks on genUI text
// (Text, Markdown, MessageBubble, Row `highlight`), a port of
// packages/exponential-ui/src/highlight.ts. A match is the query found
// LITERALLY in the text after folding both per Unicode scalar: canonical
// decomposition (NFD), lower case (root locale), combining marks dropped
// (U+0300–036F, 1AB0–1AFF, 1DC0–1DFF, 20D0–20FF, FE20–FE2F). Matches run
// left to right and never overlap; a match ends after the scalars its last
// folded character came from plus the combining marks right after them. A
// query that folds to nothing matches nothing. Offsets are UTF-16 (String).

/** #EDE-95: the `highlight` prop (catalog defs `highlight`). */
data class HighlightSpec(val query: String, val current: Int? = null, val before: String? = null)

/** A chip (or any piece that never matches) inside a searched text: no query contains it. */
const val HIGHLIGHT_BARRIER: Char = '￼'

private fun isMark(cp: Int): Boolean =
    cp in 0x300..0x36f || cp in 0x1ab0..0x1aff || cp in 0x1dc0..0x1dff || cp in 0x20d0..0x20ff || cp in 0xfe20..0xfe2f

private class Folded(val chars: IntArray, val from: IntArray, val to: IntArray)

/** The folded text (code points) with, per folded character, the UTF-16 span of the scalar it came from. */
private fun fold(text: String): Folded {
    val chars = ArrayList<Int>(text.length)
    val from = ArrayList<Int>(text.length)
    val to = ArrayList<Int>(text.length)
    var at = 0
    while (at < text.length) {
        val cp = text.codePointAt(at)
        val end = at + Character.charCount(cp)
        val folded = Normalizer.normalize(text.substring(at, end), Normalizer.Form.NFD).lowercase(Locale.ROOT)
        folded.codePoints().forEach { c ->
            if (!isMark(c)) {
                chars += c
                from += at
                to += end
            }
        }
        at = end
    }
    return Folded(chars.toIntArray(), from.toIntArray(), to.toIntArray())
}

/** The query as folded characters (a barrier never matches). */
private fun foldQuery(query: String): IntArray = fold(query).chars.filter { it != HIGHLIGHT_BARRIER.code }.toIntArray()

/** Every match of [query] in [text] (UTF-16, end exclusive), left to right, never overlapping. */
fun highlightRanges(text: String, query: String): List<IntRange> {
    val q = foldQuery(query)
    if (q.isEmpty() || text.isEmpty()) return emptyList()
    val f = fold(text)
    val out = ArrayList<IntRange>()
    var i = 0
    while (i + q.size <= f.chars.size) {
        var hit = true
        for (k in q.indices) {
            if (f.chars[i + k] != q[k]) {
                hit = false
                break
            }
        }
        if (!hit) {
            i++
            continue
        }
        var end = f.to[i + q.size - 1]
        // The combining marks right after the match (folded to nothing) join it.
        while (end < text.length) {
            val cp = text.codePointAt(end)
            if (!isMark(cp)) break
            end += Character.charCount(cp)
        }
        out.add(f.from[i] until end)
        i += q.size
    }
    return out
}

/** How many matches [query] has in [text]. */
fun highlightCount(text: String, query: String): Int = highlightRanges(text, query).size

/** A spec whose query folds to something (else null). */
fun highlightSpecOf(query: String?, current: Double?, before: String?): HighlightSpec? {
    if (query == null || foldQuery(query).isEmpty()) return null
    val cur = current?.takeIf { it.isFinite() }?.let { kotlin.math.floor(it).toInt() }
    return HighlightSpec(query, cur, before?.takeIf { it.isNotEmpty() })
}

/** The index among THIS node's matches painted current (none = -1): `current` minus the matches of `before`. */
fun highlightLocalCurrent(spec: HighlightSpec): Int {
    val current = spec.current ?: return -1
    if (current < 0) return -1
    val local = current - (spec.before?.let { highlightCount(it, spec.query) } ?: 0)
    return if (local >= 0) local else -1
}

/** A run list's searched text: the non-empty runs joined, a chip as ONE [HIGHLIGHT_BARRIER]. */
fun markdownHighlightText(inlines: List<MarkdownInline>): String = buildString {
    for (run in inlines) {
        if (run.text.isEmpty()) continue
        if (run.chip != null) append(HIGHLIGHT_BARRIER) else append(run.text)
    }
}

/**
 * The run lists a Markdown searches, one per block of the normal form in
 * document order: a paragraph, heading, list item or quote; each table
 * cell (the header first, row by row); never a code block, image,
 * attachment or rule.
 */
fun markdownHighlightGroups(blocks: List<MarkdownBlock>): List<List<MarkdownInline>> {
    val out = ArrayList<List<MarkdownInline>>()
    for (block in blocks) {
        when (val k = block.kind) {
            MarkdownBlockKind.Paragraph, MarkdownBlockKind.Quote, is MarkdownBlockKind.Heading, is MarkdownBlockKind.ListItem -> out.add(block.inlines)
            is MarkdownBlockKind.Table -> {
                out.addAll(k.header)
                for (row in k.rows) out.addAll(row)
            }
            else -> Unit
        }
    }
    return out
}

/** A Markdown's matches (what a host numbers). */
fun markdownHighlightCount(blocks: List<MarkdownBlock>, query: String): Int =
    markdownHighlightGroups(blocks).sumOf { highlightCount(markdownHighlightText(it), query) }

/** One painted mark in a run list's searched text ([markdownHighlightText] offsets). */
data class HighlightMark(val start: Int, val end: Int, val current: Boolean)

/** #EDE-95: a [MarkdownView]'s marks: the query, the LOCAL current index (-1 = none) and the two fills. */
data class MarkdownHighlight(
    val query: String,
    val current: Int = -1,
    val match: androidx.compose.ui.graphics.Color,
    val matchCurrent: androidx.compose.ui.graphics.Color,
)

/** Every block's marks: per block, per searched run list of it (in [markdownHighlightGroups] order), the marks. */
internal fun markdownHighlightMarks(blocks: List<MarkdownBlock>, query: String, current: Int): List<List<List<HighlightMark>>> {
    var n = 0
    return blocks.map { block ->
        markdownHighlightGroups(listOf(block)).map { group ->
            highlightRanges(markdownHighlightText(group), query).map { r -> HighlightMark(r.first, r.last + 1, n++ == current) }
        }
    }
}

/**
 * #EDE-95: the annotation tag a painted text carries over each mark (or each
 * piece of one, per run); its value = [highlightMarkValue]. The fills are
 * drawn behind the text ([drawHighlightFills]), never as a span background
 * (that spans the line height): layout never changes.
 */
const val HIGHLIGHT_MARK_TAG: String = "exp-highlight-mark"

/** A mark annotation's value: its fill (ARGB) and whether the piece is inline code (the code font's box). */
fun highlightMarkValue(fill: androidx.compose.ui.graphics.Color, code: Boolean = false): String =
    "${fill.toArgb()}${if (code) ":code" else ""}"

/** The fill of a [highlightMarkValue]. */
fun highlightMarkFill(value: String): androidx.compose.ui.graphics.Color =
    androidx.compose.ui.graphics.Color(value.substringBefore(':').toInt())

/** Whether a [highlightMarkValue] marks inline code. */
fun highlightMarkIsCode(value: String): Boolean = value.endsWith(":code")

/**
 * THE FILL BOX (`fixtures/text-highlight.json`): the glyph box of [start, end)
 * in [layout], one rect per line it crosses: horizontally the run (its first
 * character's leading edge to its last visible character's right edge),
 * vertically the line's baseline − [ascent] .. baseline + [descent] (the
 * font's own, px), never the line height; inline code's fill box without
 * its padding.
 */
fun highlightFillRects(layout: androidx.compose.ui.text.TextLayoutResult, start: Int, end: Int, ascent: Float, descent: Float): List<androidx.compose.ui.geometry.Rect> {
    val n = layout.layoutInput.text.length
    val last = minOf(end, n) - 1
    if (start < 0 || last < start) return emptyList()
    val out = ArrayList<androidx.compose.ui.geometry.Rect>()
    for (line in layout.getLineForOffset(start)..layout.getLineForOffset(last)) {
        val from = maxOf(start, layout.getLineStart(line))
        val to = minOf(last, layout.getLineEnd(line, visibleEnd = true) - 1)
        if (to < from) continue
        val a = layout.getHorizontalPosition(from, usePrimaryDirection = true)
        val b = layout.getBoundingBox(to).right
        val baseline = layout.getLineBaseline(line)
        out += androidx.compose.ui.geometry.Rect(minOf(a, b), baseline - ascent, maxOf(a, b), baseline + descent)
    }
    return out
}

/**
 * #EDE-95: every [HIGHLIGHT_MARK_TAG] piece of [layout]'s text filled in its
 * glyph box ([highlightFillRects]); [metrics] = the (ascent, descent) in px
 * of the piece's font (code = inline code's). Behind the text.
 */
fun androidx.compose.ui.graphics.drawscope.DrawScope.drawHighlightFills(
    layout: androidx.compose.ui.text.TextLayoutResult,
    metrics: (code: Boolean) -> Pair<Float, Float>,
) {
    val text = layout.layoutInput.text
    for (mark in text.getStringAnnotations(HIGHLIGHT_MARK_TAG, 0, text.length)) {
        val (up, down) = metrics(highlightMarkIsCode(mark.item))
        val fill = highlightMarkFill(mark.item)
        for (r in highlightFillRects(layout, mark.start, mark.end, up, down)) drawRect(fill, r.topLeft, r.size)
    }
}
