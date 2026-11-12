package at.exponential.ui.primitives

import androidx.compose.foundation.Canvas
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.text.selection.DisableSelection
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.isSpecified
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.LinkInteractionListener
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.ceil
import kotlin.math.roundToInt

/** #VAPP-258: one chip kind's colours (Unspecified / null = the base chip's). */
data class MarkdownChipLook(
    val background: Color? = null,
    val border: Color = Color.Unspecified,
    val borderWidth: Float? = null,
    val ink: Color = Color.Unspecified,
    val identInk: Color = Color.Unspecified,
)

/** The colours a [MarkdownView] paints with. */
@Immutable
data class MarkdownColors(
    /** Body text. */
    val ink: Color,
    /** List markers, quote text. */
    val muted: Color,
    /** Link text (underlined). */
    val link: Color,
    /** Rules and table grid lines. */
    val border: Color,
    /** Inline code background (null = none). */
    val codeBackground: Color?,
    /** Fenced code block background. */
    val codeBlockBackground: Color,
    /** The quote bar. */
    val quoteBorder: Color,
    /** Round 9 §9.1: an inline-profile paragraph's and heading's text (Unspecified = [ink]). */
    val paragraphInk: Color = Color.Unspecified,
    val headingInk: Color = Color.Unspecified,
    /** A GFM task box = THE Checkbox box: the unchecked fill and hairline. */
    val taskFill: Color = Color.Transparent,
    /** Unspecified = [border]. */
    val taskBorder: Color = Color.Unspecified,
    /** The checked fill + hairline (Unspecified = [link]). */
    val taskChecked: Color = Color.Unspecified,
    /** The check on [taskChecked]. */
    val taskCheck: Color = Color.White,
    /** #VAPP-258: the inline chip = the Chip `root` recipe's fill, border (drawn inside) and the `label` colour (Unspecified = [ink]). */
    val chipBackground: Color = Color.Transparent,
    val chipBorder: Color = Color.Unspecified,
    val chipBorderWidth: Float = 0f,
    val chipInk: Color = Color.Unspecified,
    /** #VAPP-258: a block image's border = the `Markdown/image` recipe's (drawn inside its corners, the box unchanged; 0 = none). */
    val imageBorder: Color = Color.Unspecified,
    val imageBorderWidth: Float = 0f,
    /** #VAPP-258: a fenced block's border colour (its width = [MarkdownStyles.codeBorder]; Unspecified = [border]). */
    val codeBlockBorder: Color = Color.Unspecified,
    /** #VAPP-258: a table header row's fill (`Markdown/headerCell`). */
    val headerCellBackground: Color = Color.Transparent,
    /** #VAPP-258: a theme's `Markdown/chip*` colours per chip kind (over the chip* fields above). */
    val chipLooks: Map<String, MarkdownChipLook> = emptyMap(),
) {
    /** Factories over the tokens. */
    companion object {
        /** foreground / mutedForeground / primary / border / muted from [tokens]. */
        fun from(tokens: PrimitiveTokens): MarkdownColors = MarkdownColors(
            ink = tokens.foreground,
            muted = tokens.mutedForeground,
            link = tokens.primary,
            border = tokens.border,
            codeBackground = tokens.muted,
            codeBlockBackground = tokens.muted,
            quoteBorder = tokens.border,
            taskFill = tokens.card,
            taskBorder = tokens.input,
            taskChecked = tokens.primary,
            taskCheck = tokens.primaryForeground,
            chipBackground = tokens.background,
            chipBorder = tokens.mutedForeground,
            chipBorderWidth = 1f,
            chipInk = tokens.mutedForeground,
        )
    }
}

/** #VAPP-258: one coloured range `[start, end)` of a fenced block's text. */
@Immutable
data class MarkdownCodeSpan(val start: Int, val end: Int, val color: Color)

/**
 * #VAPP-258 (fixtures/markdown-render.json): colours a fenced block's text
 * (its fence's info string as [lang]): the renderer's CodeBlock tokens in
 * CodeBlock's colours. Colour only: the block's font, box and height stay.
 */
fun interface MarkdownCodeHighlighter {
    /** The coloured ranges of [text] (empty = plain). */
    fun spans(text: String, lang: String?): List<MarkdownCodeSpan>
}

/**
 * Resolves a [MarkdownTextSpec.family] name (or the mono family for code
 * spans) to a Compose family; null = the default family.
 */
fun interface MarkdownFontResolver {
    /** The family for [name] (null name = the default face). */
    fun family(name: String?, mono: Boolean): FontFamily?
}

/**
 * Builds the Compose text of [Markdown] inlines and measures it in dp
 * through ONE [TextMeasurer], so the layout's heights equal the painted
 * ones (`n lines × lineHeight`). [density] is the px-per-dp the measurer
 * runs at; sizes are taken as sp == dp (pin `fontScale` to 1 around the
 * paint, as [MarkdownView] does).
 */
