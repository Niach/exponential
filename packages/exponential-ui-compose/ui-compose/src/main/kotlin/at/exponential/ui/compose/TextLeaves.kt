package at.exponential.ui.compose

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.LinkInteractionListener
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withLink
import at.exponential.ui.host.TextLinkSegment
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.graphics.Color
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.primitives.MarkdownCodeHighlighter
import at.exponential.ui.primitives.MarkdownCodeSpan
import at.exponential.ui.theme.PartStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.flag
import at.exponential.ui.json.shownText
import at.exponential.ui.json.str
import at.exponential.ui.measure.SurfaceMeasurer
import at.exponential.ui.model.press
import at.exponential.ui.paint.MarkdownPainter
import at.exponential.ui.primitives.MarkdownColors
import at.exponential.ui.primitives.MarkdownFontResolver
import at.exponential.ui.primitives.MarkdownImageAlt
import at.exponential.ui.primitives.MarkdownView
import androidx.compose.foundation.clickable
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.max

/** A one-line label in the leaf's text style, ellipsized (tabs, triggers, items, buttons). */
@Composable
internal fun LeafLine(cx: LeafContext, text: String, modifier: Modifier = Modifier, color: androidx.compose.ui.graphics.Color = cx.ink, ts: ResolvedTextStyle = cx.textStyle, align: TextAlign? = null) {
    BasicText(
        cx.shown(text),
        modifier = modifier.textSlack(textBias(align, cx.rtl)),
        style = cx.composeTextStyle(ts = ts, color = color, align = align),
        softWrap = false,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/**
 * Any `Text` leaf, dispatched by its OWNER (round 1 parts): a tab, an
 * accordion trigger, a menu item label, a Select option, a Table header
 * cell or typed cell, a CodeBlock line; else plain text.
 */
@Composable
internal fun TextPart(cx: LeafContext) {
    val n = cx.node
    when (n.ownerComponent to n.part) {
        "Tabs" to "tab" -> TabLeaf(cx)
        "Accordion" to "trigger" -> AccordionTriggerLeaf(cx)
        "Menu" to "itemLabel" -> MenuItemLeaf(cx)
        "Select" to "item" -> SelectItemLeaf(cx)
        "Table" to "headerCell" -> HeaderCellLeaf(cx)
        "Table" to "cell" -> when (cx.props.str("cellType")) {
            "boolean" -> BooleanCellLeaf(cx)
            "badge" -> if (cx.props.shownText("text").isEmpty()) TextLeaf(cx) else BadgeCellLeaf(cx)
            else -> TextCellLeaf(cx)
        }
        "CodeBlock" to "code" -> if (cx.props["tokens"]?.array != null) CodeLineLeaf(cx) else TextLeaf(cx)
        "DatePicker" to "day" -> if (cx.props.str("mark").isNotEmpty()) CalendarMarkedDayLeaf(cx) else TextLeaf(cx)
        else -> TextLeaf(cx)
    }
}

/**
 * #VAPP-282: a marked calendar day: the day's text + its event dot (4 x 4,
 * centred, 1 dp over the bottom; primaryForeground on the selected day;
 * fixtures/month-grid.json).
 */
@Composable
internal fun CalendarMarkedDayLeaf(cx: LeafContext) {
    val selected = "selected" in cx.node.states
    val dot = (if (selected) cx.themeColor("primaryForeground") else cx.tone(cx.props.str("mark"))) ?: cx.themeColor("primary") ?: cx.ink
    Box(Modifier.size(cx.size.width.dp, cx.size.height.dp)) {
        TextLeaf(cx)
        Box(
            Modifier.absoluteOffset(((cx.size.width - 4f) / 2f).dp, (cx.size.height - 1f - 4f).dp)
                .size(4.dp)
                .background(dot, RoundedCornerShape(50)),
        )
    }
}

/**
 * `Text`: the string in the measurer's style inside the content box,
 * wrapping; `lines` clamps with an ellipsis; the core's physical
 * `textAlign` (round 2: start / end against the node's direction), the
 * node's direction as the bidi paragraph direction, `textTransform`,
 * tracking, decoration and italics from the visual.
 */
@Composable
internal fun TextLeaf(cx: LeafContext) {
    val align = leafTextAlign(cx.style.textAlign ?: cx.props["align"]?.string, cx.rtl)
    val lines = cx.node.lines
    val text = cx.props.shownText("text")
    if (text.isEmpty()) return
    // #EXP-1674: a `hostLinks` Text (the Row subtitle) the host links part of.
    if (cx.props["hostLinks"]?.bool == true) {
        val segments = cx.model.textLinkSegments(text)
        if (segments != null) {
            LinkedTextLeaf(cx, segments, align, lines)
            return
        }
    }
    // EXP-1324 `shimmer`: the glyphs filled with the moving gradient.
    val shimmer = textShimmerBrush(cx)
    InnerBox(cx) {
        BasicText(
            cx.shown(text),
            modifier = Modifier.textSlack(textBias(align, cx.rtl)).fillMaxSize(),
            style = cx.composeTextStyle(align = align).let { if (shimmer != null) it.copy(brush = shimmer) else it },
            softWrap = lines != 1,
            maxLines = lines ?: Int.MAX_VALUE,
            overflow = if (lines != null) TextOverflow.Ellipsis else TextOverflow.Clip,
        )
    }
}

/**
 * #EXP-1674: the links in the text's own colour, underlined. A tap on a link
 * opens it through the URL policy and never presses the Row behind it (the link
 * consumes it; the rest of the text falls through to the Row).
 */
@Composable
private fun LinkedTextLeaf(cx: LeafContext, segments: List<TextLinkSegment>, align: androidx.compose.ui.text.style.TextAlign?, lines: Int?) {
    val model = cx.model
    val annotated = remember(segments, cx.props) {
        buildAnnotatedString {
            segments.forEach { segment ->
                val shown = cx.shown(segment.text)
                val href = segment.href
                if (href != null) {
                    val styles = TextLinkStyles(style = SpanStyle(textDecoration = TextDecoration.Underline))
                    withLink(LinkAnnotation.Clickable(href, styles = styles, linkInteractionListener = LinkInteractionListener { model.openHref(href) })) { append(shown) }
                } else {
                    append(shown)
                }
            }
        }
    }
    InnerBox(cx) {
        BasicText(
            annotated,
            modifier = Modifier.textSlack(textBias(align, cx.rtl)).fillMaxSize(),
            style = cx.composeTextStyle(align = align),
            softWrap = lines != 1,
            maxLines = lines ?: Int.MAX_VALUE,
            overflow = if (lines != null) TextOverflow.Ellipsis else TextOverflow.Clip,
        )
    }
}

/**
 * A Tabs `tab`: icon + label + count, centred in the content box; the
 * `Tabs/indicator` bar along the bottom edge while selected.
 */
@Composable
internal fun TabLeaf(cx: LeafContext) {
    val selected = cx.node.selected
    val ind = cx.part("Tabs", "indicator", if (selected) listOf("selected") else emptyList())
    val indH = ind.height ?: 0f
    LeafFrame {
        val r = cx.inner
        Row(
            Modifier.absoluteOffset(r.left.dp, r.top.dp).size(r.width.dp, r.height.dp),
            horizontalArrangement = Arrangement.spacedBy(cx.spacing("xs").dp, Alignment.CenterHorizontally),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            cx.props["icon"]?.string?.let { IconView(it, 16f, cx.ink, cx.model, rtl = cx.rtl) }
            LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f, fill = false))
            cx.props["count"]?.let { count -> LeafLine(cx, count.displayText, color = cx.themeColor("mutedForeground") ?: cx.ink) }
        }
        val bg = ind.style.background
        if (selected && indH > 0f && bg != null) {
            Box(
                Modifier
                    .align(Alignment.BottomStart)
                    .fillMaxWidth()
                    .height(indH.dp)
                    .background(bg, RoundedCornerShape(ind.style.radius.dp)),
            )
        }
    }
}

