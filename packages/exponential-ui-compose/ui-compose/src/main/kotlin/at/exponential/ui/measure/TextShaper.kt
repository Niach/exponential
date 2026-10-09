package at.exponential.ui.measure

import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownInline
import at.exponential.ui.primitives.MarkdownTextMeasure
import at.exponential.ui.primitives.MarkdownTextSpec
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/** A theme family NAME → the Compose family (null = not available here). */
fun interface FontResolver {
    /** The family for `name` (null name = the theme's default). */
    fun family(name: String?): FontFamily?
}

/**
 * Text shaping for the measurer AND the text leaves: one Compose
 * [TextMeasurer] answers taffy's questions and the views paint the result
 * with the SAME [style], so a measured height is the painted height. Line
 * height follows CSS (`n` lines = `n × lineHeight`). Sizes in and out are
 * dp; [density] converts the measurer's px (its Density pins fontScale 1).
 */
class TextShaper(val measurer: TextMeasurer, val density: Float, val fonts: FontResolver) {
    private val widthCache = object : LinkedHashMap<Pair<Any, TextStyle>, Float>(256, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Pair<Any, TextStyle>, Float>?): Boolean = size > 4096
    }

    /**
     * The surface default family NAME (the theme's `sans`, set by
     * `SurfaceModel.textShaper`): a style without a family takes it, as CSS
     * inherits the surface's font and gpui's `Fonts::family(None)`.
     */
    var defaultFamily: String? = null

    /** The Compose family of a theme family NAME (null = [defaultFamily]; registered, else monospace / default). */
    fun family(name: String?): FontFamily {
        val n = name ?: defaultFamily
        return fonts.family(n) ?: if (n != null && looksMono(n)) FontFamily.Monospace else FontFamily.Default
    }

    /** A family NAME that asks for a monospace face (the fallback when it is not registered). */
    private fun looksMono(name: String): Boolean {
        val n = name.lowercase()
        return n == "ui-monospace" || n == "monospace" || n == "ui-mono" || "mono" in n || "code" in n || "courier" in n || "consolas" in n || "menlo" in n
    }

    /** The Compose style a resolved text style renders with (fontScale-free sp = dp). */
    fun style(ts: ResolvedTextStyle, italic: Boolean = ts.italic, family: String? = null, align: TextAlign? = null, rtl: Boolean? = null): TextStyle = TextStyle(
        fontSize = ts.fontSize.sp,
        lineHeight = ts.lineHeight.sp,
        fontWeight = FontWeight(ts.fontWeight.coerceIn(1, 1000)),
        fontStyle = if (italic) FontStyle.Italic else FontStyle.Normal,
        fontFamily = family(family ?: ts.fontFamily),
        letterSpacing = ts.letterSpacing?.takeIf { it != 0f }?.sp ?: TextUnit.Unspecified,
        textAlign = align ?: TextAlign.Unspecified,
        // The bidi paragraph direction = the node's direction (round 2 §2).
        textDirection = when (rtl) {
            true -> TextDirection.Rtl
            false -> TextDirection.Ltr
            null -> TextDirection.Unspecified
        },
        lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
        platformStyle = PlatformTextStyle(includeFontPadding = false),
        // Unhinted fractional advances, as Chromium, gpui and CoreText lay
        // text out; the default (Static: hinted, whole-pixel advances) is
        // ~0.3 px wider per glyph at density 3.
        textMotion = TextMotion.Animated,
    )

    private val baselines = HashMap<ResolvedTextStyle, Float>()

    /** The first baseline (dp) of one line in `ts` from the line box top. */
    fun baseline(ts: ResolvedTextStyle): Float = baselines.getOrPut(ts) {
        val r = layout(AnnotatedString("Hg"), style(ts), null, 1)
        r.firstBaseline / density
    }

    private fun layout(text: AnnotatedString, style: TextStyle, wrapPx: Int?, maxLines: Int = Int.MAX_VALUE): TextLayoutResult =
        measurer.measure(
            text,
            style,
            overflow = TextOverflow.Clip,
            softWrap = wrapPx != null,
            maxLines = maxLines,
            constraints = if (wrapPx != null) Constraints(maxWidth = max(wrapPx, 1)) else Constraints(),
        )

    /**
     * The width (dp) of `text` on one line (the widest `\n` line): the
     * shaped FRACTIONAL extent, as the web and gpui lay it out. Compose's
     * intrinsic width is ceiled to a device pixel, which adds up along a
     * row of labels; the painters give text
     * [at.exponential.ui.compose.TEXT_SLACK_PX] instead, so a frame of this
     * width rounded to px never wraps or ellipsizes it.
     */
    fun lineWidth(text: AnnotatedString, style: TextStyle): Float {
        if (text.isEmpty()) return 0f
        val key: Pair<Any, TextStyle> = (if (text.spanStyles.isEmpty()) text.text else text) to style
        widthCache[key]?.let { return it }
        val r = layout(text, style, null)
        var px = 0f
        for (i in 0 until r.lineCount) px = max(px, r.getLineRight(i) - r.getLineLeft(i))
        val w = px / density
        widthCache[key] = w
        return w
    }

    /** The width (dp) of a plain string on one line. */
    fun lineWidth(text: String, style: TextStyle): Float = lineWidth(AnnotatedString(text), style)

    /** Max-content width of `text` (the widest line of a multi-line text). */
    fun maxContent(text: String, ts: ResolvedTextStyle): Float {
        val st = style(ts)
        return text.split("\n").maxOfOrNull { lineWidth(it, st) } ?: 0f
    }

    /** Min-content width: the widest whitespace-separated word. */
    fun minContent(text: String, ts: ResolvedTextStyle): Float {
        val st = style(ts)
        return text.split(WHITESPACE).filter { it.isNotEmpty() }.maxOfOrNull { lineWidth(it, st) } ?: 0f
    }

    /** The height (dp) of `text` wrapped at `wrap` dp (null = no wrap), at most `clamp` lines, never less than one line. */
    fun wrappedHeight(text: AnnotatedString, style: TextStyle, lineHeight: Float, wrap: Float?, clamp: Int? = null): Float {
        if (text.isEmpty()) return lineHeight
        val px = wrap?.let { ceil(max(it, 1f) * density).toInt() }
        val lines = layout(text, style, px).lineCount
        var h = max(lines, 1) * lineHeight
        if (clamp != null && clamp > 0) h = min(h, clamp * lineHeight)
        return max(h, lineHeight)
    }

    /** [wrappedHeight] of a plain string in a resolved style. */
    fun wrappedHeight(text: String, ts: ResolvedTextStyle, wrap: Float?, clamp: Int? = null): Float =
        wrappedHeight(AnnotatedString(text), style(ts), ts.lineHeight, wrap, clamp)

    /**
     * Text content size (dp) at an inner wrap width (null = max-content,
     * 0 = min-content), the gpui rule: a one-line text is `nowrap`, so its
     * min-content is the whole line (a Badge around it never gets narrower);
     * the core makes the leaf clip on x, so it still ellipsizes in a row.
     */
    fun measure(text: String, ts: ResolvedTextStyle, wrap: Float?, lines: Int?): Size {
        val single = lines == 1
        return when {
            wrap == null -> Size(maxContent(text, ts), wrappedHeight(text, ts, null, lines))
            wrap <= 0f -> Size(if (single) maxContent(text, ts) else minContent(text, ts), ts.lineHeight)
            else -> {
                val maxW = maxContent(text, ts)
                val used = if (single) min(maxW, wrap) else min(maxW, max(wrap, minContent(text, ts)))
                val h = if (single) ts.lineHeight else wrappedHeight(text, ts, max(used, 1f), lines)
                Size(used, h)
            }
        }
    }

    /**
     * The runs of some markdown spans in a base spec (bold / italic / code /
     * link / strike) as an AnnotatedString, painted with [markdownStyle].
     */
    fun runs(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, mono: String?, ink: Color, link: Color, codeBackground: Color?): AnnotatedString {
        val b = AnnotatedString.Builder()
        for (span in inlines) {
            if (span.text.isEmpty()) continue
            val start = b.length
            b.append(span.text)
            val decorations = listOfNotNull(
                if (span.strike) TextDecoration.LineThrough else null,
                if (span.link != null) TextDecoration.Underline else null,
            )
            b.addStyle(
                SpanStyle(
                    color = if (span.link != null) link else ink,
                    fontWeight = if (span.bold) FontWeight(max(spec.weight, 600)) else null,
                    fontStyle = if (span.italic) FontStyle.Italic else null,
                    fontFamily = if (span.code) family(mono ?: "ui-monospace") else null,
                    textDecoration = if (decorations.isEmpty()) null else TextDecoration.combine(decorations),
                    background = if (span.code && codeBackground != null) codeBackground else Color.Unspecified,
                ),
                start,
                b.length,
            )
        }
        return b.toAnnotatedString()
    }

    /** The paragraph style a markdown spec paints with (the base the [runs] spans override). */
    fun markdownStyle(spec: MarkdownTextSpec, align: TextAlign? = null): TextStyle =
        style(ResolvedTextStyle(spec.size, spec.weight, spec.lineHeight, spec.family), align = align)

    private companion object {
        val WHITESPACE = Regex("\\s+")
    }
}

