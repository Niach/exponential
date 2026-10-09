package at.exponential.ui.compose

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.Props
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.PartStyle
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.max

/**
 * Everything a leaf painter reads: the node, its frame size (dp), its box
 * style (mirror-aware, `SurfaceModel.boxStyle`), its ink and the text style
 * the measurer shaped it with.
 */
class LeafContext(
    /** The surface model. */
    val model: SurfaceModel,
    /** The leaf node. */
    val node: NodeInfo,
    /** The core's frame size (dp). */
    val size: Size,
    /** The box style the node paints with. */
    val style: PaintStyle,
    /** The text colour. */
    val ink: Color,
    /** The resolved text style (the measurer's). */
    val textStyle: ResolvedTextStyle,
) {
    /** The node index. */
    val index: Int get() = node.index

    /** The node's props. */
    val props: Props get() = node.props

    /** The owner's props (a synthetic part's component). */
    val ownerProps: Props get() = model.ownerProps(node.index)

    /** The reported interaction states (`hover`, `pressed`, `focus`). */
    val states: List<String> get() = model.statesOf(node.id)

    /** Dark mode? */
    val dark: Boolean get() = model.mode == Mode.Dark

    /** The content box inside padding + border (what the measurer counted), leaf-local dp. */
    val inner: Rect
        get() {
            val (t, r, b, l) = style.insets
            return Rect(l, t, l + max(0f, size.width - l - r), t + max(0f, size.height - t - b))
        }

    /** Right-to-left (the node's own direction, else the surface's). */
    val rtl: Boolean get() = style.direction?.let { it == "rtl" } ?: (model.direction == "rtl")

    /** `text` in the leaf's `textTransform` (the core's text style, inherited like CSS). */
    fun shown(text: String): String = at.exponential.ui.measure.SurfaceMeasurer.applyTransform(text, textStyle.textTransform)

    /** A built-in string (`$string.<id>`, the host's overrides). */
    fun string(id: String, params: Map<String, Any?> = emptyMap()): String = model.string(id, params)

    /** A part of `component` resolved with the OWNER's props. */
    fun part(component: String, part: String, states: List<String> = emptyList()): PartStyle =
        model.part(component, part, ownerProps, states)

    /** A part of `component` resolved with explicit props. */
    fun part(component: String, part: String, props: Props, states: List<String> = emptyList()): PartStyle =
        model.part(component, part, props, states)

    /** A theme colour token. */
    fun themeColor(name: String): Color? = model.color(name)

    /** A spacing token (dp). */
    fun spacing(name: String): Float = model.spacing(name)

    /** A control size token (dp). */
    fun control(name: String, fallback: Float): Float = model.control(name, fallback)

    /** The semantic tone colour of a `tone` prop. */
    fun tone(name: String): Color? = when (name) {
        "primary" -> themeColor("primary")
        "success" -> themeColor("success")
        "warning" -> themeColor("warning")
        "danger", "destructive" -> themeColor("destructive")
        "info" -> themeColor("info")
        "neutral", "muted" -> themeColor("mutedForeground")
        else -> null
    }

    /**
     * The Compose style of [ts] (default: the leaf's, with its tracking and
     * italics) through the model's `TextShaper.style`, the one the measurer
     * shaped with, so the painted size is the measured one; coloured [color].
     */
    fun composeTextStyle(
        ts: ResolvedTextStyle = textStyle,
        color: Color = ink,
        italic: Boolean = ts.italic || textStyle.italic,
        family: String? = null,
        align: TextAlign? = null,
    ): TextStyle {
        val base = model.textShaper().style(ts, italic, family, align, rtl = rtl).copy(color = color)
        return when (style.textDecoration) {
            "underline" -> base.copy(textDecoration = TextDecoration.Underline)
            "line-through" -> base.copy(textDecoration = TextDecoration.LineThrough)
            else -> base
        }
    }
}

/**
 * A `textAlign` / `align` value → Compose. The core resolves a leaf's
 * PHYSICAL align (round 2: start / end against the node's direction);
 * a logical value still here resolves against [rtl] (the surface pins Ltr).
 */
internal fun leafTextAlign(value: String?, rtl: Boolean = false): TextAlign? = when (value) {
    "center" -> TextAlign.Center
    "right" -> TextAlign.Right
    "left" -> TextAlign.Left
    "end" -> if (rtl) TextAlign.Left else TextAlign.Right
    "start" -> if (rtl) TextAlign.Right else TextAlign.Left
    "justify" -> TextAlign.Justify
    else -> null
}

/** The leaf's frame: a full-size box, content placed by the caller. */
@Composable
internal fun LeafFrame(content: @Composable BoxScope.() -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopStart, content = content)
}

/** Places [content] at the leaf's [LeafContext.inner] box (padding + border inset). */
@Composable
internal fun InnerBox(cx: LeafContext, contentAlignment: Alignment = Alignment.TopStart, content: @Composable BoxScope.() -> Unit) {
    val r = cx.inner
    LeafFrame {
        Box(
            Modifier.absoluteOffset(r.left.dp, r.top.dp).size(r.width.dp, r.height.dp),
            contentAlignment = contentAlignment,
            content = content,
        )
    }
}

/** A recipe part's own box (background, border, radius, opacity): ToggleGroup items, chips. */
internal fun Modifier.leafPartBox(style: PaintStyle, fallbackRadius: Float = 0f): Modifier {
    val shape = RoundedCornerShape(max(style.radius, fallbackRadius).dp)
    var m = this
    style.opacity?.let { m = m.alpha(it) }
    style.background?.let { m = m.background(it, shape) }
    if (style.borderWidth > 0f) style.borderColor?.let { m = m.border(style.borderWidth.dp, it, shape) }
    return m
}
