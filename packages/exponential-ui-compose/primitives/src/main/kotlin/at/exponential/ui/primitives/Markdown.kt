package at.exponential.ui.primitives

import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max

// The read-only GFM subset every Exponential UI renderer paints the same way
// (the gpui painter's `paint::markdown`, the SwiftUI painter's `Markdown` and
// the React renderer's block model): paragraphs, ATX headings, bullet /
// ordered / task lists, quotes, fenced code, rules, pipe tables; inline bold,
// italic, strike, code, links. No dependency: the parser is ours so the
// measurer and the painter share ONE block model, and the layout rule below
// is what both use. A verbatim port of Markdown.swift.

/** One inline span: its text and the marks on it. */
data class MarkdownInline(
    val text: String,
    val bold: Boolean = false,
    val italic: Boolean = false,
    val code: Boolean = false,
    val strike: Boolean = false,
    val link: String? = null,
)

/** What a block is. */
sealed class MarkdownBlockKind {
    /** A paragraph. */
    data object Paragraph : MarkdownBlockKind()

    /** An ATX heading of `level` 1…6. */
    data class Heading(val level: Int) : MarkdownBlockKind()

    /** `marker` = "•" or "3."; `task` = a GFM task box state. */
    data class ListItem(val marker: String, val task: Boolean?) : MarkdownBlockKind()

    /** A block quote (its lines joined). */
    data object Quote : MarkdownBlockKind()

    /** A fenced code block (one code span, lines joined by `\n`). */
    data object CodeBlock : MarkdownBlockKind()

    /** A pipe table: the header cells and the body rows. */
    data class Table(val header: List<List<MarkdownInline>>, val rows: List<List<List<MarkdownInline>>>) : MarkdownBlockKind()

    /** A thematic break. */
    data object Rule : MarkdownBlockKind()
}

/** One block of a document. */
data class MarkdownBlock(
    val kind: MarkdownBlockKind,
    val inlines: List<MarkdownInline>,
    /** Consecutive list items share a group (no paragraph gap inside it). */
    val listGroup: Int? = null,
)

/** Typography of one block kind (dp sizes, CSS weight, family NAME). */
data class MarkdownTextSpec(
    val size: Float,
    val lineHeight: Float,
    val weight: Int,
    val family: String?,
)

private fun roundHalfAway(x: Float): Float = if (x >= 0) floor(x + 0.5f) else -floor(-x + 0.5f)

/**
 * Resolved metrics of a markdown document (the `Markdown` recipe parts in
 * the painter; plain defaults otherwise). The numbers are the gpui
 * painter's, so the native painters agree.
 */
data class MarkdownStyles(
    var body: MarkdownTextSpec,
    var heading: MarkdownTextSpec = MarkdownTextSpec(body.size + 4, body.lineHeight + 8, 600, body.family),
    var code: MarkdownTextSpec = MarkdownTextSpec(max(body.size - 2, 10f), body.lineHeight, 400, null),
    var codePad: Float = 12f,
    var quoteBorder: Float = 2f,
    var quotePad: Float = 12f,
    var blockGap: Float = 8f,
    var headingTop: Float = 12f,
    var headingBottom: Float = 4f,
    var listIndent: Float = roundHalfAway(body.size * 1.4f),
    var cellPadH: Float = 8f,
    var cellPadV: Float = 4f,
) {
    /** The typography of a block kind. */
    fun spec(kind: MarkdownBlockKind): MarkdownTextSpec = when (kind) {
        is MarkdownBlockKind.Heading -> heading
        MarkdownBlockKind.CodeBlock -> code
        else -> body
    }

    internal fun margins(kind: MarkdownBlockKind): Pair<Float, Float> =
        if (kind is MarkdownBlockKind.Heading) headingTop to headingBottom else 0f to blockGap

    /** The gap above block `i` (margins collapse; first/last margins drop). */
    fun gapBefore(blocks: List<MarkdownBlock>, i: Int): Float {
        if (i == 0) return 0f
        val prev = blocks[i - 1]
        val cur = blocks[i]
        if (prev.listGroup != null && prev.listGroup == cur.listGroup) return 0f
        return max(margins(prev.kind).second, margins(cur.kind).first)
    }
}

/**
 * What the layout asks the text system: heights of spans wrapped at a width
 * (null = one line per paragraph), widths, widest words. All in dp.
 */
interface MarkdownTextMeasure {
    /** The height of `inlines` wrapped at `width` (null = no wrap). */
    fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): Float

    /** The one-line width of `inlines` (the widest `\n` line). */
    fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float

    /** The widest whitespace-separated word of `inlines`. */
    fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float
}

/** A placed block: its top, height and (tables) each row's height, header first. */
data class MarkdownBlockBox(val y: Float, val height: Float, val rows: List<Float>)

