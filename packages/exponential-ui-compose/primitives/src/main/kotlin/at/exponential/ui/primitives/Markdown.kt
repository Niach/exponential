package at.exponential.ui.primitives

import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max

// The read-only GFM subset every Exponential UI renderer paints the same way
// (the gpui painter's `paint::markdown`, the SwiftUI painter's `Markdown` and
// the React renderer's block model): paragraphs, ATX headings, bullet /
// ordered / task lists (nested by marker indent), quotes, fenced code,
// rules, pipe tables, block images (a paragraph that is one `![alt](src)`,
// and #FEED-208 an image directly in a top-level paragraph's text, which
// splits the paragraph around it);
// inline bold, italic, strike, code, links (CommonMark destinations:
// balanced parentheses, no whitespace). No dependency: the parser is ours so the
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
    /** #VAPP-258: a host chip (a text link painted as the Chip look, inline); never merged with a neighbour. */
    val chip: MarkdownChip? = null,
)

/** #VAPP-258 (catalog/host.json `markdown`): a text link's chip: [kind] in [KINDS], a non-empty [label], an optional icon registry name. */
data class MarkdownChip(val kind: String, val label: String, val icon: String? = null, val ident: String? = null) {
    /** #VAPP-258: the identifier painted mono before the label (an empty one = none). */
    val shownIdent: String? get() = ident?.takeIf { it.isNotEmpty() }

    /** A known kind and a non-empty label (else the chip is dropped, the link stays). */
    val valid: Boolean get() = kind in KINDS && label.isNotEmpty()

    /** The kinds. */
    companion object {
        /** `chipKinds`. */
        val KINDS: Set<String> = setOf("issue", "mention", "entity")
    }
}

/** #VAPP-258: one piece of a host's text links: [text], its POLICED [href] (null = text) and an optional [chip]. */
data class MarkdownTextLink(val text: String, val href: String? = null, val chip: MarkdownChip? = null)

/** #VAPP-258: a host's `attachment(href)` answer: [type] in [KINDS] and a non-empty [name]. */
data class MarkdownAttachment(val type: String, val name: String) {
    /** The kinds. */
    companion object {
        /** `attachmentKinds`. */
        val KINDS: Set<String> = setOf("video", "audio", "file")
    }
}

/** #VAPP-258: a host's `imageSize(src)` answer (natural size; non-positive or non-finite = unknown). */
data class MarkdownSize(val width: Float, val height: Float) {
    /** Positive and finite. */
    val valid: Boolean get() = width.isFinite() && height.isFinite() && width > 0f && height > 0f
}

/**
 * #VAPP-258: the host hooks a document is resolved through (all optional):
 * the URL policy ([linkPolicy], null result = text), the media policy
 * ([imagePolicy]), [attachment] (asked with the RAW destination), the
 * natural [imageSize] of a block image (the RAW src) and the [textLinks]
 * (policed segments, chips on them). The measurer and the painter resolve
 * through the same hooks, so the measured blocks are the painted ones.
 */
class MarkdownHooks(
    val linkPolicy: ((String) -> String?)? = null,
    val imagePolicy: ((String) -> Boolean)? = null,
    val attachment: ((String) -> MarkdownAttachment?)? = null,
    val imageSize: ((String) -> MarkdownSize?)? = null,
    val textLinks: ((String) -> List<MarkdownTextLink>?)? = null,
    /** #VAPP-258 step 5: the host's render options (hard breaks, a streamed prefix). */
    val options: MarkdownOptions = MarkdownOptions(),
    /** #VAPP-258 (markdown-host.json `loaded`): the size a block image DECODED at (the media cache, [MarkdownImageSizes]): an image [imageSize] cannot size takes it. */
    val loadedSize: ((String) -> MarkdownSize?)? = null,
)

/**
 * #VAPP-258 step 5 (`catalog/host.json` markdown.options): how a host's
 * Markdowns render; the default = what a Markdown painted before them.
 * [hardBreaks] = `breaks: hard` (every newline inside a paragraph, list item
 * or quote paragraph breaks the line); [chat] = `density: chat` (the
 * `Markdown` recipes resolved with `density: chat`, the theme's chat
 * rhythm); [selectable] = whether the text selects and copies (null = no,
 * as before); [streaming] = the text is a growing prefix (its stable blocks
 * memoized, its unfinished tail held back). The measurer reads the same.
 */
data class MarkdownOptions(
    val hardBreaks: Boolean = false,
    val chat: Boolean = false,
    val selectable: Boolean? = null,
    val streaming: Boolean = false,
    /** #VAPP-258: a block image's height cap (dp); null = none (the width at the image's aspect, at most the viewport's height). */
    val imageMaxHeight: Float? = IMAGE_MAX_HEIGHT,
) {
    /** [imageMaxHeight] as applied: null = none, an invalid number = [IMAGE_MAX_HEIGHT]. */
    val imageCap: Float? get() = imageMaxHeight?.let { if (it.isFinite() && it > 0f) it else IMAGE_MAX_HEIGHT }

    companion object {
        /** catalog/host.json `markdown.imageMaxHeight`: the default cap. */
        const val IMAGE_MAX_HEIGHT = 160f

        /** An unsized image's aspect (height / width) with no cap: 4:3. */
        const val UNSIZED_ASPECT = 0.75f

        /** From this many blocks on a painter lays out only the visible ones (`markdown.lazyBlocks`). */
        const val LAZY_BLOCKS = 24
    }
}

/** What a block is. */
sealed class MarkdownBlockKind {
    /** A paragraph. */
    data object Paragraph : MarkdownBlockKind()

    /** An ATX heading of `level` 1…6. */
    data class Heading(val level: Int) : MarkdownBlockKind()

    /**
     * `marker` = "•" or "3."; `task` = a GFM task box state; `depth` = the
     * nesting level (0 = top; a child list indents one `listIndent` more).
     */
    data class ListItem(val marker: String, val task: Boolean?, val depth: Int = 0) : MarkdownBlockKind()

    /**
     * A paragraph that is one image `![alt](src)`: painted through the
     * host's media policy at `imageHeight`, the alt text in the box while it
     * loads or when it fails; a DENIED src is a paragraph of its alt text
     * ([Markdown.resolveImages]). `inlines` = the alt text.
     */
    data class Image(val src: String, val alt: String, val width: Float? = null, val height: Float? = null) : MarkdownBlockKind()

    /**
     * #VAPP-258: a top-level paragraph that is one markdown link the host
     * classified ([Markdown.resolveAttachments]): `video` / `audio` / `file`,
     * its [name] and the POLICED [href]. `inlines` = the link's label.
     */
    data class Attachment(val type: String, val name: String, val href: String) : MarkdownBlockKind()

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
    /** #VAPP-258: a code block's fence info string (` ```ts ` = `ts`). */
    val lang: String? = null,
)

/** Round 9 (`fixtures/markdown-inline.json` `documentPaint`): a bullet's width and its centre in the indent, in em. */
const val MARKDOWN_BULLET_EM = 0.36f
const val MARKDOWN_BULLET_CENTRE_EM = 0.46f