class MarkdownComposeText(
    private val measurer: TextMeasurer,
    private val density: Float,
    private val fonts: MarkdownFontResolver = MarkdownFontResolver { _, mono -> if (mono) FontFamily.Monospace else null },
    private val monoFamily: String? = null,
) : MarkdownTextMeasure {
    /** The paragraph style of [spec]: fixed line box, no font padding. */
    fun style(spec: MarkdownTextSpec, color: Color = Color.Unspecified): TextStyle = TextStyle(
        color = color,
        fontSize = spec.size.sp,
        fontWeight = FontWeight(spec.weight),
        lineHeight = spec.lineHeight.sp,
        fontFamily = fonts.family(spec.family, mono = false),
        lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
        platformStyle = PlatformTextStyle(includeFontPadding = false),
        // Fractional advances, as the SDK measurer (`TextShaper.style`).
        textMotion = TextMotion.Animated,
    )

    /**
     * The spans of [inlines] on [spec]: bold ≥ 600, italic, strike, mono code,
     * underlined links. Round 9: with [code], a code span takes its type and
     * [MARKDOWN_CODE_PAD] on each side ([MarkdownPills] paints its fill).
     */
    fun annotated(
        inlines: List<MarkdownInline>,
        spec: MarkdownTextSpec,
        ink: Color = Color.Unspecified,
        link: Color = Color.Unspecified,
        codeBackground: Color? = null,
        boldFloor: Int? = null,
        onLink: ((String) -> Unit)? = null,
        code: MarkdownCodeStyle? = null,
    ): AnnotatedString = buildAnnotatedString {
        val mono = fonts.family(monoFamily, mono = true)
        var chips = 0
        for (span in inlines) {
            if (span.text.isEmpty()) continue
            val chip = span.chip
            if (chip != null) {
                // #VAPP-258: the chip is a placeholder (its label = the alternate text, the accessible name).
                appendInlineContent(markdownChipId(chips++), chip.label)
                continue
            }
            var weight = spec.weight
            if (span.bold) weight = maxOf(weight, 600)
            if (boldFloor != null) weight = maxOf(weight, boldFloor)
            val decorations = buildList {
                if (span.strike) add(TextDecoration.LineThrough)
                if (span.link != null) add(TextDecoration.Underline)
            }
            val style = SpanStyle(
                color = if (span.link != null) link else ink,
                fontWeight = FontWeight(weight),
                fontStyle = if (span.italic) FontStyle.Italic else null,
                fontSize = if (span.code && code != null) code.size(spec).sp else TextUnit.Unspecified,
                fontFamily = if (span.code) mono else null,
                textDecoration = if (decorations.isEmpty()) null else TextDecoration.combine(decorations),
                background = if (span.code && code == null && codeBackground != null) codeBackground else Color.Unspecified,
            )
            val pill = span.code && code != null
            if (pill) {
                pushStringAnnotation(MARKDOWN_CODE_TAG, "")
                appendInlineContent(MARKDOWN_CODE_PAD_ID, MARKDOWN_CODE_PAD)
            }
            val url = span.link
            if (url != null) {
                val annotation = if (onLink != null) {
                    LinkAnnotation.Clickable(url, linkInteractionListener = LinkInteractionListener { onLink(url) })
                } else {
                    LinkAnnotation.Url(url)
                }
                withLink(annotation) { withStyle(style) { append(span.text) } }
            } else {
                withStyle(style) { append(span.text) }
            }
            if (pill) {
                appendInlineContent(MARKDOWN_CODE_PAD_ID, MARKDOWN_CODE_PAD)
                pop()
            }
        }
    }

    /** Round 9: the ascent and descent of [code]'s font at its size on [spec] (dp): the pill's height, as the web's inline box. */
    fun codeMetrics(code: MarkdownCodeStyle, spec: MarkdownTextSpec): Pair<Float, Float> {
        val r = measurer.measure(
            "x",
            style = TextStyle(
                fontFamily = fonts.family(monoFamily, mono = true),
                fontSize = code.size(spec).sp,
                platformStyle = PlatformTextStyle(includeFontPadding = false),
            ),
        )
        return (r.firstBaseline / density) to ((r.size.height - r.firstBaseline) / density)
    }

    private fun layout(
        inlines: List<MarkdownInline>,
        spec: MarkdownTextSpec,
        width: Float?,
        placeholders: List<AnnotatedString.Range<Placeholder>> = emptyList(),
        code: MarkdownCodeStyle? = null,
    ): TextLayoutResult =
        measurer.measure(
            annotated(inlines, spec, code = code),
            style = style(spec),
            placeholders = placeholders,
            softWrap = width != null,
            constraints = if (width != null) {
                Constraints(maxWidth = (width * density).roundToInt().coerceAtLeast(1))
            } else {
                Constraints()
            },
        )

    override fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): Float {
        if (inlines.all { it.text.isEmpty() }) return spec.lineHeight
        return maxOf(1, layout(inlines, spec, width).lineCount) * spec.lineHeight
    }

    override fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float {
        val r = layout(inlines, spec, null)
        var widest = 0f
        for (line in 0 until r.lineCount) widest = maxOf(widest, r.getLineRight(line) - r.getLineLeft(line))
        return ceil(widest / density)
    }

    override fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float {
        val words = Markdown.plain(inlines).split(Regex("\\s+")).filter { it.isNotEmpty() }
        return words.maxOfOrNull { width(listOf(MarkdownInline(text = it)), spec) } ?: 0f
    }

    /** #VAPP-258: a chip label's (or ident's) width shaped in [spec], the painted font. */
    fun labelWidth(label: String, spec: MarkdownTextSpec, chips: MarkdownChipStyle): Float {
        if (label.isEmpty()) return chips.labelWidth(0f, density)
        val r = layout(listOf(MarkdownInline(text = label)), spec, null)
        return chips.labelWidth(r.getLineRight(0) - r.getLineLeft(0), density)
    }

    /** #VAPP-258: the chip placeholders of [inlines] (each capped at [cap]). */
    fun placeholders(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, chips: MarkdownChipStyle, cap: Float?): List<AnnotatedString.Range<Placeholder>> =
        markdownChipPlaceholders(inlines, spec, chips, cap) { t, sp -> labelWidth(t, sp, chips) }

    override fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?, chips: MarkdownChipStyle): Float {
        if (inlines.none { it.chip != null || it.code }) return height(inlines, spec, width)
        return maxOf(1, layout(inlines, spec, width, placeholders(inlines, spec, chips, width), chips.code).lineCount) * spec.lineHeight
    }

    override fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, chips: MarkdownChipStyle): Float {
        if (inlines.none { it.chip != null || it.code }) return width(inlines, spec)
        val r = layout(inlines, spec, null, placeholders(inlines, spec, chips, null), chips.code)
        var widest = 0f
        for (line in 0 until r.lineCount) widest = maxOf(widest, r.getLineRight(line) - r.getLineLeft(line))
        return ceil(widest / density)
    }

    override fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, chips: MarkdownChipStyle): Float {
        if (inlines.none { it.chip != null }) return widestWord(inlines, spec)
        val text = inlines.map { if (it.chip != null) it.copy(text = " ", chip = null) else it }
        val widestChip = inlines.mapNotNull { it.chip }.maxOf { markdownChipWidth(it, chips, null) { t, sp -> labelWidth(t, sp, chips) } }
        return maxOf(widestWord(text, spec), widestChip)
    }
}

