package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The pill geometry and paint: one capsule height, horizontal padding, a
 * leading dot or glyph, a label. The painter sizes it from the `Pill`
 * recipe; the Exponential app's `GlassPill` maps its rungs onto it.
 */
@Immutable
data class PillStyle(
    /** The capsule's height. */
    val height: Dp = 24.dp,
    /** Inset between the capsule's edge and its content. */
    val horizontalPadding: Dp = 8.dp,
    /** Gap between dot, leading, label and trailing. */
    val spacing: Dp = 4.dp,
    /** The label size when [textStyle] does not set one. */
    val fontSize: TextUnit = 12.sp,
    /** The label weight when [textStyle] does not set one. */
    val fontWeight: FontWeight = FontWeight.Medium,
    /** The capsule fill. */
    val fill: Color = Color.Transparent,
    /** The hairline (null = none), drawn inside the capsule. */
    val stroke: Color? = null,
    /** The hairline's width. */
    val strokeWidth: Dp = 1.dp,
    /** The label and slot content colour (`Unspecified` = the tokens' foreground). */
    val label: Color = Color.Unspecified,
    /** The leading colour disc's diameter. */
    val dotSize: Dp = 6.dp,
    /** The glyph size a slot is expected to draw at. */
    val glyphSize: Dp = 12.dp,
    /** A text style that wins over [fontSize]/[fontWeight] where it sets them. */
    val textStyle: TextStyle? = null,
    /** A surface laid UNDER [fill] (an opaque card beneath a translucent fill). */
    val underlay: Color? = null,
) {
    /** The label's resolved text style in [color]. */
    fun resolvedTextStyle(color: Color): TextStyle =
        TextStyle(fontSize = fontSize, fontWeight = fontWeight)
            .merge(textStyle)
            .merge(TextStyle(color = color))

    /** Factories over the ambient tokens. */
    companion object {
        /** The neutral pill on [tokens]: card fill, border hairline, foreground label. */
        fun default(tokens: PrimitiveTokens): PillStyle = PillStyle(
            height = tokens.pillHeight,
            fill = tokens.card,
            stroke = tokens.border,
            strokeWidth = tokens.hairline,
            label = tokens.foreground,
        )
    }
}

/**
 * A capsule label with an optional leading [dot], [leading] and [trailing]
 * slots (drawn in the label colour via `LocalContentColor`). Not a button:
 * [modifier] sizes and places it; [innerModifier] lands inside the clipped
 * capsule before the padding, where a `clickable` keeps its ripple in shape.
 * An empty [label] draws no text (an icon-only capsule).
 */
@Composable
fun PillView(
    label: String,
    modifier: Modifier = Modifier,
    style: PillStyle = PillStyle.default(LocalPrimitiveTokens.current),
    dot: Color? = null,
    maxLines: Int = 1,
    horizontalPadding: Dp = style.horizontalPadding,
    innerModifier: Modifier = Modifier,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val shape = RoundedCornerShape(percent = 50)
    val ink = style.label.takeOrElse { LocalPrimitiveTokens.current.foreground }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(style.spacing, Alignment.CenterHorizontally),
        modifier = modifier
            .height(style.height)
            .clip(shape)
            .then(if (style.underlay != null) Modifier.background(style.underlay, shape) else Modifier)
            .background(style.fill, shape)
            .then(if (style.stroke != null) Modifier.border(style.strokeWidth, style.stroke, shape) else Modifier)
            .then(innerModifier)
            .padding(horizontal = horizontalPadding),
    ) {
        CompositionLocalProvider(LocalContentColor provides ink) {
            leading?.invoke()
            if (dot != null) {
                Box(Modifier.size(style.dotSize).background(dot, CircleShape))
            }
            if (label.isNotEmpty()) {
                BasicText(
                    label,
                    style = style.resolvedTextStyle(ink),
                    maxLines = maxLines,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            trailing?.invoke()
        }
    }
}