/**
 * Round 9 (`fixtures/markdown-inline.json` `documentPaint`): inline code, the
 * `Markdown/code` recipe: its type ([size]; null = 0.925 em of the run), a
 * [radius]-cornered fill, [padH] (null = 0.3 em) reserved in the line on each
 * side and [padV] over the font's own ascent and descent, as the web's inline
 * box. On [MarkdownChipStyle] because every measurer already gets that one.
 */
data class MarkdownCodeStyle(
    val size: Float? = null,
    val padH: Float? = null,
    val padV: Float = 0f,
    val radius: Float = 4f,
) {
    fun size(base: MarkdownTextSpec): Float = size ?: (base.size * 0.925f)
    fun padH(base: MarkdownTextSpec): Float = padH ?: (base.size * 0.3f)
}

/**
 * #VAPP-258: the inline chip's geometry: [label] = the Text `caption`
 * style, [padH] = `$spacing.sm` each side, [gap] = `$spacing.xs` after the
 * [iconSize] (`$control.iconSm`) icon, [height] = `$control.badge` (capped
 * at the line's height).
 */
data class MarkdownChipStyle(
    val label: MarkdownTextSpec = MarkdownTextSpec(12f, 16f, 400, null),
    val padH: Float = 8f,
    val gap: Float = 4f,
    val iconSize: Float = 16f,
    val height: Float = 20f,
    /** #VAPP-258: the ident's type (`Markdown/chipIdent`; null = the mono family at the label's size). */
    val ident: MarkdownTextSpec? = null,
    /** #VAPP-258: the box's corner radius (null = a capsule). */
    val radius: Float? = null,
    /** #VAPP-258: a theme's `Markdown/chip*` metrics per chip kind. */
    val kinds: Map<String, MarkdownChipKindStyle> = emptyMap(),
    /** Round 9: inline code. */
    val code: MarkdownCodeStyle = MarkdownCodeStyle(),
) {
    /** The chip box's height in a line [lineHeight] tall. */
    fun height(lineHeight: Float): Float = minOf(height, lineHeight)

    /** The style a chip of [kind] lays out with (its kind's overrides on top). */
    fun resolved(kind: String): MarkdownChipStyle {
        val k = kinds[kind] ?: return this
        return copy(label = k.label ?: label, ident = k.ident ?: ident, padH = k.padH ?: padH, radius = k.radius ?: radius, kinds = emptyMap())
    }

    /** The ident's type: its own, else the mono family at the label's size. */
    val identSpec: MarkdownTextSpec get() = ident ?: MarkdownTextSpec(label.size, label.lineHeight, label.weight, "ui-monospace")

    /** The chip's width around a label [labelWidth] wide and an ident [identWidth] wide (uncapped). */
    fun width(chip: MarkdownChip, labelWidth: Float, identWidth: Float = 0f): Float =
        2 * padH + labelWidth + (if (chip.icon.isNullOrEmpty()) 0f else iconSize + gap) + (if (chip.shownIdent == null) 0f else identWidth + gap)

    /** The label's width for a shaped [px] extent at [density] (whole px + 1 slack: what measure and paint both use). */
    fun labelWidth(px: Float, density: Float): Float = (ceil(px - 0.01f) + 1f) / density
}

/** #VAPP-258: one chip kind's `Markdown/chip*` metrics (null = the base). */
data class MarkdownChipKindStyle(
    val label: MarkdownTextSpec? = null,
    val ident: MarkdownTextSpec? = null,
    val padH: Float? = null,
    val radius: Float? = null,
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
    /** The height cap of a block image (loaded or not, so measure = paint); null = none (`imageMaxHeight: null`). */
    var imageHeight: Float? = 160f,
    /** #VAPP-258: the viewport height an uncapped image stops at (null = unknown, no limit). */
    var viewportHeight: Float? = null,
    /** #VAPP-258: a block image's corner radius (the `Markdown/image` recipe's `borderRadius`; 0 without a theme). */
    var imageRadius: Float = 0f,
    /** #VAPP-258: the inline chip. */
    var chip: MarkdownChipStyle = MarkdownChipStyle(),
    /** #VAPP-258: a heading's type and margins per `level` (1..6; absent = [heading], [headingTop], [headingBottom]). */
    var headingLevels: Map<Int, MarkdownTextSpec> = emptyMap(),
    var headingTops: Map<Int, Float> = emptyMap(),
    var headingBottoms: Map<Int, Float> = emptyMap(),
    /** #VAPP-258: a list item's `Markdown/listItem` margins (collapsing with each other and, at a list's end, the block gap). */
    var itemTop: Float = 0f,
    var itemBottom: Float = 0f,
    /** #VAPP-258: a fenced block's border (`Markdown/codeBlock` borderWidth), inside its box: the box grows by it on each side. */
    var codeBorder: Float = 0f,
    /** #VAPP-258: a video attachment's max width (`mediaIntrinsicWidth`) and aspect (`mediaAspectRatio`). */
    var mediaMaxWidth: Float = 320f,
    var mediaAspect: Float = 16f / 9f,
    /** #VAPP-258: an audio attachment = its title line + [audioGap] + the controls row. */
    var audioTitle: Float = body.lineHeight,
    var audioGap: Float = 4f,
    var audioRow: Float = 32f,
    /** #VAPP-258: a file attachment's row height (the FileUpload `file` row). */
    var fileHeight: Float = 40f,
    /** Round 9 §9.1: an inline-profile paragraph's type (Text/root of its `variant`); null = [body]. */
    var paragraph: MarkdownTextSpec? = null,
) {
    /** #VAPP-258: an attachment block's height at [width]. */
    fun attachmentHeight(kind: MarkdownBlockKind.Attachment, width: Float): Float = when (kind.type) {
        "video" -> attachmentWidth(kind, width) / mediaAspect
        "audio" -> audioTitle + audioGap + audioRow
        else -> fileHeight
    }

    /** #VAPP-258: an attachment block's width at [width] (a video caps at [mediaMaxWidth]). */
    fun attachmentWidth(kind: MarkdownBlockKind.Attachment, width: Float): Float =
        if (kind.type == "video") max(0f, minOf(width, mediaMaxWidth)) else max(0f, width)

    /** The leading indent of a list item at [depth] (its marker column + the body inset). */
    fun listInset(depth: Int): Float = listIndent * (depth + 1)

    /** The typography of a block kind. */
    fun spec(kind: MarkdownBlockKind): MarkdownTextSpec = when (kind) {
        is MarkdownBlockKind.Heading -> headingLevels[levelOf(kind)] ?: heading
        MarkdownBlockKind.CodeBlock -> code
        MarkdownBlockKind.Paragraph -> paragraph ?: body
        else -> body
    }

    private fun levelOf(kind: MarkdownBlockKind.Heading): Int = kind.level.coerceIn(1, 6)

    internal fun margins(kind: MarkdownBlockKind): Pair<Float, Float> = when (kind) {
        is MarkdownBlockKind.Heading -> (headingTops[levelOf(kind)] ?: headingTop) to (headingBottoms[levelOf(kind)] ?: headingBottom)
        is MarkdownBlockKind.ListItem -> itemTop to max(itemBottom, blockGap)
        else -> 0f to blockGap
    }

    /** The gap above block `i` (margins collapse; first/last margins drop; inside a list, the items' own margins). */
    fun gapBefore(blocks: List<MarkdownBlock>, i: Int): Float {
        if (i == 0) return 0f
        val prev = blocks[i - 1]
        val cur = blocks[i]
        if (prev.listGroup != null && prev.listGroup == cur.listGroup) return max(itemBottom, itemTop)
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

    /** #VAPP-258: [height] with chip runs as [chips] boxes (each capped at the wrap width). Default: chips ignored. */
    fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?, chips: MarkdownChipStyle): Float = height(inlines, spec, width)

    /** #VAPP-258: [width] with chip runs as [chips] boxes. */
    fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, chips: MarkdownChipStyle): Float = width(inlines, spec)

    /** #VAPP-258: [widestWord] where a chip is one unbreakable word. */
    fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, chips: MarkdownChipStyle): Float = widestWord(inlines, spec)
}