/** #VAPP-258: the inline-content id of the [n]th chip of a run list. */
fun markdownChipId(n: Int): String = "eui-chip-$n"

/** Round 9: the word joiner a code span's side padding is a placeholder over: it keeps the padding on the span's line and adds nothing to a copy. */
const val MARKDOWN_CODE_PAD = "\u2060"

/** Round 9: the inline-content id of [MARKDOWN_CODE_PAD]. */
const val MARKDOWN_CODE_PAD_ID = "eui-code-pad"

/** Round 9: the annotation over a code span, its pads included. */
const val MARKDOWN_CODE_TAG = "eui-code"

/**
 * #VAPP-258: the placeholders of [inlines]' chip runs in the text built by
 * appending every non-empty run (a chip run as its label): each chip
 * [MarkdownChipStyle.width] wide (capped at [cap]) and
 * [MarkdownChipStyle.height] tall, centred in the line, and a code run's
 * side padding. What the painter's inline content and every measurer agree on.
 */
fun markdownChipPlaceholders(
    inlines: List<MarkdownInline>,
    spec: MarkdownTextSpec,
    chips: MarkdownChipStyle,
    cap: Float?,
    labelWidth: (String, MarkdownTextSpec) -> Float,
): List<AnnotatedString.Range<Placeholder>> {
    val out = ArrayList<AnnotatedString.Range<Placeholder>>()
    var at = 0
    for (span in inlines) {
        if (span.text.isEmpty()) continue
        val chip = span.chip
        if (chip == null) {
            if (span.code) {
                val pad = Placeholder(chips.code.padH(spec).sp, 1.sp, PlaceholderVerticalAlign.Center)
                out.add(AnnotatedString.Range(pad, at, at + MARKDOWN_CODE_PAD.length))
                at += MARKDOWN_CODE_PAD.length + span.text.length
                out.add(AnnotatedString.Range(pad, at, at + MARKDOWN_CODE_PAD.length))
                at += MARKDOWN_CODE_PAD.length
            } else {
                at += span.text.length
            }
            continue
        }
        val w = markdownChipWidth(chip, chips, cap, labelWidth)
        out.add(AnnotatedString.Range(Placeholder(w.sp, chips.height(spec.lineHeight).sp, PlaceholderVerticalAlign.Center), at, at + chip.label.length))
        at += chip.label.length
    }
    return out
}

/** #VAPP-258: a chip's box width (capped at [cap]): its kind's style, the label and the ident each shaped in its own type. */
fun markdownChipWidth(chip: MarkdownChip, chips: MarkdownChipStyle, cap: Float?, labelWidth: (String, MarkdownTextSpec) -> Float): Float {
    val k = chips.resolved(chip.kind)
    val w = k.width(chip, labelWidth(chip.label, k.label), chip.shownIdent?.let { labelWidth(it, k.identSpec) } ?: 0f)
    return if (cap != null) minOf(w, maxOf(cap, 1f)) else w
}

/**
 * #VAPP-258 (markdown-host.json `imageBoxes` `loaded`): the media cache of
 * the sizes block images DECODED at, by src. An image painter reports its
 * picture here once it loads ([remember]); a [MarkdownView] (and a
 * measurer, [MarkdownHooks.loadedSize]) gives an image the host could not
 * size that aspect: one relayout from the placeholder box, then measure =
 * paint. Snapshot state: a view reading it recomposes when a size lands.
 */
object MarkdownImageSizes {
    private val sizes = mutableStateMapOf<String, MarkdownSize>()

    /** The size [src] decoded at, if it loaded before. */
    operator fun get(src: String): MarkdownSize? = sizes[src]