/** The laid-out document: one box per block and the total height. */
data class MarkdownLayout(val blocks: List<MarkdownBlockBox>, val height: Float)

/** The parser and the layout rule. */
object Markdown {
    private data class Flags(val bold: Boolean = false, val italic: Boolean = false, val strike: Boolean = false)

    private fun push(out: MutableList<MarkdownInline>, text: String, f: Flags, link: String?) {
        if (text.isEmpty()) return
        val last = out.lastOrNull()
        if (last != null && !last.code && last.bold == f.bold && last.italic == f.italic && last.strike == f.strike && last.link == link) {
            out[out.size - 1] = last.copy(text = last.text + text)
            return
        }
        out.add(MarkdownInline(text = text, bold = f.bold, italic = f.italic, strike = f.strike, link = link))
    }

    private data class LinkMatch(val label: String, val href: String, val consumed: Int)

    /** `[label](href)` at the start of `s` → (label, href, consumed). */
    private fun linkAt(s: String): LinkMatch? {
        if (!s.startsWith("[")) return null
        val rest = s.substring(1)
        val close = rest.indexOf("](")
        if (close < 0) return null
        val label = rest.substring(0, close)
        val after = rest.substring(close + 2)
        val end = after.indexOf(')')
        if (end < 0) return null
        val href = after.substring(0, end)
        if (href.any { it.isWhitespace() } || label.isEmpty()) return null
        return LinkMatch(label, href, 1 + label.length + 2 + href.length + 1)
    }

    private fun isAlnum(c: Char): Boolean = c.isLetter() || c.isDigit()

    /** A delimited run `ddTEXTdd` at the start of `s` → (TEXT, consumed). */
    private fun delimited(s: String, delim: String): Pair<String, Int>? {
        if (!s.startsWith(delim)) return null
        val rest = s.substring(delim.length)
        val end = rest.indexOf(delim)
        if (end < 0) return null
        val inner = rest.substring(0, end)
        if (inner.isEmpty() || inner[0].isWhitespace()) return null
        if (delim.startsWith("_")) {
            val nextAt = end + delim.length
            if (nextAt < rest.length && isAlnum(rest[nextAt])) return null
        }
        return inner to (delim.length * 2 + inner.length)
    }

    private fun parseInline(s: String, f: Flags, link: String?, out: MutableList<MarkdownInline>) {
        var plainStart = 0
        var i = 0
        while (i < s.length) {
            val rest = s.substring(i)
            val c = s[i]
            var handled: Int? = null
            if (c == '`') {
                val end = s.indexOf('`', i + 1)
                if (end >= 0) {
                    push(out, s.substring(plainStart, i), f, link)
                    out.add(MarkdownInline(text = s.substring(i + 1, end), code = true, link = link))
                    handled = end - (i + 1) + 2
                }
            } else if (c == '!' && rest.length > 1 && rest[1] == '[') {
                val m = linkAt(rest.substring(1))
                if (m != null) {
                    push(out, s.substring(plainStart, i), f, link)
                    push(out, m.label, f, link)
                    handled = m.consumed + 1
                }
            } else if (c == '[') {
                val m = linkAt(rest)
                if (m != null) {
                    push(out, s.substring(plainStart, i), f, link)
                    parseInline(m.label, f, m.href, out)
                    handled = m.consumed
                }
            } else if (c == '*' || c == '_' || c == '~') {
                val prevAlnum = i > 0 && isAlnum(s[i - 1])
                if (!(c == '_' && prevAlnum)) {
                    val double = if (c == '*') "**" else if (c == '_') "__" else "~~"
                    val d = delimited(rest, double)
                    if (d != null) {
                        push(out, s.substring(plainStart, i), f, link)
                        val nf = if (c == '~') f.copy(strike = true) else f.copy(bold = true)
                        parseInline(d.first, nf, link, out)
                        handled = d.second
                    } else if (c != '~') {
                        val single = delimited(rest, c.toString())
                        if (single != null) {
                            push(out, s.substring(plainStart, i), f, link)
                            parseInline(single.first, f.copy(italic = true), link, out)
                            handled = single.second
                        }
                    }
                }
            }
            if (handled != null) {
                i += handled
                plainStart = i
            } else {
                i += 1
            }
        }
        push(out, s.substring(minOf(plainStart, s.length)), f, link)
    }

    /** The inline spans of one line of markdown. */
    fun parseInline(s: String): List<MarkdownInline> {
        val out = ArrayList<MarkdownInline>()
        parseInline(s, Flags(), null, out)
        return out
    }

    private fun isTableRow(line: String): Boolean {
        val t = line.trim()
        return t.length >= 2 && t.startsWith("|") && t.endsWith("|")
    }

