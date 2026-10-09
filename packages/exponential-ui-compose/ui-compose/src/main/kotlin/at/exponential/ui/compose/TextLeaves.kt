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
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import at.exponential.ui.json.JsonValue
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
import at.exponential.ui.primitives.MarkdownView
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
            else -> TextLeaf(cx)
        }
        "CodeBlock" to "code" -> if (cx.props["tokens"]?.array != null) CodeLineLeaf(cx) else TextLeaf(cx)
        else -> TextLeaf(cx)
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
    InnerBox(cx) {
        BasicText(
            cx.shown(text),
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

/** A Select option (the core's popup): icon + label + the check slot (the `check` glyph while selected). */
@Composable
internal fun SelectItemLeaf(cx: LeafContext) {
    val selected = cx.node.selected || cx.props.flag("selected")
    val check = cx.props.str("check").ifEmpty { "ui-check" }
    InnerBox(cx) {
        Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(max(cx.style.gap, 8f).dp), verticalAlignment = Alignment.CenterVertically) {
            cx.props["icon"]?.string?.let { IconView(it, 16f, cx.ink, cx.model, rtl = cx.rtl) }
            LeafLine(cx, cx.props.shownText("text"), Modifier.weight(1f))
            Box(Modifier.size(16.dp)) { if (selected) IconOrGlyph(check, Glyph.Check, 16f, cx) }
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
    val root = cx.part("Badge", "root", cx.props)
    val label = cx.part("Badge", "label", cx.props)
    val ink = label.color ?: root.color ?: cx.ink
    val ts = cx.textStyle.copy(fontSize = max(cx.textStyle.fontSize - 2f, 10f))
    InnerBox(cx, Alignment.CenterStart) {
        Box(Modifier.leafPartBox(root.style, 999f).padding(horizontal = 8.dp)) { LeafLine(cx, cx.props.shownText("text"), color = ink, ts = ts) }
    }
}

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
                val p = cx.part("CodeBlock", "token", cx.ownerProps + ("kind" to JsonValue.Str(kind)))
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

/** A `Link`: the underlined label (else the href), centred; a tap presses it (the core opens the URL). */
@Composable
internal fun LinkLeaf(cx: LeafContext) {
    val label = cx.props.shownText("label").ifEmpty { cx.props.str("href") }
    val model = cx.model
    val index = cx.index
    InnerBox(cx, Alignment.Center) {
        BasicText(
            cx.shown(label),
            modifier = Modifier.textSlack(0f).pointerInput(index) { detectTapGestures { model.press(index) } },
            style = cx.composeTextStyle().copy(textDecoration = TextDecoration.Underline),
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

/**
 * `Markdown`: the host's renderer when it has one, else the primitives'
 * `MarkdownView` with the measurer's [MarkdownPainter.styles] and fonts,
 * so the painted document is the measured height. Links open through the
 * host.
 */
@Composable
internal fun MarkdownLeaf(cx: LeafContext) {
    val text = cx.props.shownText("text")
    val model = cx.model
    val r = cx.inner
    val custom = model.host.markdown(text, r.width)
    if (custom != null) {
        InnerBox(cx) { custom() }
        return
    }
    val theme = model.effectiveTheme
    val styles = remember(theme, model.mode, cx.textStyle, cx.props) { MarkdownPainter.styles(theme, model.mode, cx.textStyle, cx.props) }
    val muted = cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.6f)
    val border = cx.themeColor("border") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.2f)
    val colors = MarkdownColors(
        ink = cx.ink,
        muted = muted,
        link = cx.part("Markdown", "link", cx.props).color ?: cx.themeColor("primary") ?: cx.ink,
        border = border,
        codeBackground = cx.part("Markdown", "code", cx.props).style.background,
        codeBlockBackground = cx.part("Markdown", "codeBlock", cx.props).style.background ?: cx.themeColor("muted") ?: cx.ink.copy(alpha = 0.06f),
        quoteBorder = cx.part("Markdown", "quote", cx.props).style.borderColor ?: border,
    )
    val shaper = model.textShaper()
    val fonts = remember(shaper) { MarkdownFontResolver { name, mono -> if (mono) shaper.family(name ?: "ui-monospace") else shaper.family(name) } }
    InnerBox(cx) {
        MarkdownView(
            text = text,
            modifier = Modifier.fillMaxSize().clipToBounds(),
            styles = styles,
            colors = colors,
            codeBlockRadius = (theme?.radius("md") ?: 6f).dp,
            monoFamily = theme?.monoFamily,
            fonts = fonts,
            onLink = { model.host.openUrl(it) },
        )
    }
}
