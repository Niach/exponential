package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import at.exponential.ui.primitives.Segment
import at.exponential.ui.primitives.SegmentedControl
import at.exponential.ui.primitives.SegmentedStyle
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
    // EXP-1152: an optional per-segment CUSTOM label, drawn INSTEAD of
    // `Text(label(option))` when it returns a composable for that option (the
    // Work tabs' Changes segment wears the diff's tinted `+N −M`, the desktop
    // `FaceToggle::diff` rule). [label] stays the segment's accessible name,
    // and the slot receives the segment's content color so it can keep the
    // EXP-698 alpha rule; geometry and the constant weight are unchanged.
    labelContent: ((T) -> (@Composable (contentColor: Color) -> Unit)?)? = null,
    // EXP-1162: an optional per-segment TRAILING accessory after the label
    // (the Work tabs' state dot), and the segment's spoken name when it says
    // more than the label ("Run, running"). null = unchanged.
    trailing: ((T) -> (@Composable () -> Unit)?)? = null,
    description: ((T) -> String?)? = null,
    // EXP-1162: an optional per-segment LEADING accessory before the label
    // (the Work tabs' Run mark), drawn as given: it owns its own gap. Unlike
    // [leadingIcon] it may be absent per segment. null = unchanged.
    leading: ((T) -> (@Composable () -> Unit)?)? = null,
) {
    // SLOP-18 / VAPP-89: the strip is the SDK's `SegmentedControl`; this
    // composable maps the glass chrome and the per-segment slots onto it.
    // EXP-698: segments are SELECTABLE `Role.Tab`s (TalkBack reads "2 of 3,
    // selected"), and a label never changes weight, only alpha.
    val capsule = GlassSegmentedControlDefaults.Shape
    val segments = options.map { option ->
        val icon = leadingIcon
        val lead = leading?.invoke(option)
        val custom = labelContent?.invoke(option)
        val trail = trailing?.invoke(option)
        val count = badge(option)
        Segment(
            value = option,
            label = label(option),
            leading = if (icon != null || lead != null) {
                { _: Color ->
                    if (icon != null) {
                        icon(option)
                        Spacer(Modifier.width(6.dp))
                    }
                    lead?.invoke()
                }
            } else {
                null
            },
            content = custom?.let { slot -> { color: Color -> slot(color) } },
            trailing = if (trail != null || count > 0) {
                { _: Color ->
                    trail?.invoke()
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
            } else {
                null
            },
            contentDescription = description?.invoke(option),
            testTag = testTag?.invoke(option),
        )
    }
    SegmentedControl(
        segments = segments,
        selection = selected,
        onSelect = onSelect,
        modifier = modifier,
        style = GlassSegmentedControlDefaults.segmentedStyle(
            embedded = embedded,
            textStyle = textStyle ?: MaterialTheme.typography.labelLarge,
        ),
    )
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

    /**
     * The SDK style of the strip (VAPP-89): capsule track and segments, no
     * segment padding (segments are equal-width and centre their content).
     * A STANDALONE strip pins [Height] and its segments fill it (36 - 2x3
     * inset; padding them too clipped every label by ~2dp); an [embedded]
     * one has no track and takes its height from [SegmentVerticalPadding].
     */
    fun segmentedStyle(embedded: Boolean, textStyle: TextStyle): SegmentedStyle = SegmentedStyle(
        height = Height,
        trackFill = ContainerFill,
        trackStroke = Hairline,
        trackStrokeWidth = GlassTokens.Hairline,
        segmentFill = ActiveFill,
        label = DesignTokens.Palette.Foreground.copy(alpha = TextEmphasis.Secondary),
        selectedLabel = DesignTokens.Palette.Foreground,
        disabledLabel = DesignTokens.Palette.Foreground.copy(alpha = TextEmphasis.Quaternary),
        inset = ContainerPadding,
        horizontalPadding = 0.dp,
        fontWeight = LabelWeight,
        textStyle = textStyle,
        capsule = true,
        segmentVerticalPadding = if (embedded) SegmentVerticalPadding else null,
        showsTrack = !embedded,
        segmentSpacing = SegmentSpacing,
    )

    /** Container and segments are both full capsules. */
    val Shape: RoundedCornerShape = RoundedCornerShape(percent = 50)
}
