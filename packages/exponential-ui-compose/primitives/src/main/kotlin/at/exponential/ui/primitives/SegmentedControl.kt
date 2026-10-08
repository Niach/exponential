package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
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
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** The look of a segmented control: the track and the selected segment. */
@Immutable
data class SegmentedStyle(
    /** The track's height (a minimum when [minimumHeight]). */
    val height: Dp = 32.dp,
    /** The track fill. */
    val trackFill: Color = Color.Gray.copy(alpha = 0.15f),
    /** The track hairline (null = none), drawn inside the track. */
    val trackStroke: Color? = null,
    /** The selected segment's fill. */
    val segmentFill: Color = Color.Gray.copy(alpha = 0.3f),
    /** The selected segment's hairline (null = none). */
    val segmentStroke: Color? = null,
    /** An unselected label. */
    val label: Color = Color.Gray,
    /** The selected label. */
    val selectedLabel: Color = Color.White,
    /** A disabled segment's label. */
    val disabledLabel: Color = Color.Gray.copy(alpha = 0.5f),
    /** The track corner radius (ignored when [capsule]). */
    val radius: Dp = 8.dp,
    /** Inset between the track's edge and a segment. */
    val inset: Dp = 2.dp,
    /** A segment's horizontal padding around its content. */
    val horizontalPadding: Dp = 12.dp,
    /** The label size when [textStyle] does not set one. */
    val fontSize: TextUnit = 13.sp,
    /** The label weight; wins over [textStyle] (labels never change weight). */
    val fontWeight: FontWeight = FontWeight.Medium,
    /** A base text style for the labels. */
    val textStyle: TextStyle? = null,
    /** Capsule track and segments instead of [radius] rounded rectangles. */
    val capsule: Boolean = false,
    /** [height] is a minimum instead of a fixed height. */
    val minimumHeight: Boolean = false,
    /** When set, a segment's height comes from this vertical padding instead of filling the track. */
    val segmentVerticalPadding: Dp? = null,
    /** Draw the track; `false` = the segments alone (a strip embedded in a chromed card). */
    val showsTrack: Boolean = true,
    /** The track hairline's width. */
    val trackStrokeWidth: Dp = 1.dp,
    /** Gap between segments. */
    val segmentSpacing: Dp = 0.dp,
) {
    /** A label's text style in [color]. */
    fun resolvedTextStyle(color: Color): TextStyle =
        TextStyle(fontSize = fontSize).merge(textStyle).merge(TextStyle(fontWeight = fontWeight, color = color))

    /** Factories over the ambient tokens. */
    companion object {
        /** The control on [tokens]: muted track, card segment, border hairlines. */
        fun default(tokens: PrimitiveTokens): SegmentedStyle = SegmentedStyle(
            height = tokens.rowHeight,
            trackFill = tokens.muted,
            trackStroke = tokens.border,
            segmentFill = tokens.card,
            segmentStroke = tokens.border,
            label = tokens.mutedForeground,
            selectedLabel = tokens.foreground,
            disabledLabel = tokens.mutedForeground.copy(alpha = 0.5f),
            radius = tokens.radiusMd,
            trackStrokeWidth = tokens.hairline,
        )
    }
}

/**
 * One segment of a [SegmentedControl]. [content] draws INSTEAD of the label
 * text (it receives the segment's label colour; [label] stays the spoken
 * name); [leading]/[trailing] sit beside the label and own their gaps.
 */
@Immutable
class Segment<T>(
    /** The value selecting this segment picks. */
    val value: T,
    /** The label text and the default accessible name. */
    val label: String,
    /** Not selectable. */
    val disabled: Boolean = false,
    /** Drawn before the label, in the label colour. */
    val leading: (@Composable (Color) -> Unit)? = null,
    /** Drawn instead of the label text. */
    val content: (@Composable (Color) -> Unit)? = null,
    /** Drawn after the label. */
    val trailing: (@Composable (Color) -> Unit)? = null,
    /** The spoken name when it says more than [label]. */
    val contentDescription: String? = null,
    /** A test tag for UI tests. */
    val testTag: String? = null,
)

/**
 * A segmented control: a track of equal ([fill]) or hugging segments, the
 * selected one raised. Single selection; each segment is a `Role.Tab`
 * selectable.
 */
@Composable
fun <T> SegmentedControl(
    segments: List<Segment<T>>,
    selection: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    style: SegmentedStyle = SegmentedStyle.default(LocalPrimitiveTokens.current),
    fill: Boolean = true,
) {
    val trackShape: Shape = if (style.capsule) RoundedCornerShape(percent = 50) else RoundedCornerShape(style.radius)
    val segmentShape: Shape =
        if (style.capsule) RoundedCornerShape(percent = 50) else RoundedCornerShape((style.radius - style.inset).coerceAtLeast(0.dp))
    val track = if (style.showsTrack) {
        Modifier
            .then(if (style.minimumHeight) Modifier.heightIn(min = style.height) else Modifier.height(style.height))
            .clip(trackShape)
            .background(style.trackFill, trackShape)
            .then(if (style.trackStroke != null) Modifier.border(style.trackStrokeWidth, style.trackStroke, trackShape) else Modifier)
            .padding(style.inset)
    } else {
        Modifier
    }
    Row(
        modifier = modifier
            .then(if (fill) Modifier.fillMaxWidth() else Modifier)
            .then(track)
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(style.segmentSpacing),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        segments.forEach { segment ->
            val selected = segment.value == selection
            val color = when {
                segment.disabled -> style.disabledLabel
                selected -> style.selectedLabel
                else -> style.label
            }
            val fixedHeight = style.showsTrack && style.segmentVerticalPadding == null && !style.minimumHeight
            Row(
                modifier = Modifier
                    .then(if (fill) Modifier.weight(1f) else Modifier)
                    .then(if (segment.testTag != null) Modifier.testTag(segment.testTag) else Modifier)
                    .clip(segmentShape)
                    .background(if (selected) style.segmentFill else Color.Transparent, segmentShape)
                    .then(
                        if (selected && style.segmentStroke != null) {
                            Modifier.border(1.dp, style.segmentStroke, segmentShape)
                        } else {
                            Modifier
                        },
                    )
                    .selectable(
                        selected = selected,
                        enabled = !segment.disabled,
                        role = Role.Tab,
                        onClick = { onSelect(segment.value) },
                    )
                    .then(
                        when {
                            style.segmentVerticalPadding != null -> Modifier.padding(vertical = style.segmentVerticalPadding)
                            fixedHeight -> Modifier.fillMaxHeight()
                            else -> Modifier
                        },
                    )
                    .padding(horizontal = style.horizontalPadding),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CompositionLocalProvider(LocalContentColor provides color) {
                    segment.leading?.invoke(color)
                    val custom = segment.content
                    if (custom != null) {
                        val name = segment.contentDescription ?: segment.label
                        Row(
                            modifier = Modifier.clearAndSetSemantics { contentDescription = name },
                            verticalAlignment = Alignment.CenterVertically,
                        ) { custom(color) }
                    } else {
                        val spoken = segment.contentDescription
                        BasicText(
                            segment.label,
                            style = style.resolvedTextStyle(color),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = if (spoken != null) {
                                Modifier.clearAndSetSemantics { contentDescription = spoken }
                            } else {
                                Modifier
                            },
                        )
                    }
                    segment.trailing?.invoke(color)
                }
            }
        }
    }
}
