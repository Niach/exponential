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
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.flag
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
internal fun LeafLine(cx: LeafContext, text: String, modifier: Modifier = Modifier, color: androidx.compose.ui.graphics.Color = cx.ink, ts: ResolvedTextStyle = cx.textStyle) {
    BasicText(
        text,
        modifier = modifier,
        style = cx.composeTextStyle(ts = ts, color = color),
        softWrap = false,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/**
 * `Text`: the string in the measurer's style inside the content box,
 * wrapping; `lines` clamps with an ellipsis; `textAlign` / `align` aligns.
 */
@Composable
internal fun TextLeaf(cx: LeafContext) {
    val align = leafTextAlign(cx.style.textAlign ?: cx.props["align"]?.string)
    val lines = cx.node.lines
    InnerBox(cx) {
        BasicText(
            cx.props.str("text"),
            modifier = Modifier.fillMaxSize(),
            style = cx.composeTextStyle(align = align),
            softWrap = true,
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
            horizontalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterHorizontally),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            cx.props["icon"]?.string?.let { IconView(it, 16f, cx.ink, cx.model) }
            LeafLine(cx, cx.props.str("text"), Modifier.weight(1f, fill = false))
            cx.props["count"]?.let { count ->
                LeafLine(
                    cx,
                    count.displayText,
                    Modifier.padding(horizontal = 6.dp),
                    color = cx.themeColor("mutedForeground") ?: cx.ink,
                    ts = ResolvedTextStyle(12f, 400, 16f, cx.textStyle.fontFamily),
                )
            }
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

/** An Accordion `trigger`: the title (+ ` · count`) and a chevron that turns over when open. */
@Composable
internal fun AccordionTriggerLeaf(cx: LeafContext) {
    var title = cx.props.str("text")
    cx.props["count"]?.let { title += " · ${it.displayText}" }
    val turn by animateFloatAsState(if (cx.node.open) 180f else 0f, tween(150), label = "chevron")
    InnerBox(cx) {
        Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(max(cx.style.gap, 8f).dp), verticalAlignment = Alignment.CenterVertically) {
            LeafLine(cx, title, Modifier.weight(1f))
            GlyphView(Glyph.ChevronDown, 16f, cx.ink, Modifier.rotate(turn))
        }
    }
}

/** A DropdownMenu `item`: icon + label, tinted `destructive` when the item is. */
@Composable
internal fun MenuItemLeaf(cx: LeafContext) {
    val ink = if (cx.props.flag("destructive")) cx.themeColor("destructive") ?: cx.ink else cx.ink
    InnerBox(cx) {
        Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(max(cx.style.gap, 8f).dp), verticalAlignment = Alignment.CenterVertically) {
            cx.props["icon"]?.string?.let { IconView(it, 16f, ink, cx.model) }
            LeafLine(cx, cx.props.str("text"), Modifier.weight(1f), color = ink)
        }
    }
}

/** A `Link`: the underlined label (else the href), centred; a tap presses it (the core opens the URL). */
@Composable
internal fun LinkLeaf(cx: LeafContext) {
    val label = cx.props.str("label").ifEmpty { cx.props.str("href") }
    val model = cx.model
    val index = cx.index
    InnerBox(cx, Alignment.Center) {
        BasicText(
            label,
            modifier = Modifier.pointerInput(index) { detectTapGestures { model.press(index) } },
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
            SurfaceMeasurer.unknownLabel(cx.props, cx.node.component),
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
    val text = cx.props.str("text")
    val model = cx.model
    val r = cx.inner
    val custom = model.host.markdown(text, r.width)
    if (custom != null) {
        InnerBox(cx) { custom() }
        return
    }
    val theme = model.theme
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
    val shaper = model.shaper()
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
