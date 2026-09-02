package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
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
 * equal-width segments, the active one filled white-0.12. Optional
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
                        .clip(capsule)
                        .background(GlassTokens.RowFill, capsule)
                        .border(GlassTokens.Hairline, GlassSegmentedControlDefaults.Hairline, capsule)
                        .padding(GlassSegmentedControlDefaults.ContainerPadding)
                },
            ),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
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
                    .padding(vertical = GlassSegmentedControlDefaults.SegmentVerticalPadding),
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
                    fontWeight = if (active) FontWeight.SemiBold else FontWeight.Normal,
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
    /** The capsule container's hairline — the heaviest non-active stroke. */
    val Hairline: Color = GlassTokens.StrokeStrong

    /** The selected segment's fill. */
    val ActiveFill: Color = GlassTokens.RowFillActive

    /** Inset between the container's edge and a segment. */
    val ContainerPadding: Dp = 4.dp

    /** A segment's own vertical padding — the strip's height comes from this. */
    val SegmentVerticalPadding: Dp = 7.dp

    /** Container and segments are both full capsules. */
    val Shape: RoundedCornerShape = RoundedCornerShape(percent = 50)
}