/**
 * An Accordion `trigger`: the title, the count as its own muted part after
 * it (round 2 §7) and a chevron that turns over when open.
 */
@Composable
internal fun AccordionTriggerLeaf(cx: LeafContext) {
    val turn by animateFloatAsState(if (cx.node.open) 180f else 0f, tween(if (cx.model.reducedMotion) 0 else 150), label = "chevron")
    val count = cx.props["count"]?.displayText?.takeIf { it.isNotEmpty() }
    val muted = cx.part("Accordion", "count").color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.6f)
    InnerBox(cx) {
        Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(max(cx.style.gap, 8f).dp), verticalAlignment = Alignment.CenterVertically) {
            Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f, fill = false))
                if (count != null) LeafLine(cx, count, color = muted)
            }
            GlyphView(Glyph.ChevronDown, 16f, cx.ink, Modifier.rotate(turn))
        }
    }
}

/** A Menu `itemLabel`: icon + label (the item's recipe colours it: destructive, disabled). */
@Composable
internal fun MenuItemLeaf(cx: LeafContext) {
    InnerBox(cx) {
        Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(max(cx.style.gap, 4f).dp), verticalAlignment = Alignment.CenterVertically) {
            cx.props["icon"]?.string?.let { IconView(it, 16f, cx.ink, cx.model, rtl = cx.rtl) }
            LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f))
        }
    }
}