    /** Remember the size [src] decoded at (invalid = ignored; bounded). `true` = new (a re-measure is due). */
    fun remember(src: String, width: Float, height: Float): Boolean {
        val size = MarkdownSize(width, height)
        if (!size.valid || sizes[src] == size) return false
        if (sizes.size >= 512 && src !in sizes) sizes.clear()
        sizes[src] = size
        return true
    }
}

/**
 * A read-only markdown document: [text] parsed by [Markdown.parse], laid
 * out by [Markdown.layout] at the available width, each block painted at
 * its box (paragraphs, headings, bullet / numbered / task lists, quotes
 * with a bar, fenced code on a rounded background, rules, table grids;
 * inline bold, italic, code, strike, links; block images). Sizes are
 * taken at `fontScale = 1` so the measured layout is the painted one.
 * [linkPolicy] polices every href (null result = plain text, never
 * navigates); links open through [onLink] (null = the platform URI
 * handler). [imagePolicy] polices every block image's src (refused = a
 * paragraph of its alt text, no box); [image] paints an allowed one inside
 * its box (null = the alt text) and falls back to the alt text itself.
 * [codeHighlighter] colours a fenced block's text by its fence's language
 * (#VAPP-258; null = plain). #VAPP-258 host hooks (catalog/host.json
 * `markdown`): [linkSegments] = [textLinks] with chips (painted inline as
 * the Chip look through [chipIcon]); [imageSize] boxes a block image at
 * its aspect; [onImagePress] makes block images pressable; [attachments]
 * classifies a one-link paragraph (raw href), painted by [attachmentView]
 * (null = a link row). #VAPP-258 step 5 ([options], `markdown.options`):
 * hard breaks and a streamed prefix parse as the measurer parses them;
 * `selectable` true = a SelectionContainer, false = selection disabled; a
 * document of [MarkdownOptions.LAZY_BLOCKS]+ blocks composes only the
 * blocks that cross the visible window (the clipped bounds, one viewport of
 * overscan), each still at its laid-out box (the height is the layout's).
 */
@Composable
fun MarkdownView(
    text: String,
    modifier: Modifier = Modifier,
    styles: MarkdownStyles = MarkdownStyles(MarkdownTextSpec(size = 14f, weight = 400, lineHeight = 20f, family = null)),
    colors: MarkdownColors = MarkdownColors.from(LocalPrimitiveTokens.current),
    codeBlockRadius: Dp = LocalPrimitiveTokens.current.radiusMd,
    monoFamily: String? = LocalPrimitiveTokens.current.monoFamily,
    fonts: MarkdownFontResolver = MarkdownFontResolver { _, mono -> if (mono) FontFamily.Monospace else null },
    onLink: ((String) -> Unit)? = null,
    linkPolicy: ((String) -> String?)? = null,
    imagePolicy: ((String) -> Boolean)? = null,
    textLinks: ((String) -> List<Pair<String, String?>>?)? = null,
    image: (@Composable (src: String, alt: String) -> Unit)? = null,
    codeHighlighter: MarkdownCodeHighlighter? = null,
    linkSegments: ((String) -> List<MarkdownTextLink>?)? = null,
    imageSize: ((String) -> MarkdownSize?)? = null,
    attachments: ((String) -> MarkdownAttachment?)? = null,
    onImagePress: ((String) -> Unit)? = null,
    chipIcon: (@Composable (name: String, size: Float, color: Color) -> Unit)? = null,
    attachmentView: (@Composable (attachment: MarkdownBlockKind.Attachment, width: Float, height: Float) -> Unit)? = null,
    options: MarkdownOptions = MarkdownOptions(),
    /** #VAPP-258: the viewport height an uncapped image (`imageMaxHeight: null`) stops at; null = the window's. */
    viewportHeight: Float? = null,
    /** #VAPP-258: the laid-out document each time it lays out (a host folding it at whole lines, [MarkdownLaidOut.foldHeight]). */
    onLayout: ((MarkdownLaidOut) -> Unit)? = null,
) {
    val prepared = remember(text, linkPolicy, imagePolicy, attachments, imageSize, options.hardBreaks, options.streaming) {
        Markdown.prepare(text, MarkdownHooks(linkPolicy = linkPolicy, imagePolicy = imagePolicy, attachment = attachments, imageSize = imageSize, options = options))
    }
    // #VAPP-258: an image the host could not size takes its decoded size
    // once it loaded (the media cache; this read recomposes when one lands).
    val parsed = Markdown.sizeLoaded(prepared) { MarkdownImageSizes[it] }
    // A host's text links read live, like a `hostLinks` Text's.
    val blocks = when {
        linkSegments != null -> Markdown.linkSegments(parsed, linkSegments)
        textLinks != null -> Markdown.linkText(parsed, textLinks)
        else -> parsed
    }
    val hooks = MarkdownPaintHooks(onLink, image, codeHighlighter, onImagePress, chipIcon, attachmentView)
    val outer = LocalDensity.current
    val pinned = remember(outer.density) { Density(outer.density, 1f) }
    CompositionLocalProvider(LocalDensity provides pinned) {
        val measurer = rememberTextMeasurer()
        val shaper = remember(measurer, pinned.density, fonts, monoFamily) {
            MarkdownComposeText(measurer, pinned.density, fonts, monoFamily)
        }
        val window = androidx.compose.ui.platform.LocalConfiguration.current.screenHeightDp.toFloat()
        val viewport = viewportHeight ?: window.takeIf { it > 0f }
        // #VAPP-258: the host's image cap (options) and the viewport an uncapped image stops at.
        val styles = remember(styles, options.imageCap, viewport) { styles.copy(imageHeight = options.imageCap, viewportHeight = viewport) }
        BoxWithConstraints(modifier.fillMaxWidth()) {
            val width = if (constraints.hasBoundedWidth) constraints.maxWidth / pinned.density else 10_000f
            val layout = remember(blocks, styles, width, shaper) { Markdown.layout(blocks, styles, width, shaper) }
            if (onLayout != null) {
                val laid = remember(layout, blocks, styles) { MarkdownLaidOut(blocks, layout, styles) }
                androidx.compose.runtime.SideEffect { onLayout(laid) }
            }
            val lazy = blocks.size >= MarkdownOptions.LAZY_BLOCKS
            // The visible window in dp (clipped by every parent); before the
            // first placement, the first screenfuls.
            var window by remember(lazy) { mutableStateOf(0f..LAZY_FIRST_WINDOW) }
            val content: @Composable () -> Unit = {
                Box(
                    Modifier
                        .width(width.dp)
                        .height(layout.height.dp)
                        .then(
                            if (!lazy) Modifier else Modifier.onGloballyPositioned { c ->
                                val clipped = c.boundsInWindow()
                                val top = c.positionInWindow().y
                                val d = pinned.density
                                val next = if (clipped.height <= 0f) 0f..0f else ((clipped.top - top) / d)..((clipped.bottom - top) / d)
                                if (next != window) window = next
                            },
                        ),
                ) {
                    val span = window.endInclusive - window.start
                    val from = window.start - span
                    val to = window.endInclusive + span
                    blocks.forEachIndexed { i, block ->
                        val box = layout.blocks[i]
                        if (!lazy || (box.y + box.height >= from && box.y <= to)) {
                            Box(
                                Modifier
                                    .offset(y = box.y.dp)
                                    .width(width.dp)
                                    .height(box.height.dp)
                                    .then(if (lazy) Modifier.testTag(MARKDOWN_BLOCK_TAG) else Modifier),
                            ) {
                                MarkdownBlockView(block, box, width, styles, colors, codeBlockRadius, shaper, hooks)
                            }
                        }
                    }
                }
            }
            when (options.selectable) {
                true -> SelectionContainer { content() }
                false -> DisableSelection { content() }
                null -> content()
            }
        }
    }
}

