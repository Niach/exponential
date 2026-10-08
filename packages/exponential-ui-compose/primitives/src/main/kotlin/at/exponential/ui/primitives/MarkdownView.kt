package at.exponential.ui.primitives

import androidx.compose.foundation.Canvas
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
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
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.LineHeightStyle
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

/** The colours a [MarkdownView] paints with. */
@Immutable
data class MarkdownColors(
    /** Body text. */
    val ink: Color,
    /** List markers, quote text, unchecked task boxes. */
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
        )
    }
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
    )

    /** The spans of [inlines] on [spec]: bold ≥ 600, italic, strike, mono code, underlined links. */
    fun annotated(
        inlines: List<MarkdownInline>,
        spec: MarkdownTextSpec,
        ink: Color = Color.Unspecified,
        link: Color = Color.Unspecified,
        codeBackground: Color? = null,
        boldFloor: Int? = null,
        onLink: ((String) -> Unit)? = null,
    ): AnnotatedString = buildAnnotatedString {
        val mono = fonts.family(monoFamily, mono = true)
        for (span in inlines) {
            if (span.text.isEmpty()) continue
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
                fontFamily = if (span.code) mono else null,
                textDecoration = if (decorations.isEmpty()) null else TextDecoration.combine(decorations),
                background = if (span.code && codeBackground != null) codeBackground else Color.Unspecified,
            )
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
        }
    }

    private fun layout(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): TextLayoutResult =
        measurer.measure(
            annotated(inlines, spec),
            style = style(spec),
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
}