/**
 * A Select option (the core's popup): icon + label + the check slot (the
 * `check` glyph while selected, the `check` part's size and colour,
 * #EXP-1629); `hover` = the popup's cursor ([at.exponential.ui.model.setSelectCursor]).
 */
@Composable
internal fun SelectItemLeaf(cx: LeafContext) {
    val selected = cx.node.selected || cx.props.flag("selected")
    val check = cx.props.str("check").ifEmpty { "ui-check" }
    val part = cx.part("Select", "check", cx.node.partStates)
    val size = part.width ?: 16f
    InnerBox(cx) {
        Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(max(cx.style.gap, 8f).dp), verticalAlignment = Alignment.CenterVertically) {
            cx.props["icon"]?.string?.let { IconView(it, 16f, cx.ink, cx.model, rtl = cx.rtl) }
            LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f))
            Box(Modifier.size(size.dp)) { if (selected) IconOrGlyph(check, Glyph.Check, size, cx, part.color ?: cx.ink) }
        }
    }
}

/** A Table `headerCell`: the label (the cell's `align`) + the sort arrow slot (`sortIcon`) when sortable. */
@Composable
internal fun HeaderCellLeaf(cx: LeafContext) {
    val icon = cx.props.str("sortIcon")
    val sortable = icon.isNotEmpty() || cx.props.flag("sortable")
    val align = leafTextAlign(cx.style.textAlign ?: cx.props["align"]?.string, cx.rtl)
    InnerBox(cx) {
        Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f), align = align)
            if (sortable) Box(Modifier.size(16.dp)) { if (icon.isNotEmpty()) IconOrGlyph(icon, if (icon.endsWith("up")) Glyph.ChevronUp else Glyph.ChevronDown, 16f, cx) }
        }
    }
}

/**
 * EXP-1548: a text Table cell: wraps (one line with an ellipsis in a
 * `truncate` column), centred in the row like gpui and the web's
 * `align-items: center`.
 */
@Composable
internal fun TextCellLeaf(cx: LeafContext) {
    val align = leafTextAlign(cx.style.textAlign ?: cx.props["align"]?.string, cx.rtl)
    val lines = cx.node.lines
    val text = cx.props.shownText("text")
    if (text.isEmpty()) return
    InnerBox(cx, Alignment.CenterStart) {
        BasicText(
            cx.shown(text),
            modifier = Modifier.textSlack(textBias(align, cx.rtl)).fillMaxWidth(),
            style = cx.composeTextStyle(align = align),
            softWrap = lines != 1,
            maxLines = lines ?: Int.MAX_VALUE,
            overflow = if (lines != null) TextOverflow.Ellipsis else TextOverflow.Clip,
        )
    }
}

/** A `boolean` Table cell: a tick for true, nothing for false. */
@Composable
internal fun BooleanCellLeaf(cx: LeafContext) {
    val on = cx.props["value"]?.let { it.bool ?: (it.displayText == "true") } ?: (cx.props.shownText("text") == "true")
    if (!on) return
    val align = leafTextAlign(cx.style.textAlign ?: cx.props["align"]?.string, cx.rtl)
    InnerBox(cx, if (align == TextAlign.Right) Alignment.CenterEnd else if (align == TextAlign.Center) Alignment.Center else Alignment.CenterStart) {
        GlyphView(Glyph.Check, 16f, cx.ink)
    }
}