/** A placed block: its top, height and (tables) each row's height, header first. */
data class MarkdownBlockBox(val y: Float, val height: Float, val rows: List<Float>)

/** The laid-out document: one box per block and the total height. */
data class MarkdownLayout(val blocks: List<MarkdownBlockBox>, val height: Float)

/** The parser and the layout rule. */
object Markdown {
    private data class Flags(val bold: Boolean = false, val italic: Boolean = false, val strike: Boolean = false)

    private val ENTITIES = mapOf("amp" to "&", "lt" to "<", "gt" to ">", "quot" to "\"", "apos" to "'", "nbsp" to "\u00A0")
    private val ENTITY = Regex("&(?:#([0-9]{1,7})|#[xX]([0-9a-fA-F]{1,6})|([A-Za-z]+));")

    /** #VAPP-258 (markdown-render.json): the common named entities and numeric references decoded (text, never code). */
    fun decodeEntities(s: String): String = if ('&' !in s) s else ENTITY.replace(s) { m ->
        val (dec, hex, name) = m.destructured
        when {
            name.isNotEmpty() -> ENTITIES[name] ?: m.value
            else -> {
                val code = if (dec.isNotEmpty()) dec.toIntOrNull() else hex.toIntOrNull(16)
                if (code != null && code in 1..0x10FFFF) String(Character.toChars(code)) else m.value
            }
        }
    }

    private fun push(out: MutableList<MarkdownInline>, raw: String, f: Flags, link: String?) {
        if (raw.isEmpty()) return
        val text = decodeEntities(raw)
        val last = out.lastOrNull()
        if (last != null && !last.code && last.chip == null && last.bold == f.bold && last.italic == f.italic && last.strike == f.strike && last.link == link) {
            out[out.size - 1] = last.copy(text = last.text + text)
            return
        }
        out.add(MarkdownInline(text = text, bold = f.bold, italic = f.italic, strike = f.strike, link = link))
    }

    private data class LinkMatch(val label: String, val href: String, val consumed: Int)

    /** Link labels nest at most this deep; a deeper `[` is literal text (Markdown.swift's cap). */
    const val MAX_LINK_DEPTH = 32

    /**
     * The searches of one inline string, memoised so a hostile line stays
     * linear: `indexOf` per needle (a miss from `p` = a miss from every
     * later `p`, a hit at `c` = the hit for every start up to `c`) and the
     * link destination scan (a scan that ran into whitespace or the end
     * answers every later start inside the run it covered).
     */
    private class Scan(val s: String) {
        private val hits = HashMap<String, IntArray>()

        /**
         * #FEED-214: each `[`'s balancing `]` (-1 = none), found in ONE pass
         * with a stack (`\` + any character is skipped), like web's
         * LinkScanner: a label holds nested brackets (`[![x](1.png)](u)`).
         */
        private val bracketClose: IntArray by lazy {
            val out = IntArray(s.length) { -1 }
            val open = ArrayList<Int>()
            var i = 0
            while (i < s.length) {
                when (s[i]) {
                    '\\' -> i += 1
                    '[' -> open.add(i)
                    ']' -> if (open.isNotEmpty()) out[open.removeAt(open.size - 1)] = i
                }
                i += 1
            }
            out
        }

        /** The `]` balancing the `[` at [at]; -1 = none. */
        fun closeBracket(at: Int): Int = bracketClose[at]

        fun indexOf(needle: String, from: Int): Int {
            val memo = hits.getOrPut(needle) { intArrayOf(-2, -2) } // [from, result]
            if (memo[0] >= 0 && from >= memo[0]) {
                if (memo[1] < 0) return -1
                if (from <= memo[1]) return memo[1]
            }
            val r = s.indexOf(needle, from)
            memo[0] = from
            memo[1] = r
            return r
        }

        // A failed destination scan from `runStart` to `runEnd` (whitespace or the end):
        // closes[p - runStart] = the `)` a scan from p stops at, or -1.
        private var runStart = -1
        private var runEnd = -1
        private var closes = IntArray(0)

        /** The `)` closing a destination that starts at [start] (balanced, `\` escapes, no whitespace); -1 = none. */
        fun destinationEnd(start: Int): Int {
            if (start > runStart && start < runEnd) return closes[start - runStart]
            var i = start
            var depth = 0
            while (i < s.length) {
                val c = s[i]
                when {
                    c == '\\' && i + 1 < s.length -> i += 1
                    c.isWhitespace() || c.isISOControl() -> break
                    c == '(' -> depth += 1
                    c == ')' -> {
                        if (depth == 0) return i
                        depth -= 1
                    }
                }
                i += 1
            }
            remember(start, i)
            return -1
        }

        /**
         * The answers of every start inside a failed run: a scan from p stops
         * at the first unescaped `)` at or after p whose depth (counted from
         * the run's start) equals p's own.
         */
        private fun remember(from: Int, to: Int) {
            val n = to - from
            val depthAt = IntArray(n)
            val live = BooleanArray(n)
            var depth = 0
            var i = from
            while (i < to) {
                depthAt[i - from] = depth
                live[i - from] = true
                val c = s[i]
                if (c == '\\' && i + 1 < s.length) {
                    i += 1
                    if (i < to) depthAt[i - from] = depth
                } else if (c == '(') {
                    depth += 1
                } else if (c == ')') {
                    depth -= 1
                }
                i += 1
            }
            val nearest = IntArray(n + 1) { -1 }
            val out = IntArray(n)
            for (k in n - 1 downTo 0) {
                if (live[k] && s[from + k] == ')') nearest[depthAt[k]] = from + k
                out[k] = if (live[k]) nearest[depthAt[k]] else -1
            }
            runStart = from
            runEnd = to
            closes = out
        }
    }

    /**
     * `[label](href)` at `s[at]` → (label, href, consumed). The label
     * balances brackets (#FEED-214, as web, Swift and gpui). The destination
     * follows CommonMark: balanced parentheses (`A_(b)`), `\` escapes, no
     * whitespace, never empty.
     */
    private fun linkAt(scan: Scan, at: Int, image: Boolean = false): LinkMatch? {
        val s = scan.s
        if (at >= s.length || s[at] != '[') return null
        val close = scan.closeBracket(at)
        if (close < 0 || close + 1 >= s.length || s[close + 1] != '(') return null
        val start = close + 2
        val end = scan.destinationEnd(start)
        if (end < 0) return null
        val label = s.substring(at + 1, close)
        val href = s.substring(start, end).replace(Regex("""\\([!-/:-@\[-`{-~])"""), "$1")
        if (href.isEmpty() || (label.isEmpty() && !image)) return null
        return LinkMatch(label, href, end + 1 - at)
    }