    private fun cells(line: String): List<String> {
        var t = line.trim()
        if (t.startsWith("|")) t = t.substring(1)
        if (t.endsWith("|")) t = t.substring(0, t.length - 1)
        return t.replace("\\|", "\u0001").split("|").map { it.replace("\u0001", "|").trim() }
    }

    private fun isTableRule(line: String): Boolean = isTableRow(line) && cells(line).all { c ->
        val core = c.trim(':')
        c.isNotEmpty() && core.contains("-") && core.all { it == '-' }
    }

    private fun isRule(t: String): Boolean =
        t.length >= 3 && (t.all { it == '-' } || t.all { it == '*' } || t.all { it == '_' })

    /** `- item`, `* item`, `3. item` → (marker, rest). */
    private fun listMarker(line: String): Pair<String, String>? {
        val t = line.trimStart(' ', '\t')
        if (t.startsWith("- ") || t.startsWith("* ") || t.startsWith("+ ")) return "•" to t.substring(2)
        val digits = t.takeWhile { it.isDigit() }
        if (digits.isNotEmpty() && digits.length <= 9) {
            val after = t.substring(digits.length)
            if (after.startsWith(". ") || after.startsWith(") ")) return "$digits." to after.substring(2)
        }
        return null
    }

    /** Parse a document into blocks. */
    fun parse(text: String): List<MarkdownBlock> {
        val normalized = text.replace("\r\n", "\n").replace("\r", "\n")
        val lines = normalized.split("\n")
        val blocks = ArrayList<MarkdownBlock>()
        val para = ArrayList<String>()
        var group = 0
        fun flush() {
            if (para.isNotEmpty()) {
                blocks.add(MarkdownBlock(MarkdownBlockKind.Paragraph, parseInline(para.joinToString(" "))))
                para.clear()
            }
        }
        var i = 0
        while (i < lines.size) {
            val line = lines[i]
            val trimmed = line.trim()
            if (trimmed.startsWith("```")) {
                flush()
                val body = ArrayList<String>()
                i += 1
                while (i < lines.size && !lines[i].trim().startsWith("```")) {
                    body.add(lines[i])
                    i += 1
                }
                i += 1
                blocks.add(MarkdownBlock(MarkdownBlockKind.CodeBlock, listOf(MarkdownInline(text = body.joinToString("\n"), code = true))))
                continue
            }
            val hashes = trimmed.takeWhile { it == '#' }.length
            if (hashes in 1..6 && trimmed.substring(hashes).startsWith(" ")) {
                flush()
                blocks.add(MarkdownBlock(MarkdownBlockKind.Heading(hashes), parseInline(trimmed.substring(hashes).trim())))
                i += 1
                continue
            }
            if (trimmed.startsWith(">")) {
                flush()
                val body = ArrayList<String>()
                while (i < lines.size && lines[i].trimStart(' ').startsWith(">")) {
                    var l = lines[i].trimStart(' ')
                    while (l.startsWith(">")) l = l.substring(1)
                    if (l.startsWith(" ")) l = l.substring(1)
                    body.add(l.trim())
                    i += 1
                }
                blocks.add(MarkdownBlock(MarkdownBlockKind.Quote, parseInline(body.filter { it.isNotEmpty() }.joinToString(" "))))
                continue
            }
            if (isRule(trimmed) && para.isEmpty()) {
                blocks.add(MarkdownBlock(MarkdownBlockKind.Rule, emptyList()))
                i += 1
                continue
            }
            if (listMarker(line) != null) {
                flush()
                group += 1
                while (i < lines.size) {
                    val (marker, rest) = listMarker(lines[i]) ?: break
                    var task: Boolean? = null
                    var body = rest
                    if (rest.startsWith("[ ] ")) {
                        task = false
                        body = rest.substring(4)
                    } else if (rest.startsWith("[x] ") || rest.startsWith("[X] ")) {
                        task = true
                        body = rest.substring(4)
                    }
                    blocks.add(MarkdownBlock(MarkdownBlockKind.ListItem(marker, task), parseInline(body.trim()), group))
                    i += 1
                }
                continue
            }
            if (isTableRow(line) && i + 1 < lines.size && isTableRule(lines[i + 1])) {
                flush()
                val header = cells(line).map { parseInline(it) }
                i += 2
                val rows = ArrayList<List<List<MarkdownInline>>>()
                while (i < lines.size && isTableRow(lines[i])) {
                    rows.add(cells(lines[i]).map { parseInline(it) })
                    i += 1
                }
                blocks.add(MarkdownBlock(MarkdownBlockKind.Table(header, rows), emptyList()))
                continue
            }
            if (trimmed.isEmpty()) flush() else para.add(trimmed)
            i += 1
        }
        flush()
        return blocks
    }