/** A `badge` Table cell: the value in a Badge look (`Badge/root` + `Badge/label`), one line. */
@Composable
internal fun BadgeCellLeaf(cx: LeafContext) {
    // The secondary Badge, as the React and SwiftUI renderers draw a badge cell.
    val badge = cx.props + ("variant" to JsonValue.Str("secondary"))
    val root = cx.part("Badge", "root", badge)
    val label = cx.part("Badge", "label", badge)
    val ink = label.color ?: root.color ?: cx.ink
    val ts = cx.textStyle.copy(fontSize = max(cx.textStyle.fontSize - 2f, 10f))
    InnerBox(cx, Alignment.CenterStart) {
        Box(Modifier.leafPartBox(root.style, 999f).padding(horizontal = 8.dp)) { LeafLine(cx, cx.props.shownText("text"), color = ink, ts = ts) }
    }
}

/**
 * A CodeBlock token's look by its `kind`: the `CodeBlock/token {kind}`
 * recipe under [base]. CodeBlock lines paint its colour, weight and italic;
 * a Markdown code block (#VAPP-258) its colour only.
 */
internal fun SurfaceModel.codeTokenPart(kind: String, base: Props = emptyMap()): PartStyle =
    part("CodeBlock", "token", base + ("kind" to JsonValue.Str(kind)))

/**
 * #VAPP-258 (fixtures/markdown-render.json): a fenced block's coloured
 * ranges: the catalog's ONE tokenizer (`tokenizeCodeJson`, CodeBlock's
 * tokens) over [text] as [lang], each token but `plain` in [color] of its
 * kind. `plain` and `ansi` (whose escapes the tokenizer drops) never
 * highlight; an unknown language tokenizes as all `plain`; tokens that do
 * not concatenate back to the text colour nothing.
 */
fun markdownCodeSpans(text: String, lang: String?, color: (String) -> Color?): List<MarkdownCodeSpan> {
    if (lang.isNullOrEmpty() || lang == "plain" || lang == "ansi" || text.isEmpty()) return emptyList()
    val out = ArrayList<MarkdownCodeSpan>()
    val joined = StringBuilder()
    val cache = HashMap<String, Color?>()
    val lines = JsonValue.parse(at.exponential.ui.ffi.tokenizeCodeJson(text, lang)).array ?: return emptyList()
    lines.forEachIndexed { i, line ->
        if (i > 0) joined.append('\n')
        for (t in line.array ?: emptyList()) {
            val piece = t["text"]?.string ?: ""
            val kind = t["kind"]?.string ?: "plain"
            val start = joined.length
            joined.append(piece)
            if (piece.isEmpty() || kind == "plain") continue
            cache.getOrPut(kind) { color(kind) }?.let { out.add(MarkdownCodeSpan(start, joined.length, it)) }
        }
    }
    return if (joined.toString() == text) out else emptyList()
}

/** Markdown's fenced blocks in CodeBlock's colours under [model]'s theme. */
internal fun markdownCodeHighlighter(model: SurfaceModel): MarkdownCodeHighlighter =
    MarkdownCodeHighlighter { text, lang -> markdownCodeSpans(text, lang) { model.codeTokenPart(it).color } }

/** A CodeBlock line: the core tokenizer's tokens, each coloured / weighted by `CodeBlock/token {kind}`. */
@Composable
internal fun CodeLineLeaf(cx: LeafContext) {
    val tokens = cx.props["tokens"]?.array ?: emptyList()
    val model = cx.model
    val text = remember(tokens, model.effectiveTheme, model.mode) {
        val b = AnnotatedString.Builder()
        val cache = HashMap<String, SpanStyle>()
        for (t in tokens) {
            val s = t["text"]?.string ?: continue
            if (s.isEmpty()) continue
            val kind = t["kind"]?.string ?: "plain"
            val span = cache.getOrPut(kind) {
                val p = model.codeTokenPart(kind, cx.ownerProps)
                SpanStyle(
                    color = p.color ?: androidx.compose.ui.graphics.Color.Unspecified,
                    fontWeight = p.style.fontWeight?.let { FontWeight(it) },
                    fontStyle = if (p.style.fontStyle == "italic") FontStyle.Italic else null,
                )
            }
            val start = b.length
            b.append(s)
            b.addStyle(span, start, b.length)
        }
        b.toAnnotatedString()
    }
    InnerBox(cx) {
        // Visible overflow: an italic token's slant (a comment's last glyph)
        // overhangs its advance; BasicText's default Clip would cut it.
        BasicText(text, modifier = Modifier.textSlack(textBias(null, cx.rtl)).fillMaxSize(), style = cx.composeTextStyle(), softWrap = cx.node.lines != 1, maxLines = cx.node.lines ?: Int.MAX_VALUE, overflow = TextOverflow.Visible)
    }
}