/** #VAPP-258: a [MarkdownView]'s laid-out document (dp). */
class MarkdownLaidOut(val blocks: List<MarkdownBlock>, val layout: MarkdownLayout, val styles: MarkdownStyles) {
    /** The tallest height at most [limit] that ends on a whole line ([Markdown.foldHeight]). */
    fun foldHeight(limit: Float): Float = Markdown.foldHeight(blocks, layout, styles, limit)
}

/** #VAPP-258 step 5: a lazy document's block boxes carry this test tag. */
const val MARKDOWN_BLOCK_TAG = "xui-markdown-block"

/** What a lazy document composes before it knows its visible window (dp). */
private const val LAZY_FIRST_WINDOW = 1600f

/** The painter-only hooks a [MarkdownView] passes down to its blocks. */
private class MarkdownPaintHooks(
    val onLink: ((String) -> Unit)?,
    val image: (@Composable (src: String, alt: String) -> Unit)?,
    val codeHighlighter: MarkdownCodeHighlighter?,
    val onImagePress: ((String) -> Unit)?,
    val chipIcon: (@Composable (name: String, size: Float, color: Color) -> Unit)?,
    val attachmentView: (@Composable (attachment: MarkdownBlockKind.Attachment, width: Float, height: Float) -> Unit)?,
)