    /** The plain text of some spans. */
    fun plain(inlines: List<MarkdownInline>): String = inlines.joinToString("") { it.text }

    /** The plain text of a whole document (accessibility). */
    fun plainText(text: String): String = parse(text).joinToString("\n") { block ->
        val kind = block.kind
        if (kind is MarkdownBlockKind.Table) {
            (listOf(kind.header) + kind.rows).joinToString("\n") { row -> row.joinToString(" ") { plain(it) } }
        } else {
            plain(block.inlines)
        }
    }

    /**
     * Lay the blocks out `width` wide (the painter places blocks at these
     * boxes; the measurer reports `height`).
     */
    fun layout(blocks: List<MarkdownBlock>, s: MarkdownStyles, width: Float, text: MarkdownTextMeasure): MarkdownLayout {
        var y = 0f
        val out = ArrayList<MarkdownBlockBox>(blocks.size)
        for ((i, b) in blocks.withIndex()) {
            y += s.gapBefore(blocks, i)
            val spec = s.spec(b.kind)
            val rows = ArrayList<Float>()
            val height: Float = when (val k = b.kind) {
                MarkdownBlockKind.Paragraph, is MarkdownBlockKind.Heading -> text.height(b.inlines, spec, max(width, 1f))
                is MarkdownBlockKind.ListItem -> text.height(b.inlines, spec, max(width - s.listIndent, 1f))
                MarkdownBlockKind.Quote -> text.height(b.inlines, spec, max(width - s.quoteBorder - s.quotePad, 1f))
                MarkdownBlockKind.CodeBlock -> text.height(b.inlines, spec, null) + 2 * s.codePad
                MarkdownBlockKind.Rule -> 1f
                is MarkdownBlockKind.Table -> {
                    val cols = maxOf(k.header.size, k.rows.maxOfOrNull { it.size } ?: 0, 1)
                    val cellW = max(width / cols - 2 * s.cellPadH, 1f)
                    for (row in listOf(k.header) + k.rows) {
                        val h = row.map { text.height(it, s.body, cellW) }.fold(s.body.lineHeight) { a, x -> max(a, x) }
                        rows.add(h + 2 * s.cellPadV + 1)
                    }
                    rows.sum() + 1
                }
            }
            out.add(MarkdownBlockBox(y, height, rows))
            y += height
        }
        return MarkdownLayout(out, y)
    }

    /** Max-content width: the widest block on one line. */
    fun maxContentWidth(blocks: List<MarkdownBlock>, s: MarkdownStyles, text: MarkdownTextMeasure): Float {
        val widths = blocks.map { b ->
            val spec = s.spec(b.kind)
            when (val k = b.kind) {
                MarkdownBlockKind.Paragraph, is MarkdownBlockKind.Heading -> text.width(b.inlines, spec)
                is MarkdownBlockKind.ListItem -> text.width(b.inlines, spec) + s.listIndent
                MarkdownBlockKind.Quote -> text.width(b.inlines, spec) + s.quoteBorder + s.quotePad
                MarkdownBlockKind.CodeBlock -> {
                    val first = b.inlines.firstOrNull() ?: MarkdownInline(text = "", code = true)
                    (first.text.split("\n").maxOfOrNull { text.width(listOf(MarkdownInline(text = it, code = true)), spec) } ?: 0f) + 2 * s.codePad
                }
                MarkdownBlockKind.Rule -> 0f
                is MarkdownBlockKind.Table -> {
                    val cols = max(k.header.size, 1).toFloat()
                    val widest = (listOf(k.header) + k.rows).flatten().maxOfOrNull { text.width(it, s.body) } ?: 0f
                    cols * (widest + 2 * s.cellPadH)
                }
            }
        }
        return widths.maxOrNull()?.let { ceil(it) } ?: 0f
    }

    /** Min-content width: the widest word (code blocks never wrap). */
    fun minContentWidth(blocks: List<MarkdownBlock>, s: MarkdownStyles, text: MarkdownTextMeasure): Float {
        val widths = blocks.map { b ->
            val spec = s.spec(b.kind)
            when (val k = b.kind) {
                MarkdownBlockKind.CodeBlock -> maxContentWidth(listOf(b), s, text)
                is MarkdownBlockKind.Table -> max(k.header.size, 1) * (2 * s.cellPadH + 16)
                is MarkdownBlockKind.ListItem -> text.widestWord(b.inlines, spec) + s.listIndent
                MarkdownBlockKind.Quote -> text.widestWord(b.inlines, spec) + s.quoteBorder + s.quotePad
                else -> text.widestWord(b.inlines, spec)
            }
        }
        return widths.maxOrNull()?.let { ceil(it) } ?: 0f
    }
}