/** A host icon by name when the host maps it, else a built-in [glyph]. */
@Composable
internal fun IconOrGlyph(name: String, glyph: Glyph, size: Float, cx: LeafContext, color: androidx.compose.ui.graphics.Color = cx.ink) {
    if (cx.model.host.icon(name, size) != null) IconView(name, size, color, cx.model, rtl = cx.rtl) else GlyphView(glyph, size, color)
}

/**
 * A `Link`: the underlined label (else the href), centred; a tap presses it
 * (the core opens the URL through the host's URL policy). An href the
 * policy denies paints as plain text (no underline) and never navigates.
 */
@Composable
internal fun LinkLeaf(cx: LeafContext) {
    val href = cx.props.str("href")
    val label = cx.props.shownText("label").ifEmpty { href }
    val model = cx.model
    val index = cx.index
    val denied = href.isNotEmpty() && remember(href, model.host) { model.href(href) == null }
    InnerBox(cx, Alignment.Center) {
        BasicText(
            cx.shown(label),
            modifier = Modifier.textSlack(0f).pointerInput(index) { detectTapGestures { model.press(index) } },
            style = if (denied) cx.composeTextStyle() else cx.composeTextStyle().copy(textDecoration = TextDecoration.Underline),
            softWrap = false,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The `Unknown` placeholder note in the `Unknown/label` recipe. */
@Composable
internal fun UnknownLeaf(cx: LeafContext) {
    val label = cx.part("Unknown", "label")
    val color = label.color ?: cx.themeColor("destructive") ?: cx.ink
    val ts = ResolvedTextStyle(label.px("fontSize") ?: 12f, 400, label.px("lineHeight") ?: 16f, label.fontFamily)
    InnerBox(cx) {
        BasicText(
            "${cx.string("unknownComponent")} ${cx.props.str("component").ifEmpty { cx.node.component }}",
            modifier = Modifier.fillMaxSize(),
            style = cx.composeTextStyle(ts = ts, color = color),
            softWrap = true,
        )
    }
}

private val TASK_OFF: Props = mapOf("checked" to JsonValue.Bool(false))
private val TASK_ON: Props = mapOf("checked" to JsonValue.Bool(true))

/**
 * `Markdown`: the host's renderer when it has one, else the primitives'
 * `MarkdownView` with the measurer's [MarkdownPainter.styles] and fonts,
 * so the painted document is the measured height. Every link href passes
 * the host's URL policy (denied = plain text) and opens through the host,
 * the text its `textLinks` links too (`fixtures/markdown-text-links.json`);
 * block images load through its media policy (denied = a paragraph of the
 * alt text, as the measurer lays it out; failed = the alt text in the box).
 */
@Composable
internal fun MarkdownLeaf(cx: LeafContext) {
    val text = cx.props.shownText("text")
    val model = cx.model
    val r = cx.inner
    // Round 9 §9.1: the inline profile is always the built-in painter.
    val inline = MarkdownPainter.isInline(cx.props)
    val custom = if (inline) null else model.host.markdown(text, r.width)
    if (custom != null) {
        InnerBox(cx) { custom() }
        return
    }
    val theme = model.effectiveTheme
    // #VAPP-258 step 5: the host's options (the chat rhythm, hard breaks, streaming, selection).
    val options = model.host.markdownOptions
    val props = remember(cx.props, options.chat) { MarkdownPainter.props(cx.props, options.chat) }
    val scale = model.textScale
    val styles = remember(theme, model.mode, cx.textStyle, props, scale) { MarkdownPainter.styles(theme, model.mode, cx.textStyle, props, scale) }
    val muted = cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.6f)
    val border = cx.themeColor("border") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.2f)
    val taskOff = cx.part("Checkbox", "box", TASK_OFF)
    val taskOn = cx.part("Checkbox", "box", TASK_ON, listOf("checked"))
    val chipRoot = cx.part("Chip", "root", MarkdownPainter.CHIP_PROPS)
    val colors = MarkdownColors(
        ink = cx.ink,
        muted = muted,
        link = cx.part("Markdown", "link", props).color ?: cx.themeColor("primary") ?: cx.ink,
        border = border,
        codeBackground = cx.part("Markdown", "code", props).style.background,
        codeBlockBackground = cx.part("Markdown", "codeBlock", props).style.background ?: cx.themeColor("muted") ?: cx.ink.copy(alpha = 0.06f),
        quoteBorder = cx.part("Markdown", "quote", props).style.borderColor ?: border,
        paragraphInk = if (inline) MarkdownPainter.inlineInk(theme, model.mode, MarkdownPainter.inlineRecipes("paragraph", MarkdownPainter.variant(props))) ?: Color.Unspecified else Color.Unspecified,
        headingInk = if (inline) MarkdownPainter.inlineInk(theme, model.mode, MarkdownPainter.inlineRecipes("heading")) ?: Color.Unspecified else Color.Unspecified,
        // A task box IS the theme's Checkbox box (off / on) and its check.
        taskFill = taskOff.style.background ?: Color.Transparent,
        taskBorder = taskOff.style.borderColor ?: border,
        taskChecked = taskOn.style.background ?: cx.themeColor("primary") ?: cx.ink,
        taskCheck = cx.part("Checkbox", "check", TASK_ON, listOf("checked")).color ?: cx.themeColor("primaryForeground") ?: Color.White,
        // #VAPP-258: an inline chip = the Chip root + label recipes (the catalog's default chip).
        chipBackground = chipRoot.style.background ?: Color.Transparent,
        chipBorder = chipRoot.style.borderColor ?: border,
        chipBorderWidth = chipRoot.style.borderWidth,
        chipInk = cx.part("Chip", "label", MarkdownPainter.CHIP_PROPS).color ?: cx.ink,
        // #VAPP-258: the `Markdown/image` recipe's border.
        imageBorder = cx.part("Markdown", "image", props).style.borderColor ?: Color.Unspecified,
        imageBorderWidth = cx.part("Markdown", "image", props).style.borderWidth,
        // #VAPP-258: the fenced block's border, the header row's fill, each chip kind's colours.
        codeBlockBorder = cx.part("Markdown", "codeBlock", props).style.borderColor ?: Color.Unspecified,
        headerCellBackground = cx.part("Markdown", "headerCell", props).style.background ?: Color.Transparent,
        chipLooks = theme?.let { MarkdownPainter.chipLooks(it, model.mode, props + ("density" to JsonValue.Str(if (options.chat) "chat" else "document"))) } ?: emptyMap(),
    )
    val shaper = model.textShaper()
    val fonts = remember(shaper) { MarkdownFontResolver { name, mono -> if (mono) shaper.family(name ?: "ui-monospace") else shaper.family(name) } }
    InnerBox(cx) {
        MarkdownView(
            text = text,
            modifier = Modifier.fillMaxSize().clipToBounds(),
            styles = styles,
            options = options,
            viewportHeight = model.viewportHeight.takeIf { it > 0f },
            colors = colors,
            codeBlockRadius = (theme?.radius("md") ?: 6f).dp,
            monoFamily = theme?.monoFamily,
            fonts = fonts,
            onLink = { model.openHref(it) },
            linkPolicy = remember(model, model.host) { { href: String -> model.href(href) } },
            imagePolicy = remember(model, model.host) { { src: String -> model.mediaRequest(src) != null } },
            // #VAPP-258: the host hooks, the measurer's (`SurfaceModel.markdownHooks`).
            linkSegments = { piece -> model.markdownTextLinks(piece) },
            attachments = remember(model, model.host) { { href: String -> model.markdownAttachment(href) } },
            imageSize = remember(model, model.host) { { src: String -> model.markdownImageSize(src) } },
            onImagePress = model.host.onImagePress,
            chipIcon = { name, size, color -> IconView(name, size, color, model, rtl = cx.rtl) },
            attachmentView = { a, w, h -> MarkdownAttachmentView(cx, styles, a.type, a.name, a.href, w, h) },
            codeHighlighter = remember(model, model.effectiveTheme, model.mode) { markdownCodeHighlighter(model) },
            image = { src, alt ->
                val picture = rememberPolicedImage(model, src)
                if (picture != null) {
                    // #VAPP-258 (markdown-host.json `loaded`): a picture the host
                    // could not size reports its decoded size; the leaf measures
                    // again at that aspect (no gap under it).
                    androidx.compose.runtime.LaunchedEffect(picture, src) {
                        at.exponential.ui.primitives.MarkdownImageSizes.remember(src, picture.width.toFloat(), picture.height.toFloat())
                        if (model.markdownImageSize(src) == null) model.remeasure(cx.index)
                    }
                    androidx.compose.foundation.Image(
                        picture,
                        alt,
                        Modifier.fillMaxSize(),
                        alignment = markdownImageAlignment(cx.rtl),
                        contentScale = androidx.compose.ui.layout.ContentScale.Fit,
                    )
                } else {
                    MarkdownImageAlt(alt, cx.composeTextStyle(color = muted))
                }
            },
        )
    }
}

/**
 * #VAPP-258: a Markdown attachment block [w] × [h]: `video` = the Video
 * native (no poster), `audio` = the AudioPlayer native titled `name`,
 * `file` = the FileUpload `file` row (fileIcon + fileName; a press opens
 * the href through the URL policy and the host's opener).
 */
@Composable
private fun MarkdownAttachmentView(
    cx: LeafContext,
    styles: at.exponential.ui.primitives.MarkdownStyles,
    // The attachment's fields, not `MarkdownBlockKind.Attachment`: naming the
    // primitives' sealed class here makes the release Dokka read its JVM 17
    // PermittedSubclasses (ASM9) and fail the publish.
    type: String,
    attachmentName: String,
    href: String,
    w: Float,
    h: Float,
) {
    val model = cx.model
    when (type) {
        "video" -> Box(Modifier.size(w.dp, h.dp)) {
            VideoView(
                model,
                src = href,
                poster = "",
                autoplay = false,
                durationMs = null,
                durationStyle = cx.composeTextStyle(ts = ResolvedTextStyle(12f, 400, 16f, cx.textStyle.fontFamily), color = Color.White),
            )
        }
        "audio" -> Box(Modifier.size(w.dp, h.dp)) {
            val ts = MarkdownPainter.audioTitle(model.effectiveTheme, model.mode, cx.textStyle, model.textScale)
            val color = cx.part("AudioPlayer", "track", emptyMap()).color ?: cx.ink
            AudioView(cx, src = href, title = attachmentName, durationMs = null, rowHeight = styles.audioRow) {
                BasicText(it, style = cx.composeTextStyle(ts = ts, color = color), maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis)
            }
        }
        else -> {
            val file = cx.part("FileUpload", "file", emptyMap())
            val name = cx.part("FileUpload", "fileName", emptyMap())
            val icon = cx.part("FileUpload", "fileIcon", emptyMap())
            val padH = file.px("paddingHorizontal") ?: file.px("padding") ?: cx.spacing("sm")
            val ts = cx.textStyle.copy(
                fontSize = name.px("fontSize") ?: cx.textStyle.fontSize,
                lineHeight = name.px("lineHeight") ?: cx.textStyle.lineHeight,
                fontFamily = name.fontFamily ?: cx.textStyle.fontFamily,
            )
            Row(
                Modifier
                    .size(w.dp, h.dp)
                    .leafPartBox(file.style)
                    .clickable(role = androidx.compose.ui.semantics.Role.Button) { model.openHref(href) }
                    .padding(horizontal = padH.dp),
                horizontalArrangement = Arrangement.spacedBy((file.px("gap") ?: cx.spacing("sm")).dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val iconSize = cx.control("iconSm", 16f)
                IconView("ui-file", iconSize, icon.color ?: cx.ink, model, rtl = cx.rtl)
                BasicText(attachmentName, style = cx.composeTextStyle(ts = ts, color = name.color ?: cx.ink), maxLines = 1, softWrap = false, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** A markdown block image inside its box: fitted, TOP and start aligned (Swift, React). */
internal fun markdownImageAlignment(rtl: Boolean): Alignment = if (rtl) Alignment.TopEnd else Alignment.TopStart
