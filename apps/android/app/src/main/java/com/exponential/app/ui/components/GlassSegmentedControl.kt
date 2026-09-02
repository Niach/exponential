package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Full-width glass-pill segmented control — a 1:1 port of the iOS My Work
 * Inbox/My Issues tab language (EXP-192): one glass capsule container holding
 * equal-width segments, the active one filled with the shared
 * [GlassSegmentedControlDefaults.ActiveFill] (`glass.fillActive`). Optional
 * per-segment count [badge] (white primary capsule, the Inbox unread count).
 * EXP-615 adds an optional per-segment [leadingIcon] (the agent strip's brand
 * marks); it draws in the segment's own content color via [LocalContentColor],
 * so callers pass a tint-less `Icon`.
 */
@Composable
fun <T> GlassSegmentedControl(
    options: List<T>,
    selected: T,
    label: (T) -> String,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    badge: (T) -> Int = { 0 },
    leadingIcon: (@Composable (T) -> Unit)? = null,
    // EXP-642: optional per-segment testTag, so a capture suite can address ONE
    // segment (the Start-coding sheet's Issues/Actions/Chat tabs) instead of
    // guessing at a label that also matches other nodes. null = untagged.
    testTag: ((T) -> String?)? = null,
    // EXP-615: an optional smaller face, for a strip whose labels would
    // otherwise wrap at phone widths. Segment labels never wrap regardless
    // (one line, ellipsized).
    textStyle: TextStyle? = null,
    // EXP-694: the strip as the FIRST ROW of a grouped card instead of a
    // free-floating capsule — no own fill, no hairline, no container padding
    // (the group's row padding provides it). Segments are unchanged, so the
    // active pill still reads the same inside the card.
    embedded: Boolean = false,
) {
    val capsule = GlassSegmentedControlDefaults.Shape
    Row(
        modifier = modifier
            .fillMaxWidth()
            .then(
                if (embedded) {
                    Modifier
                } else {
                    Modifier
                        .height(GlassSegmentedControlDefaults.Height)
                        .clip(capsule)
                        .background(GlassSegmentedControlDefaults.ContainerFill, capsule)
                        .border(GlassTokens.Hairline, GlassSegmentedControlDefaults.Hairline, capsule)
                        .padding(GlassSegmentedControlDefaults.ContainerPadding)
                },
            ),
        horizontalArrangement = Arrangement.spacedBy(GlassSegmentedControlDefaults.SegmentSpacing),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        options.forEach { option ->
            val active = option == selected
            val tag = testTag?.invoke(option)
            Row(
                modifier = Modifier
                    .weight(1f)
                    .then(if (tag != null) Modifier.testTag(tag) else Modifier)
                    .clip(capsule)
                    .background(
                        if (active) GlassSegmentedControlDefaults.ActiveFill else Color.Transparent,
                        capsule,
                    )
                    .clickable { onSelect(option) }
                    // A STANDALONE strip has a fixed height, so its segments
                    // fill it and pad nothing: 36 - 2x3 container inset - 2x6
                    // segment padding left 18dp for a 20sp line and clipped
                    // every label by ~2dp. An EMBEDDED strip has no height of
                    // its own, so the padding is what gives it one.
                    .then(
                        if (embedded) {
                            Modifier.padding(
                                vertical = GlassSegmentedControlDefaults.SegmentVerticalPadding,
                            )
                        } else {
                            Modifier.fillMaxHeight()
                        },
                    ),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val contentColor = MaterialTheme.colorScheme.onSurface.copy(
                    alpha = if (active) 1f else TextEmphasis.Secondary,
                )
                if (leadingIcon != null) {
                    CompositionLocalProvider(LocalContentColor provides contentColor) {
                        leadingIcon(option)
                    }
                    Spacer(Modifier.width(6.dp))
                }
                Text(
                    label(option),
                    style = textStyle ?: MaterialTheme.typography.labelLarge,
                    // EXP-698: CONSTANT weight — only the alpha moves. A weight
                    // swap re-measures the label, so the strip used to twitch
                    // horizontally on every selection.
                    fontWeight = GlassSegmentedControlDefaults.LabelWeight,
                    color = contentColor,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val count = badge(option)
                if (count > 0) {
                    Spacer(Modifier.width(6.dp))
                    Text(
                        count.toString(),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.SemiBold,
                        color = DesignTokens.Palette.PrimaryForeground,
                        modifier = Modifier
                            .clip(capsule)
                            .background(BadgeFill, capsule)
                            .padding(horizontal = 6.dp, vertical = 2.dp),
                    )
                }
            }
        }
    }
}

/**
 * The count-badge fill: the solid near-white primary with dark text (EXP-594
 * — the indigo accent is retired), like its iOS counterpart.
 */
private val BadgeFill = DesignTokens.Palette.Primary

/**
 * The segmented strip's own numbers (EXP-698). Pinned by
 * `GlassSegmentedControlDefaultsTest`, mirroring iOS's
 * `GlassSegmentedControlTokenTests` — the strip is the one control the four
 * clients draw identically, so its chrome may not be re-typed at a call site.
 */
object GlassSegmentedControlDefaults {
    /**
     * The capsule container's fill — the SECTION rung (EXP-698). A segmented
     * strip is a container of choices, not a row; on the row fill it read as
     * one more list row that happened to have tabs in it.
     */
    val ContainerFill: Color = GlassTokens.SectionFill

    /** The capsule container's hairline — the section rung's own stroke. */
    val Hairline: Color = GlassTokens.StrokeSection

    /** The selected segment's fill. */
    val ActiveFill: Color = GlassTokens.RowFillActive

    /**
     * Inset between the container's edge and a segment. 3dp, not 4: at 4 the
     * active pill floated inside a visible gutter instead of sitting in a
     * track.
     */
    val ContainerPadding: Dp = 3.dp

    /** Segments TOUCH — the active fill is the only thing separating them. */
    val SegmentSpacing: Dp = 0.dp

    /** A standalone strip is the large control rung. */
    val Height: Dp = DesignTokens.Size.ControlLg

    /**
     * An EMBEDDED strip's segment padding, and ONLY that: a standalone strip
     * pins [Height] and its segments fill it, so padding them too would eat
     * into the line box and clip the label.
     */
    val SegmentVerticalPadding: Dp = 6.dp

    /**
     * A segment label never changes weight, only alpha (EXP-698) — a
     * SemiBold/Normal swap re-measures the text and shifted the strip.
     */
    val LabelWeight: FontWeight = FontWeight.Medium

    /** Container and segments are both full capsules. */
    val Shape: RoundedCornerShape = RoundedCornerShape(percent = 50)
}