@Composable
private fun MarkdownBlockView(
    block: MarkdownBlock,
    box: MarkdownBlockBox,
    width: Float,
    styles: MarkdownStyles,
    colors: MarkdownColors,
    codeBlockRadius: Dp,
    shaper: MarkdownComposeText,
    hooks: MarkdownPaintHooks,
) {
    val spec = styles.spec(block.kind)
    val onLink = hooks.onLink
    val uri = LocalUriHandler.current
    val open: (String) -> Unit = { href -> if (onLink != null) onLink(href) else runCatching { uri.openUri(href) } }
    fun runs(inlines: List<MarkdownInline>, ink: Color, boldFloor: Int? = null) =
        shaper.annotated(inlines, spec, ink = ink, link = colors.link, boldFloor = boldFloor, onLink = onLink, code = styles.chip.code)
    // #VAPP-258: the chips of a run list wrapped at [cap] (the width its layout used).
    fun chips(inlines: List<MarkdownInline>, cap: Float): Map<String, InlineTextContent> =
        markdownChipContent(inlines, spec, styles.chip, cap, colors, shaper, hooks.chipIcon, open)
    when (val kind = block.kind) {
        is MarkdownBlockKind.Paragraph, is MarkdownBlockKind.Heading -> {
            val ink = (if (kind is MarkdownBlockKind.Heading) colors.headingInk else colors.paragraphInk).takeOrElse { colors.ink }
            MarkdownPills(
                runs(block.inlines, ink),
                style = shaper.style(spec, ink),
                modifier = Modifier.fillMaxWidth(),
                inlineContent = chips(block.inlines, maxOf(width, 1f)),
                spec = spec, styles = styles, colors = colors, shaper = shaper,
            )
        }
        is MarkdownBlockKind.Image -> {
            val ib = Markdown.imageBox(kind.width, kind.height, width, styles.imageHeight, styles.viewportHeight)
            val press = hooks.onImagePress
            Box(
                Modifier
                    .width(ib.width.dp)
                    .height(box.height.dp)
                    // #VAPP-258: the `Markdown/image` recipe's corners.
                    .then(if (styles.imageRadius > 0f) Modifier.clip(RoundedCornerShape(styles.imageRadius.dp)) else Modifier)
                    .then(
                        if (colors.imageBorderWidth > 0f && colors.imageBorder.isSpecified) {
                            Modifier.border(colors.imageBorderWidth.dp, colors.imageBorder, RoundedCornerShape(styles.imageRadius.dp))
                        } else Modifier,
                    )
                    .then(if (press != null) Modifier.clickable(role = Role.Image) { press(kind.src) } else Modifier),
            ) {
                val image = hooks.image
                if (image != null) {
                    image(kind.src, kind.alt)
                } else {
                    MarkdownImageAlt(kind.alt, shaper.style(spec, colors.muted))
                }
            }
        }
        is MarkdownBlockKind.Attachment -> {
            val w = styles.attachmentWidth(kind, width)
            Box(Modifier.width(w.dp).height(box.height.dp)) {
                val view = hooks.attachmentView
                if (view != null) {
                    view(kind, w, box.height)
                } else {
                    // No attachment painter: the link row (its name, opening the href).
                    BasicText(
                        kind.name,
                        style = shaper.style(spec, colors.link).copy(textDecoration = TextDecoration.Underline),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.align(Alignment.CenterStart).clickable(role = Role.Button) { open(kind.href) },
                    )
                }
            }
        }
        is MarkdownBlockKind.ListItem -> Row(verticalAlignment = Alignment.Top) {
            if (kind.depth > 0) Box(Modifier.width((styles.listIndent * kind.depth).dp))
            Box(Modifier.width(styles.listIndent.dp).height(spec.lineHeight.dp), contentAlignment = Alignment.CenterStart) {
                val task = kind.task
                if (task != null) {
                    TaskBox(task, minOf(TASK_BOX, spec.lineHeight).dp, colors)
                } else if (kind.marker == BULLET) {
                    BulletDisc(spec.size, colors.ink)
                } else {
                    // Web's outside `3.`: in the text colour, one space before the item's text.
                    BasicText(kind.marker, Modifier.align(Alignment.CenterEnd).padding(end = shaper.width(listOf(MarkdownInline(text = "\u00A0")), spec).dp), style = shaper.style(spec, colors.ink))
                }
            }
            MarkdownPills(
                runs(block.inlines, colors.ink),
                style = shaper.style(spec, colors.ink),
                modifier = Modifier.width((width - styles.listInset(kind.depth)).coerceAtLeast(1f).dp),
                inlineContent = chips(block.inlines, maxOf(width - styles.listInset(kind.depth), 1f)),
                spec = spec, styles = styles, colors = colors, shaper = shaper,
            )
        }
        is MarkdownBlockKind.Quote -> Row {
            Box(Modifier.width(styles.quoteBorder.dp).height(box.height.dp).background(colors.quoteBorder))
            Box(Modifier.width(styles.quotePad.dp))
            MarkdownPills(
                runs(block.inlines, colors.muted),
                style = shaper.style(spec, colors.muted),
                modifier = Modifier.width((width - styles.quoteBorder - styles.quotePad).coerceAtLeast(1f).dp),
                inlineContent = chips(block.inlines, maxOf(width - styles.quoteBorder - styles.quotePad, 1f)),
                spec = spec, styles = styles, colors = colors, shaper = shaper,
            )
        }
        is MarkdownBlockKind.CodeBlock -> Box(
            Modifier
                .width(width.dp)
                .height(box.height.dp)
                .background(colors.codeBlockBackground, RoundedCornerShape(codeBlockRadius))
                // #VAPP-258: the `Markdown/codeBlock` border, inside the box.
                .then(if (styles.codeBorder > 0f) Modifier.border(styles.codeBorder.dp, colors.codeBlockBorder.takeOrElse { colors.border }, RoundedCornerShape(codeBlockRadius)) else Modifier)
                .padding((styles.codePad + styles.codeBorder).dp),
        ) {
            BasicText(highlightedCode(shaper.annotated(block.inlines, spec, ink = colors.ink, link = colors.link, onLink = onLink), block.lang, hooks.codeHighlighter), style = shaper.style(spec, colors.ink), softWrap = false)
        }
        is MarkdownBlockKind.Rule -> Box(Modifier.fillMaxWidth().height(1.dp).background(colors.border))
        is MarkdownBlockKind.Table -> {
            val all = listOf(kind.header) + kind.rows
            val cols = Markdown.tableColumnCount(kind)
            // #FEED-224: each column its fitted width; the table hugs them
            // (start-aligned) and scrolls sideways when wider than the box.
            val textW = (0 until cols).map { maxOf(box.cols.getOrElse(it) { 1f }, 1f) }
            val tableW = Markdown.tableWidth(textW, styles)
            val scrollState = rememberScrollState()
            val scroll = if (tableW > width + 0.5f) Modifier.horizontalScroll(scrollState) else Modifier
            Box(Modifier.width(width.dp).then(scroll)) {
                Column(Modifier.width(tableW.dp)) {
                    Box(Modifier.fillMaxWidth().height(1.dp).background(colors.border))
                    all.forEachIndexed { r, row ->
                        val rowH = box.rows.getOrNull(r)?.let { it - 1f } ?: spec.lineHeight
                        // #VAPP-258: the header row's `Markdown/headerCell` fill.
                        Row(Modifier.height(rowH.dp).then(if (r == 0) Modifier.background(colors.headerCellBackground) else Modifier)) {
                            for (c in 0 until cols) {
                                val cell = row.getOrNull(c) ?: emptyList()
                                MarkdownPills(
                                    runs(cell, colors.ink, boldFloor = if (r == 0) 600 else null),
                                    style = shaper.style(spec, colors.ink),
                                    inlineContent = chips(cell, textW[c]),
                                    modifier = Modifier
                                        .width((textW[c] + 2 * styles.cellPadH).dp)
                                        .padding(horizontal = styles.cellPadH.dp, vertical = styles.cellPadV.dp),
                                    spec = spec, styles = styles, colors = colors, shaper = shaper,
                                )
                            }
                        }
                        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.border))
                    }
                }
            }
        }
    }
}