/** Markdown measurement through the shaper (the painter's `MdText`): what [Markdown.layout] asks. */
class MarkdownShaper(val shaper: TextShaper, val mono: String?) : MarkdownTextMeasure {
    private fun annotated(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): AnnotatedString =
        shaper.runs(inlines, spec, mono, Color.Black, Color.Black, null)

    override fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): Float {
        val a = annotated(inlines, spec)
        if (a.isEmpty()) return spec.lineHeight
        return shaper.wrappedHeight(a, shaper.markdownStyle(spec), spec.lineHeight, width)
    }

    /**
     * Max-content: the widest hard line shaped WITH its runs (a bold or code
     * run is wider than the body font says; measured without them, the
     * paragraph wraps at its own max-content width, round 2 §7 `main-md`).
     */
    override fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float =
        shaper.lineWidth(annotated(inlines, spec), shaper.markdownStyle(spec))

    /** Min-content: the widest unbreakable segment, each shaped with the runs it covers. */
    override fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float {
        val a = annotated(inlines, spec)
        val style = shaper.markdownStyle(spec)
        return WORD.findAll(a.text).maxOfOrNull { shaper.lineWidth(a.subSequence(it.range.first, it.range.last + 1), style) } ?: 0f
    }

    private companion object {
        val WORD = Regex("\\S+")
    }
}