    /** `![alt](src)` = the WHOLE of `s` → (alt, src). */
    private fun imageOnly(s: String): Pair<String, String>? {
        if (!s.startsWith("![")) return null
        val m = linkAt(Scan(s), 1, image = true) ?: return null
        return if (m.consumed + 1 == s.length) m.label to m.href else null
    }

    private fun isAlnum(c: Char): Boolean = c.isLetter() || c.isDigit()

    /** A delimited run `ddTEXTdd` at `s[at]` → (TEXT, consumed). */
    private fun delimited(scan: Scan, at: Int, delim: String): Pair<String, Int>? {
        val s = scan.s
        if (!s.startsWith(delim, at)) return null
        val from = at + delim.length
        val end = scan.indexOf(delim, from)
        if (end < 0) return null
        if (end == from || s[from].isWhitespace()) return null
        if (delim.startsWith("_")) {
            val nextAt = end + delim.length
            if (nextAt < s.length && isAlnum(s[nextAt])) return null
        }
        return s.substring(from, end) to (end + delim.length - at)
    }

    /**
     * [onImage] (#FEED-208): set only on a top-level paragraph's own run (never
     * passed into emphasis or a link label), it takes each image directly in
     * the text as (alt, src) instead of its alt text, so the paragraph splits
     * around it ([splitImages]).
     */
    private fun parseInline(s: String, f: Flags, link: String?, out: MutableList<MarkdownInline>, linkDepth: Int, onImage: ((String, String) -> Unit)? = null) {
        val scan = Scan(s)
        var plainStart = 0
        var i = 0
        while (i < s.length) {
            val c = s[i]
            var handled: Int? = null
            if (c == '\\' && i + 1 < s.length && s[i + 1].code < 128 && isAsciiPunct(s[i + 1])) {
                // #VAPP-258: a backslash-escaped punctuation character is literal.
                push(out, s.substring(plainStart, i), f, link)
                push(out, s[i + 1].toString(), f, link)
                handled = 2
            } else if (c == '`') {
                val end = scan.indexOf("`", i + 1)
                if (end >= 0) {
                    push(out, s.substring(plainStart, i), f, link)
                    out.add(MarkdownInline(text = s.substring(i + 1, end), code = true, link = link))
                    handled = end - (i + 1) + 2
                }
            } else if (c == '!' && i + 1 < s.length && s[i + 1] == '[') {
                val m = linkAt(scan, i + 1, image = true)
                if (m != null) {
                    push(out, s.substring(plainStart, i), f, link)
                    // #FEED-208: directly in a top-level paragraph's text it splits the paragraph; anywhere else it paints its alt text.
                    if (onImage != null) onImage(decodeEntities(m.label), m.href) else push(out, m.label, f, link)
                    handled = m.consumed + 1
                }
            } else if (c == '[' && linkDepth < MAX_LINK_DEPTH) {
                val m = linkAt(scan, i)
                if (m != null) {
                    push(out, s.substring(plainStart, i), f, link)
                    parseInline(m.label, f, m.href, out, linkDepth + 1)
                    handled = m.consumed
                }
            } else if (c == '*' || c == '_' || c == '~') {
                val prevAlnum = i > 0 && isAlnum(s[i - 1])
                if (!(c == '_' && prevAlnum)) {
                    val double = if (c == '*') "**" else if (c == '_') "__" else "~~"
                    val triple = if (c != '~') delimited(scan, i, c.toString().repeat(3)) else null
                    val d = if (triple == null) delimited(scan, i, double) else null
                    if (triple != null) {
                        // `***x***` is both.
                        push(out, s.substring(plainStart, i), f, link)
                        parseInline(triple.first, f.copy(bold = true, italic = true), link, out, linkDepth)
                        handled = triple.second
                    } else if (d != null) {
                        push(out, s.substring(plainStart, i), f, link)
                        val nf = if (c == '~') f.copy(strike = true) else f.copy(bold = true)
                        parseInline(d.first, nf, link, out, linkDepth)
                        handled = d.second
                    } else if (c != '~') {
                        val single = delimited(scan, i, c.toString())
                        if (single != null) {
                            push(out, s.substring(plainStart, i), f, link)
                            parseInline(single.first, f.copy(italic = true), link, out, linkDepth)
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
        parseInline(s, Flags(), null, out, 0)
        return out
    }

    /**
     * #FEED-208 (`fixtures/markdown-render.json` `image_split_*`): a top-level
     * paragraph with images directly in its text = the text before each image
     * (a paragraph), the image (a block image), the text after it. ASCII
     * spaces, tabs and line breaks at a split edge trim; an empty side drops.
     */
    private fun splitImages(s: String): List<MarkdownBlock> {
        val blocks = ArrayList<MarkdownBlock>()
        val run = ArrayList<MarkdownInline>()
        fun plain(i: MarkdownInline) = !i.bold && !i.italic && !i.code && !i.strike && i.link == null && i.chip == null
        val edge = charArrayOf(' ', '\t', '\n')
        fun paragraph() {
            while (run.isNotEmpty() && plain(run.first())) {
                val t = run.first().text.trimStart(*edge)
                if (t.isNotEmpty()) { run[0] = run.first().copy(text = t); break }
                run.removeAt(0)
            }
            while (run.isNotEmpty() && plain(run.last())) {
                val t = run.last().text.trimEnd(*edge)
                if (t.isNotEmpty()) { run[run.size - 1] = run.last().copy(text = t); break }
                run.removeAt(run.size - 1)
            }
            if (run.isNotEmpty()) blocks.add(MarkdownBlock(MarkdownBlockKind.Paragraph, run.toList()))
            run.clear()
        }
        parseInline(s, Flags(), null, run, 0, onImage = { alt, src ->
            paragraph()
            blocks.add(MarkdownBlock(MarkdownBlockKind.Image(src, alt), listOf(MarkdownInline(text = alt))))
        })
        paragraph()
        return blocks
    }

    private fun isAsciiPunct(c: Char): Boolean = c in '!'..'/' || c in ':'..'@' || c in '['..'`' || c in '{'..'~'

    /** A line that opens a block of its own (never a continuation). */
    private fun startsBlock(line: String): Boolean {
        val t = line.trim()
        val hashes = t.takeWhile { it == '#' }.length
        return t.startsWith("```") || t.startsWith("~~~") || t.startsWith(">") || isTableRow(line) || isRule(t) ||
            (hashes in 1..6 && t.substring(hashes).startsWith(" "))
    }

    /** Paragraph lines joined: a line ending in a backslash or two spaces breaks hard (a newline), any other is a space. */
    private fun joinParagraph(para: List<String>, hard: Boolean = false): String {
        val sb = StringBuilder()
        para.forEachIndexed { k, line ->
            if (k > 0) {
                when {
                    sb.endsWith("\\") -> { sb.setLength(sb.length - 1); sb.append('\n') }
                    sb.endsWith("  ") -> { while (sb.endsWith(" ")) sb.setLength(sb.length - 1); sb.append('\n') }
                    else -> sb.append(if (hard) '\n' else ' ')
                }
            }
            sb.append(line)
        }
        return sb.toString().trimEnd(' ')
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

    private data class ListLine(val marker: String, val rest: String, val column: Int)

    /** `- item`, `* item`, `3. item` → (marker, rest, the marker's column; a tab = 4). */
    private fun listMarker(line: String): ListLine? {
        val lead = line.takeWhile { it == ' ' || it == '\t' }
        val column = lead.sumOf { if (it == '\t') 4 else 1 }
        val t = line.substring(lead.length)
        if (t.startsWith("- ") || t.startsWith("* ") || t.startsWith("+ ")) return ListLine("•", t.substring(2), column)
        val digits = t.takeWhile { it.isDigit() }
        if (digits.isNotEmpty() && digits.length <= 9) {
            val after = t.substring(digits.length)
            if (after.startsWith(". ") || after.startsWith(") ")) return ListLine("$digits.", after.substring(2), column)
        }
        return null
    }

    /** Parse a document into blocks. */
    fun parse(text: String): List<MarkdownBlock> = parseBlocks(text, false)

    /**
     * #VAPP-258 step 5 (`catalog/host.json` markdown.options): [hardBreaks]
     * = every newline inside a paragraph, list item or quote paragraph is a
     * line break; [streaming] = the text is a growing prefix ([parseStreaming]).
     */
    fun parse(text: String, hardBreaks: Boolean, streaming: Boolean = false): List<MarkdownBlock> =
        if (streaming) parseStreaming(text, hardBreaks) else parseBlocks(text, hardBreaks)

    private fun fenceOpen(line: String): String? {
        val t = line.trim()
        return when {
            t.startsWith("```") -> "```"
            t.startsWith("~~~") -> "~~~"
            else -> null
        }
    }

    private data class Fences(val boundary: Int, val open: Boolean, val openerLast: Boolean)

    /**
     * [lines] from [from] (a line outside a fence) on: the line index after
     * the last COMPLETE blank line outside a fence (else [from]; the last
     * line is never complete), whether a fence is open at the end, and
     * whether the last line is that fence's opener.
     */
    private fun scanFences(lines: List<String>, from: Int): Fences {
        var fence: String? = null
        var boundary = from
        var openerLast = false
        for (i in from until lines.size) {
            val line = lines[i]
            openerLast = false
            val f = fence
            if (f != null) {
                if (line.trim().startsWith(f)) fence = null
            } else {
                val open = fenceOpen(line)
                if (open != null) {
                    fence = open
                    openerLast = true
                } else if (i < lines.size - 1 && line.trim().isEmpty()) {
                    boundary = i + 1
                }
            }
        }
        return Fences(boundary, fence != null, openerLast)
    }

    private class StreamMemo(val text: String, val hard: Boolean, val boundary: Int, val stable: List<MarkdownBlock>)

    private val streamMemos = ArrayList<StreamMemo>()
    private const val STREAM_MEMO_CAP = 16

    /** Forget every streamed parse (tests: a cold parse of a prefix). */
    fun resetStreamMemo() {
        synchronized(streamMemos) { streamMemos.clear() }
    }

    private fun maxGroup(blocks: List<MarkdownBlock>): Int = blocks.mapNotNull { it.listGroup }.maxOrNull() ?: 0

    private fun offsetGroups(blocks: List<MarkdownBlock>, by: Int): List<MarkdownBlock> =
        if (by == 0) blocks else blocks.map { b -> b.listGroup?.let { b.copy(listGroup = it + by) } ?: b }

    /**
     * #VAPP-258 step 5 (`fixtures/markdown-render.json` streamingCases): a
     * growing prefix = the stable blocks before its last complete blank line
     * outside a fence (memoized per text: only what extends the old text
     * reparses) + the tail, whose unfinished end is held back.
     */
    fun parseStreaming(raw: String, hard: Boolean): List<MarkdownBlock> {
        val text = raw.replace("\r\n", "\n").replace("\r", "\n")
        val lines = text.split("\n")
        val memo = synchronized(streamMemos) { streamMemos.filter { it.hard == hard && text.startsWith(it.text) }.maxByOrNull { it.text.length } }
        val from = memo?.boundary ?: 0
        var stable = memo?.stable ?: emptyList()
        val boundary = scanFences(lines, from).boundary
        if (boundary > from) stable = stable + offsetGroups(parseBlocks(lines.subList(from, boundary).joinToString("\n"), hard), maxGroup(stable))
        synchronized(streamMemos) {
            streamMemos.removeAll { it.hard == hard && text.startsWith(it.text) }
            streamMemos.add(0, StreamMemo(text, hard, boundary, stable))
            while (streamMemos.size > STREAM_MEMO_CAP) streamMemos.removeAt(streamMemos.size - 1)
        }
        val tailLines = lines.subList(boundary, lines.size).toMutableList()
        if (tailLines.size == 1 && tailLines[0].isEmpty()) return stable
        val fences = scanFences(tailLines, 0)
        var tail: String
        if (fences.open) {
            // The open fence's last line waits while it is empty or only fence characters.
            if (!fences.openerLast && tailLines.last().trim().all { it == '`' || it == '~' }) tailLines.removeAt(tailLines.size - 1)
            tail = tailLines.joinToString("\n")
        } else {
            tail = tailLines.joinToString("\n")
            while (true) tail = tail.substring(0, holdTrailing(tail) ?: break)
        }
        val blocks = offsetGroups(parseBlocks(tail, hard), maxGroup(stable)).toMutableList()
        val last = blocks.removeLastOrNull()
        if (last != null) {
            val k = last.kind
            val held = if (k is MarkdownBlockKind.Paragraph || k is MarkdownBlockKind.Heading || k is MarkdownBlockKind.Quote || k is MarkdownBlockKind.ListItem) last.copy(inlines = holdBack(last.inlines)) else last
            if (!(held.kind is MarkdownBlockKind.Paragraph && held.inlines.all { it.text.isEmpty() })) blocks.add(held)
        }
        return stable + blocks
    }

    /**
     * Where the tail's unfinished end starts, outside a fence (null = it ends
     * finished): a last line that is only a list marker or heading hashes so
     * far, a trailing run of `*` / `_`, or of backticks / tildes unless they
     * are the line's only text (a fence opening or closing).
     */
    internal fun holdTrailing(tail: String): Int? {
        if (tail.isEmpty()) return null
        val lineStart = tail.lastIndexOf('\n') + 1
        val line = tail.substring(lineStart).trimStart(' ', '\t')
        val hashes = line.trimEnd(' ', '\t')
        val digits = line.takeWhile { it in '0'..'9' }.length
        val marker = line == "-" || line == "+" || line == "*" ||
            (hashes.length in 1..6 && hashes.all { it == '#' }) ||
            (digits in 1..9 && (line.length == digits || (line.length == digits + 1 && (line[digits] == '.' || line[digits] == ')'))))
        if (marker) return maxOf(lineStart - 1, 0)
        val stars = tail.length - tail.trimEnd('*', '_').length
        if (stars > 0) return tail.length - stars
        val ticks = tail.length - tail.trimEnd('`', '~').length
        if (ticks > 0) {
            val start = tail.length - ticks
            if (!tail.substring(minOf(lineStart, start), start).all { it == ' ' || it == '\t' }) return start
        }
        return null
    }

    /**
     * The streaming tail's last block: its plain runs (no flags, link or chip)
     * hold back what would flicker: an unfinished link at the end paints as
     * its label, and every opening delimiter run is dropped (a maximal run of
     * `*` / `_` / backticks, or of 2+ `~`, not after an ASCII letter, digit or
     * delimiter, followed by a non-space or ending the run).
     */
    internal fun holdBack(inlines: List<MarkdownInline>): List<MarkdownInline> {
        fun plain(i: MarkdownInline) = !i.bold && !i.italic && !i.code && !i.strike && i.link == null && i.chip == null
        val out = inlines.toMutableList()
        val last = out.lastOrNull()
        if (last != null && plain(last)) {
            val open = last.text.lastIndexOf('[')
            if (open >= 0) {
                val rest = last.text.substring(open + 1)
                val labelEnd = rest.indexOf(']').let { if (it < 0) rest.length else it }
                val after = rest.substring(labelEnd)
                val unfinished = after.isEmpty() || after == "]" || (after.startsWith("](") && after.substring(2).all { it != '(' && it != ')' && !it.isWhitespace() })
                if (unfinished) {
                    val start = if (open > 0 && last.text[open - 1] == '!') open - 1 else open
                    out[out.size - 1] = last.copy(text = last.text.substring(0, start) + rest.substring(0, labelEnd))
                }
            }
        }
        return out.map { if (plain(it)) it.copy(text = dropDelimiters(it.text)) else it }.filter { it.text.isNotEmpty() || it.chip != null }
    }

    internal fun dropDelimiters(s: String): String {
        val out = StringBuilder(s.length)
        fun delim(c: Char) = c == '*' || c == '_' || c == '`'
        fun alnum(c: Char) = c in '0'..'9' || c in 'a'..'z' || c in 'A'..'Z'
        var i = 0
        while (i < s.length) {
            val c = s[i]
            val tilde = c == '~'
            if (delim(c) || tilde) {
                var j = i
                while (j < s.length && (if (tilde) s[j] == '~' else delim(s[j]))) j++
                val beforeOk = i == 0 || !(alnum(s[i - 1]) || delim(s[i - 1]) || s[i - 1] == '~')
                val afterOk = j == s.length || !s[j].isWhitespace()
                if (!(beforeOk && afterOk && (!tilde || j - i >= 2))) out.append(s, i, j)
                i = j
                continue
            }
            out.append(c)
            i++
        }
        return out.toString()
    }

    private fun parseBlocks(text: String, hard: Boolean): List<MarkdownBlock> {
        val normalized = text.replace("\r\n", "\n").replace("\r", "\n")
        val lines = normalized.split("\n")
        val blocks = ArrayList<MarkdownBlock>()
        val para = ArrayList<String>()
        var group = 0
        fun flush() {
            if (para.isNotEmpty()) {
                val joined = joinParagraph(para, hard)
                val image = imageOnly(joined)
                if (image != null) {
                    blocks.add(MarkdownBlock(MarkdownBlockKind.Image(image.second, image.first), listOf(MarkdownInline(text = image.first))))
                } else {
                    blocks.addAll(splitImages(joined))
                }
                para.clear()
            }
        }
        var i = 0
        while (i < lines.size) {
            val line = lines[i]
            val trimmed = line.trim()
            if (trimmed.startsWith("```") || trimmed.startsWith("~~~")) {
                flush()
                val fence = trimmed.substring(0, 3)
                val info = trimmed.trimStart('`', '~').trim().split(Regex("\\s+")).firstOrNull()?.takeIf { it.isNotEmpty() }
                val body = ArrayList<String>()
                i += 1
                while (i < lines.size && !lines[i].trim().startsWith(fence)) {
                    body.add(lines[i])
                    i += 1
                }
                i += 1
                blocks.add(MarkdownBlock(MarkdownBlockKind.CodeBlock, listOf(MarkdownInline(text = body.joinToString("\n"), code = true)), lang = info))
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
                // Paragraphs join with a space; under `hard` a paragraph's lines with a newline.
                val joined = StringBuilder()
                var gap = false
                for (l in body) {
                    if (l.isEmpty()) { gap = true; continue }
                    if (joined.isNotEmpty()) joined.append(if (hard && !gap) '\n' else ' ')
                    joined.append(l)
                    gap = false
                }
                blocks.add(MarkdownBlock(MarkdownBlockKind.Quote, parseInline(joined.toString())))
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
                // The marker columns of the open lists, outermost first: a
                // marker ≥ 2 columns right of the previous level's nests.
                val columns = ArrayList<Int>()
                var lastBody = ""
                while (i < lines.size) {
                    val item = listMarker(lines[i])
                    if (item == null) {
                        // A non-blank line under an item that starts no block continues it.
                        val next = lines[i].trim()
                        if (next.isEmpty() || startsBlock(lines[i]) || blocks.isEmpty()) break
                        lastBody += (if (hard) "\n" else " ") + next
                        blocks[blocks.size - 1] = blocks.last().copy(inlines = parseInline(lastBody))
                        i += 1
                        continue
                    }
                    val (marker, rest, column) = item
                    while (columns.size > 1 && column < columns.last()) columns.removeAt(columns.size - 1)
                    if (columns.isEmpty() || column >= columns.last() + 2) columns.add(column)
                    val depth = columns.size - 1
                    var task: Boolean? = null
                    var body = rest
                    if (rest.startsWith("[ ] ")) {
                        task = false
                        body = rest.substring(4)
                    } else if (rest.startsWith("[x] ") || rest.startsWith("[X] ")) {
                        task = true
                        body = rest.substring(4)
                    }
                    lastBody = body.trim()
                    blocks.add(MarkdownBlock(MarkdownBlockKind.ListItem(marker, task, depth), parseInline(lastBody), group))
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
            if (trimmed.isEmpty()) flush() else para.add(line.trimStart(' ', '\t'))
            i += 1
        }
        flush()
        return blocks
    }

    /**
     * Image blocks whose src [allowed] refuses become a paragraph of their
     * alt text (dropped without one): what the measurer and the painter
     * both lay out, so a denied image never reserves a box.
     */
    fun resolveImages(blocks: List<MarkdownBlock>, allowed: (String) -> Boolean): List<MarkdownBlock> = blocks.mapNotNull { b ->
        val kind = b.kind
        when {
            kind !is MarkdownBlockKind.Image || allowed(kind.src) -> b
            kind.alt.isEmpty() -> null
            else -> MarkdownBlock(MarkdownBlockKind.Paragraph, listOf(MarkdownInline(text = kind.alt)))
        }
    }

    /**
     * Every link href through [policy]: its result (the resolved href), or
     * null = the span paints as plain text and never navigates. Only the
     * marks change, so the layout is the same.
     */
    fun policeLinks(blocks: List<MarkdownBlock>, policy: (String) -> String?): List<MarkdownBlock> {
        val memo = HashMap<String, String?>()
        fun spans(list: List<MarkdownInline>): List<MarkdownInline> = list.map { span ->
            val href = span.link ?: return@map span
            span.copy(link = memo.getOrPut(href) { policy(href) })
        }
        return blocks.map { b ->
            val kind = b.kind
            b.copy(
                kind = if (kind is MarkdownBlockKind.Table) kind.copy(header = kind.header.map(::spans), rows = kind.rows.map { r -> r.map(::spans) }) else kind,
                inlines = spans(b.inlines),
            )
        }
    }

    /**
     * `fixtures/markdown-text-links.json`: every text run through a host's text
     * links: [segments] = one piece's answer, policed (text to href), null =
     * text. Code, link labels, block images and bare urls never ask; only the
     * marks change, so the layout is the same.
     */
    fun linkText(blocks: List<MarkdownBlock>, segments: (String) -> List<Pair<String, String?>>?): List<MarkdownBlock> =
        linkSegments(blocks) { piece -> segments(piece)?.map { (text, href) -> MarkdownTextLink(text, href) } }

    /**
     * #VAPP-258 [linkText] with chips (fixtures/markdown-host.json): a
     * segment's chip stays on its run only when the run is a link (an
     * allowed href) and the chip is [MarkdownChip.valid].
     */
    fun linkSegments(blocks: List<MarkdownBlock>, segments: (String) -> List<MarkdownTextLink>?): List<MarkdownBlock> {
        fun spans(list: List<MarkdownInline>): List<MarkdownInline> = list.flatMap { span ->
            if (span.code || span.link != null) return@flatMap listOf(span)
            bareUrlPieces(span.text).flatMap { (piece, isUrl) ->
                val linked = if (isUrl) null else segments(piece)
                linked?.filter { it.text.isNotEmpty() }?.map { seg ->
                    span.copy(text = seg.text, link = seg.href, chip = seg.chip?.takeIf { seg.href != null && it.valid })
                } ?: listOf(span.copy(text = piece))
            }
        }
        return blocks.map { b ->
            when (val kind = b.kind) {
                is MarkdownBlockKind.Table -> b.copy(kind = kind.copy(header = kind.header.map(::spans), rows = kind.rows.map { r -> r.map(::spans) }))
                is MarkdownBlockKind.Image, is MarkdownBlockKind.Attachment, MarkdownBlockKind.CodeBlock, MarkdownBlockKind.Rule -> b
                else -> b.copy(inlines = spans(b.inlines))
            }
        }
    }

    /**
     * #VAPP-258: a TOP-LEVEL paragraph that is exactly one markdown link
     * (every run carries that one link) whose RAW destination [classify]
     * answers validly and whose href [policy] allows (null policy = as
     * written) becomes an [MarkdownBlockKind.Attachment] block. Run BEFORE
     * [policeLinks] and the text links (it reads the raw destination).
     */
    fun resolveAttachments(blocks: List<MarkdownBlock>, policy: ((String) -> String?)?, classify: (String) -> MarkdownAttachment?): List<MarkdownBlock> = blocks.map { b ->
        if (b.kind != MarkdownBlockKind.Paragraph) return@map b
        val runs = b.inlines.filter { it.text.isNotEmpty() }
        val raw = runs.firstOrNull()?.link ?: return@map b
        if (runs.any { it.link != raw }) return@map b
        val href = (if (policy == null) raw else policy(raw)) ?: return@map b
        val a = classify(raw) ?: return@map b
        if (a.type !in MarkdownAttachment.KINDS || a.name.isEmpty()) return@map b
        b.copy(kind = MarkdownBlockKind.Attachment(a.type, a.name, href))
    }

    /** #VAPP-258: every block image's natural size from [size] (asked with the raw src; an invalid answer = none). */
    fun sizeImages(blocks: List<MarkdownBlock>, size: (String) -> MarkdownSize?): List<MarkdownBlock> = blocks.map { b ->
        val kind = b.kind
        if (kind !is MarkdownBlockKind.Image) return@map b
        val s = size(kind.src)?.takeIf { it.valid } ?: return@map b
        b.copy(kind = kind.copy(width = s.width, height = s.height))
    }

    /**
     * #VAPP-258 (markdown-host.json `imageBoxes` `loaded`): every block image
     * the host could not size takes the size it decoded at ([loaded]; an
     * invalid answer = none, the placeholder box). The same list when none did.
     */
    fun sizeLoaded(blocks: List<MarkdownBlock>, loaded: (String) -> MarkdownSize?): List<MarkdownBlock> {
        var changed = false
        val out = blocks.map { b ->
            val kind = b.kind
            if (kind !is MarkdownBlockKind.Image || (kind.width != null && kind.height != null)) return@map b
            val s = loaded(kind.src)?.takeIf { it.valid } ?: return@map b
            changed = true
            b.copy(kind = kind.copy(width = s.width, height = s.height))
        }
        return if (changed) out else blocks
    }

    /** The size a block image's box uses: the host's ([size]), else the decoded one ([loaded]), else none. */
    fun imageSizeOf(size: MarkdownSize?, loaded: MarkdownSize?): MarkdownSize? = size?.takeIf { it.valid } ?: loaded?.takeIf { it.valid }

    /**
     * #VAPP-258 (markdown-host.json `imageBoxes`): a block image's box. With
     * a [maxHeight] cap: its natural size scaled by min(1, available / width,
     * maxHeight / height), start-aligned; unknown = available × maxHeight.
     * With none (null): the available width at its aspect (4:3 unknown),
     * scaled down to [viewport] when taller (null viewport = no limit).
     */
    fun imageBox(width: Float?, height: Float?, available: Float, maxHeight: Float?, viewport: Float? = null): MarkdownSize {
        val avail = max(available, 0f)
        val sized = width != null && height != null && MarkdownSize(width, height).valid
        if (maxHeight == null) {
            val ratio = if (sized) height!! / width!! else MarkdownOptions.UNSIZED_ASPECT
            val h = avail * ratio
            if (viewport != null && viewport.isFinite() && viewport > 0f && h > viewport) return MarkdownSize(viewport / ratio, viewport)
            return MarkdownSize(avail, h)
        }
        if (!sized) return MarkdownSize(avail, maxHeight)
        val scale = minOf(1f, avail / width!!, maxHeight / height!!)
        return MarkdownSize(width * scale, height * scale)
    }

    /** #VAPP-258: [text] parsed and resolved through [hooks] up to (not including) the text links. */
    fun prepare(text: String, hooks: MarkdownHooks): List<MarkdownBlock> {
        var blocks = parse(text, hooks.options.hardBreaks, hooks.options.streaming)
        hooks.attachment?.let { blocks = resolveAttachments(blocks, hooks.linkPolicy, it) }
        hooks.imagePolicy?.let { blocks = resolveImages(blocks, it) }
        hooks.imageSize?.let { blocks = sizeImages(blocks, it) }
        hooks.loadedSize?.let { blocks = sizeLoaded(blocks, it) }
        hooks.linkPolicy?.let { blocks = policeLinks(blocks, it) }
        return blocks
    }

    /** #VAPP-258: [prepare]d blocks through the text links. */
    fun linked(blocks: List<MarkdownBlock>, hooks: MarkdownHooks): List<MarkdownBlock> =
        hooks.textLinks?.let { linkSegments(blocks, it) } ?: blocks

    /** #VAPP-258: THE pipeline the measurer and the painter share: [prepare] then [linked]. */
    fun resolve(text: String, hooks: MarkdownHooks): List<MarkdownBlock> = linked(prepare(text, hooks), hooks)

    /** A run cut around its bare urls (`http://` or `https://`, any case, not glued to an ASCII word character, up to the next ASCII space, tab or newline); no empty piece. */
    internal fun bareUrlPieces(text: String): List<Pair<String, Boolean>> {
        fun isWord(c: Char) = c in '0'..'9' || c in 'A'..'Z' || c in 'a'..'z' || c == '_'
        val out = ArrayList<Pair<String, Boolean>>()
        var at = 0
        var i = 0
        while (i < text.length) {
            val url = (i == 0 || !isWord(text[i - 1])) && (text.startsWith("http://", i, ignoreCase = true) || text.startsWith("https://", i, ignoreCase = true))
            if (!url) {
                i++
                continue
            }
            var end = i
            while (end < text.length && text[end] != ' ' && text[end] != '\t' && text[end] != '\n') end++
            if (i > at) out.add(text.substring(at, i) to false)
            out.add(text.substring(i, end) to true)
            at = end
            i = end
        }
        if (text.length > at) out.add(text.substring(at) to false)
        return out
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
                MarkdownBlockKind.Paragraph, is MarkdownBlockKind.Heading -> text.height(b.inlines, spec, max(width, 1f), s.chip)
                is MarkdownBlockKind.ListItem -> text.height(b.inlines, spec, max(width - s.listInset(k.depth), 1f), s.chip)
                is MarkdownBlockKind.Image -> imageBox(k.width, k.height, width, s.imageHeight, s.viewportHeight).height
                is MarkdownBlockKind.Attachment -> s.attachmentHeight(k, width)
                MarkdownBlockKind.Quote -> text.height(b.inlines, spec, max(width - s.quoteBorder - s.quotePad, 1f), s.chip)
                MarkdownBlockKind.CodeBlock -> text.height(b.inlines, spec, null) + 2 * (s.codePad + s.codeBorder)
                MarkdownBlockKind.Rule -> 1f
                is MarkdownBlockKind.Table -> {
                    val cols = maxOf(k.header.size, k.rows.maxOfOrNull { it.size } ?: 0, 1)
                    val cellW = max(width / cols - 2 * s.cellPadH, 1f)
                    // #EXP-1728: the header row wraps in the semibold it paints in.
                    val head = s.body.copy(weight = maxOf(s.body.weight, 600))
                    for ((r, row) in (listOf(k.header) + k.rows).withIndex()) {
                        val h = row.map { text.height(it, if (r == 0) head else s.body, cellW, s.chip) }.fold(s.body.lineHeight) { a, x -> max(a, x) }
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

    /**
     * #VAPP-258: the tallest height at most [limit] that ends on a whole line
     * of [layout] (a fold that never cuts a line in half): a block that fits
     * ends there; the first block that does not fit contributes its whole
     * lines (text blocks: line boxes of their spec's line height, a code
     * block inside its padding; a table its whole rows); anything else stops
     * at the block above. Nothing fits = the first line's bottom (capped at
     * [limit]); a document within [limit] = its height.
     */
    fun foldHeight(blocks: List<MarkdownBlock>, layout: MarkdownLayout, s: MarkdownStyles, limit: Float): Float {
        if (layout.height <= limit) return layout.height
        var best = 0f
        for ((i, b) in blocks.withIndex()) {
            val box = layout.blocks.getOrNull(i) ?: break
            val bottom = box.y + box.height
            if (bottom <= limit) {
                best = bottom
                continue
            }
            val lh = s.spec(b.kind).lineHeight
            when (b.kind) {
                MarkdownBlockKind.Paragraph, is MarkdownBlockKind.Heading, is MarkdownBlockKind.ListItem, MarkdownBlockKind.Quote -> {
                    val n = floor(((limit - box.y) / lh).toDouble()).toInt()
                    if (n >= 1) best = box.y + n * lh else if (best == 0f) best = minOf(box.y + lh, limit)
                }
                MarkdownBlockKind.CodeBlock -> {
                    val n = floor(((limit - box.y - s.codePad - s.codeBorder) / lh).toDouble()).toInt()
                    if (n >= 1) best = box.y + s.codePad + s.codeBorder + n * lh
                }
                is MarkdownBlockKind.Table -> {
                    var y = box.y
                    for (r in box.rows) {
                        if (y + r > limit) break
                        y += r
                    }
                    if (y > box.y) best = y
                }
                else -> Unit
            }
            break
        }
        return if (best > 0f) best else limit
    }

    /** An audio attachment's max-content width (the AudioPlayer's). */
    const val AUDIO_INTRINSIC_WIDTH = 300f

    /** A file attachment's max-content width (the FileUpload's). */
    const val FILE_INTRINSIC_WIDTH = 240f

    /** Max-content width: the widest block on one line. */
    fun maxContentWidth(blocks: List<MarkdownBlock>, s: MarkdownStyles, text: MarkdownTextMeasure): Float {
        val widths = blocks.map { b ->
            val spec = s.spec(b.kind)
            when (val k = b.kind) {
                MarkdownBlockKind.Paragraph, is MarkdownBlockKind.Heading -> text.width(b.inlines, spec, s.chip)
                is MarkdownBlockKind.ListItem -> text.width(b.inlines, spec, s.chip) + s.listInset(k.depth)
                is MarkdownBlockKind.Image -> if (k.width != null && k.height != null) imageBox(k.width, k.height, k.width, s.imageHeight ?: k.height, s.viewportHeight).width else (s.imageHeight ?: MarkdownOptions.IMAGE_MAX_HEIGHT)
                is MarkdownBlockKind.Attachment -> when (k.type) {
                    "video" -> s.mediaMaxWidth
                    "audio" -> AUDIO_INTRINSIC_WIDTH
                    else -> FILE_INTRINSIC_WIDTH
                }
                MarkdownBlockKind.Quote -> text.width(b.inlines, spec, s.chip) + s.quoteBorder + s.quotePad
                MarkdownBlockKind.CodeBlock -> {
                    val first = b.inlines.firstOrNull() ?: MarkdownInline(text = "", code = true)
                    (first.text.split("\n").maxOfOrNull { text.width(listOf(MarkdownInline(text = it, code = true)), spec) } ?: 0f) + 2 * (s.codePad + s.codeBorder)
                }
                MarkdownBlockKind.Rule -> 0f
                is MarkdownBlockKind.Table -> {
                    val cols = max(k.header.size, 1).toFloat()
                    val widest = (listOf(k.header) + k.rows).flatten().maxOfOrNull { text.width(it, s.body, s.chip) } ?: 0f
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
                is MarkdownBlockKind.ListItem -> text.widestWord(b.inlines, spec, s.chip) + s.listInset(k.depth)
                is MarkdownBlockKind.Image, is MarkdownBlockKind.Attachment -> 0f
                MarkdownBlockKind.Quote -> text.widestWord(b.inlines, spec, s.chip) + s.quoteBorder + s.quotePad
                else -> text.widestWord(b.inlines, spec, s.chip)
            }
        }
        return widths.maxOrNull()?.let { ceil(it) } ?: 0f
    }
}