/**
 * #VAPP-258: the inline content of [inlines]' chips: each the Chip look
 * (fill, inside border, full radius, `padH`, the icon then the caption
 * label, one line) in a placeholder [markdownChipPlaceholders] sizes the
 * same way; a tap opens the run's href through [open].
 */
private fun markdownChipContent(
    inlines: List<MarkdownInline>,
    spec: MarkdownTextSpec,
    chips: MarkdownChipStyle,
    cap: Float,
    colors: MarkdownColors,
    shaper: MarkdownComposeText,
    icon: (@Composable (name: String, size: Float, color: Color) -> Unit)?,
    open: (String) -> Unit,
): Map<String, InlineTextContent> {
    if (inlines.none { it.chip != null || it.code }) return emptyMap()
    val out = HashMap<String, InlineTextContent>()
    // Round 9: nothing to compose, only the side padding the line reserves (the fill = [MarkdownPills]).
    if (inlines.any { it.code }) out[MARKDOWN_CODE_PAD_ID] = InlineTextContent(Placeholder(chips.code.padH(spec).sp, 1.sp, PlaceholderVerticalAlign.Center)) {}
    var n = 0
    for (span in inlines) {
        if (span.text.isEmpty()) continue
        val chip = span.chip ?: continue
        val href = span.link
        val w = markdownChipWidth(chip, chips, cap) { t, sp -> shaper.labelWidth(t, sp, chips) }
        val h = chips.height(spec.lineHeight)
        val k = chips.resolved(chip.kind)
        val look = colors.chipLooks[chip.kind]
        val ink = (look?.ink ?: Color.Unspecified).takeOrElse { colors.chipInk.takeOrElse { colors.ink } }
        out[markdownChipId(n++)] = InlineTextContent(Placeholder(w.sp, h.sp, PlaceholderVerticalAlign.Center)) {
            MarkdownChipView(chip, k, colors, look, shaper.style(k.label, ink), shaper.style(k.identSpec, (look?.identInk ?: Color.Unspecified).takeOrElse { ink }), icon) { if (href != null) open(href) }
        }
    }
    return out
}