/**
 * A read-only markdown document: [text] parsed by [Markdown.parse], laid
 * out by [Markdown.layout] at the available width, each block painted at
 * its box (paragraphs, headings, bullet / numbered / task lists, quotes
 * with a bar, fenced code on a rounded background, rules, table grids;
 * inline bold, italic, code, strike, links). Sizes are taken at
 * `fontScale = 1` so the measured layout is the painted one. Links open
 * through [onLink] (null = the platform URI handler).
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
) {
    val blocks = remember(text) { Markdown.parse(text) }
    val outer = LocalDensity.current
    val pinned = remember(outer.density) { Density(outer.density, 1f) }
    CompositionLocalProvider(LocalDensity provides pinned) {
        val measurer = rememberTextMeasurer()
        val shaper = remember(measurer, pinned.density, fonts, monoFamily) {
            MarkdownComposeText(measurer, pinned.density, fonts, monoFamily)
        }
        BoxWithConstraints(modifier.fillMaxWidth()) {
            val width = if (constraints.hasBoundedWidth) constraints.maxWidth / pinned.density else 10_000f
            val layout = remember(blocks, styles, width, shaper) { Markdown.layout(blocks, styles, width, shaper) }
            Box(Modifier.width(width.dp).height(layout.height.dp)) {
                blocks.forEachIndexed { i, block ->
                    val box = layout.blocks[i]
                    Box(
                        Modifier
                            .offset(y = box.y.dp)
                            .width(width.dp)
                            .height(box.height.dp),
                    ) {
                        MarkdownBlockView(block, box, width, styles, colors, codeBlockRadius, shaper, onLink)
                    }
                }
            }
        }
    }
}

@Composable
private fun MarkdownBlockView(
    block: MarkdownBlock,
    box: MarkdownBlockBox,
    width: Float,
    styles: MarkdownStyles,
    colors: MarkdownColors,
    codeBlockRadius: Dp,
    shaper: MarkdownComposeText,
    onLink: ((String) -> Unit)?,
) {
    val spec = styles.spec(block.kind)
    fun runs(inlines: List<MarkdownInline>, ink: Color, code: Color?, boldFloor: Int? = null) =
        shaper.annotated(inlines, spec, ink = ink, link = colors.link, codeBackground = code, boldFloor = boldFloor, onLink = onLink)
    when (val kind = block.kind) {
        is MarkdownBlockKind.Paragraph, is MarkdownBlockKind.Heading -> BasicText(
            runs(block.inlines, colors.ink, colors.codeBackground),
            style = shaper.style(spec, colors.ink),
            modifier = Modifier.fillMaxWidth(),
        )
        is MarkdownBlockKind.ListItem -> Row(verticalAlignment = Alignment.Top) {
            Box(Modifier.width(styles.listIndent.dp).height(spec.lineHeight.dp), contentAlignment = Alignment.CenterStart) {
                val task = kind.task
                if (task != null) {
                    TaskBox(task, (spec.size * 0.9f).dp, if (task) colors.ink else colors.muted)
                } else {
                    BasicText(kind.marker, style = shaper.style(spec, colors.muted))
                }
            }
            BasicText(
                runs(block.inlines, colors.ink, colors.codeBackground),
                style = shaper.style(spec, colors.ink),
                modifier = Modifier.width((width - styles.listIndent).coerceAtLeast(1f).dp),
            )
        }
        is MarkdownBlockKind.Quote -> Row {
            Box(Modifier.width(styles.quoteBorder.dp).height(box.height.dp).background(colors.quoteBorder))
            Box(Modifier.width(styles.quotePad.dp))
            BasicText(
                runs(block.inlines, colors.muted, null),
                style = shaper.style(spec, colors.muted),
                modifier = Modifier.width((width - styles.quoteBorder - styles.quotePad).coerceAtLeast(1f).dp),
            )
        }
        is MarkdownBlockKind.CodeBlock -> Box(
            Modifier
                .width(width.dp)
                .height(box.height.dp)
                .background(colors.codeBlockBackground, RoundedCornerShape(codeBlockRadius))
                .padding(styles.codePad.dp),
        ) {
            BasicText(runs(block.inlines, colors.ink, null), style = shaper.style(spec, colors.ink), softWrap = false)
        }
        is MarkdownBlockKind.Rule -> Box(Modifier.fillMaxWidth().height(1.dp).background(colors.border))
        is MarkdownBlockKind.Table -> {
            val all = listOf(kind.header) + kind.rows
            val cols = maxOf(kind.header.size, kind.rows.maxOfOrNull { it.size } ?: 0, 1)
            val cellW = width / cols
            Column(Modifier.width(width.dp)) {
                Box(Modifier.fillMaxWidth().height(1.dp).background(colors.border))
                all.forEachIndexed { r, row ->
                    val rowH = box.rows.getOrNull(r)?.let { it - 1f } ?: spec.lineHeight
                    Row(Modifier.height(rowH.dp)) {
                        for (c in 0 until cols) {
                            val cell = row.getOrNull(c) ?: emptyList()
                            BasicText(
                                runs(cell, colors.ink, colors.codeBackground, boldFloor = if (r == 0) 600 else null),
                                style = shaper.style(spec, colors.ink),
                                modifier = Modifier
                                    .width(cellW.dp)
                                    .padding(horizontal = styles.cellPadH.dp, vertical = styles.cellPadV.dp),
                            )
                        }
                    }
                    Box(Modifier.fillMaxWidth().height(1.dp).background(colors.border))
                }
            }
        }
    }
}

/** A GFM task box: a rounded square outline, ticked when [checked]. */
@Composable
private fun TaskBox(checked: Boolean, size: Dp, color: Color) {
    Canvas(Modifier.size(size)) {
        val s = this.size.minDimension
        val stroke = s / 10f
        drawRoundRect(
            color,
            topLeft = Offset(stroke / 2f, stroke / 2f),
            size = Size(s - stroke, s - stroke),
            cornerRadius = CornerRadius(s / 5f, s / 5f),
            style = Stroke(width = stroke),
        )
        if (checked) {
            val path = Path().apply {
                moveTo(s * 0.27f, s * 0.52f)
                lineTo(s * 0.44f, s * 0.68f)
                lineTo(s * 0.74f, s * 0.34f)
            }
            drawPath(path, color, style = Stroke(width = stroke * 1.4f))
        }
    }
}