@Composable
private fun MarkdownChipView(
    chip: MarkdownChip,
    chips: MarkdownChipStyle,
    colors: MarkdownColors,
    look: MarkdownChipLook?,
    label: TextStyle,
    identStyle: TextStyle,
    icon: (@Composable (name: String, size: Float, color: Color) -> Unit)?,
    onPress: () -> Unit,
) {
    val shape = chips.radius?.let { RoundedCornerShape(it.dp) } ?: RoundedCornerShape(percent = 50)
    val borderWidth = look?.borderWidth ?: colors.chipBorderWidth
    val borderColor = (look?.border ?: Color.Unspecified).takeOrElse { colors.chipBorder.takeOrElse { colors.border } }
    Row(
        Modifier
            .fillMaxSize()
            .background(look?.background ?: colors.chipBackground, shape)
            .then(if (borderWidth > 0f) Modifier.border(borderWidth.dp, borderColor, shape) else Modifier)
            .clickable(role = Role.Button, onClick = onPress)
            .padding(horizontal = chips.padH.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val name = chip.icon
        if (!name.isNullOrEmpty()) {
            Box(Modifier.size(chips.iconSize.dp), contentAlignment = Alignment.Center) {
                icon?.invoke(name, chips.iconSize, label.color)
            }
            Box(Modifier.width(chips.gap.dp))
        }
        // #VAPP-258: the mono ident shares the title's baseline (the pair
        // is centred in the chip as one line).
        Row {
            val ident = chip.shownIdent
            if (ident != null) {
                BasicText(ident, Modifier.alignByBaseline(), style = identStyle, maxLines = 1, softWrap = false)
                Box(Modifier.width(chips.gap.dp))
            }
            BasicText(chip.label, Modifier.alignByBaseline(), style = label, maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** A block image's alt text (failed, still loading or no image painter): one line, start-aligned. */
@Composable
fun MarkdownImageAlt(alt: String, style: TextStyle) {
    BasicText(alt, style = style, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
}

/** THE Checkbox box (control-geometry.json: 16 square, 1 hairline, 4 corner). */
private const val TASK_BOX = 16f

/**
 * A GFM task box drawn as THE checkbox: unchecked = [MarkdownColors.taskFill]
 * under the [MarkdownColors.taskBorder] hairline; checked = the
 * [MarkdownColors.taskChecked] square with the check (14/16 of the box).
 */
@Composable
private fun TaskBox(checked: Boolean, size: Dp, colors: MarkdownColors) {
    Canvas(Modifier.size(size)) {
        val s = this.size.minDimension
        val unit = s / TASK_BOX
        val corner = CornerRadius(4f * unit, 4f * unit)
        if (checked) {
            val fill = colors.taskChecked.takeOrElse { colors.link }
            drawRoundRect(fill, size = Size(s, s), cornerRadius = corner)
            // The `check` glyph (lucide, 24 grid: 20,6 → 9,17 → 4,12) at 14.
            val g = 14f * unit
            val o = (s - g) / 2f
            fun pt(x: Float, y: Float) = Offset(o + x / 24f * g, o + y / 24f * g)
            val path = Path().apply {
                pt(20f, 6f).let { moveTo(it.x, it.y) }
                pt(9f, 17f).let { lineTo(it.x, it.y) }
                pt(4f, 12f).let { lineTo(it.x, it.y) }
            }
            drawPath(
                path,
                colors.taskCheck,
                style = Stroke(width = 2f / 24f * g, cap = StrokeCap.Round, join = StrokeJoin.Round),
            )
        } else {
            drawRoundRect(colors.taskFill, size = Size(s, s), cornerRadius = corner)
            val hair = unit
            drawRoundRect(
                colors.taskBorder.takeOrElse { colors.border },
                topLeft = Offset(hair / 2f, hair / 2f),
                size = Size(s - hair, s - hair),
                cornerRadius = CornerRadius(corner.x - hair / 2f, corner.y - hair / 2f),
                style = Stroke(width = hair),
            )
        }
    }
}

/**
 * Round 9: a text block over its inline code's fills: each code span's
 * `Markdown/code` box from its side padding to its side padding, per line it
 * crosses, from the font's descent to its ascent around the baseline (+
 * `paddingVertical`), as the web's inline box. Under the text and its chips.
 */
@Composable
private fun MarkdownPills(
    text: AnnotatedString,
    style: TextStyle,
    modifier: Modifier,
    inlineContent: Map<String, InlineTextContent>,
    spec: MarkdownTextSpec,
    styles: MarkdownStyles,
    colors: MarkdownColors,
    shaper: MarkdownComposeText,
) {
    val fill = colors.codeBackground
    val ranges = remember(text) { text.getStringAnnotations(MARKDOWN_CODE_TAG, 0, text.length) }
    if (ranges.isEmpty() || fill == null) {
        BasicText(text, modifier, style = style, inlineContent = inlineContent)
        return
    }
    val code = styles.chip.code
    val metrics = remember(code, spec, shaper) { shaper.codeMetrics(code, spec) }
    val laid = remember { mutableStateOf<TextLayoutResult?>(null) }
    BasicText(
        text,
        modifier.drawBehind {
            val r = laid.value ?: return@drawBehind
            val radius = CornerRadius(code.radius.dp.toPx())
            val up = (metrics.first + code.padV).dp.toPx()
            val down = (metrics.second + code.padV).dp.toPx()
            for (range in ranges) {
                val last = range.end - 1
                for (line in r.getLineForOffset(range.start)..r.getLineForOffset(last)) {
                    val from = maxOf(range.start, r.getLineStart(line))
                    val to = minOf(last, r.getLineEnd(line, visibleEnd = true) - 1)
                    val a = r.getHorizontalPosition(from, usePrimaryDirection = true)
                    val b = r.getBoundingBox(to).right
                    val baseline = r.getLineBaseline(line)
                    drawRoundRect(fill, Offset(minOf(a, b), baseline - up), Size(kotlin.math.abs(b - a), up + down), radius)
                }
            }
        },
        style = style,
        onTextLayout = { laid.value = it },
        inlineContent = inlineContent,
    )
}

/** The unordered list marker [Markdown.parse] writes. */
private const val BULLET = "\u2022"

/**
 * A bullet list's marker as the web draws `list-style: disc` (an outside
 * marker in the text colour): a filled disc 0.36 em across, its centre
 * 0.46 em into the indent, on the first line's middle
 * (`fixtures/markdown-inline.json` `documentPaint`).
 */
@Composable
private fun BulletDisc(fontSize: Float, color: Color) {
    val d = fontSize * MARKDOWN_BULLET_EM
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier
                .offset(x = maxOf(0f, fontSize * MARKDOWN_BULLET_CENTRE_EM - d / 2f).dp)
                .size(d.dp)
                .align(Alignment.CenterStart)
                .background(color, CircleShape),
        )
    }
}

/** #VAPP-258: [runs] with [highlighter]'s colours over the block's text (colour only). */
internal fun highlightedCode(runs: AnnotatedString, lang: String?, highlighter: MarkdownCodeHighlighter?): AnnotatedString {
    val spans = highlighter?.spans(runs.text, lang).orEmpty().filter { it.start in 0 until it.end && it.end <= runs.length }
    if (spans.isEmpty()) return runs
    val b = AnnotatedString.Builder(runs)
    for (span in spans) b.addStyle(SpanStyle(color = span.color), span.start, span.end)
    return b.toAnnotatedString()
}
