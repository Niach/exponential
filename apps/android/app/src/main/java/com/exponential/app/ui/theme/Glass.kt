package com.exponential.app.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * Glass design tokens — a 1:1 port of the iOS app's GlassTheme.swift so the two
 * clients read as the same product. The look is translucent "glass" surfaces (a
 * faint white fill + hairline white stroke) floating over a dark zinc gradient
 * [AppBackground]. We approximate iOS's `.ultraThinMaterial` with a low-alpha
 * white fill over the gradient (the chosen alpha-fill approach — no real blur,
 * no extra dependency).
 */
object GlassTokens {
    // EXP-698: every member below is an ALIAS of the generated
    // [DesignTokens] twin — the hand-written literals this object used to
    // carry drifted from the shared design-token source by a percent or two on
    // every rung. Nothing here may be written down again; add a token to
    // packages/design-tokens/tokens.json and alias it.

    // Background gradient — iOS Zinc 950 -> 900 (top -> bottom).
    val BackgroundTop = DesignTokens.Glass.BackgroundTop
    val BackgroundBottom = DesignTokens.Glass.BackgroundBottom

    // Surface fills (approximating .ultraThinMaterial over dark).
    val RowFill = DesignTokens.Glass.FillRow
    val RowFillActive = DesignTokens.Glass.FillActive
    val SectionFill = DesignTokens.Glass.FillSection
    val CardFill = DesignTokens.Glass.FillCard

    /**
     * [CardFill] composited over the opaque card base (white .06 over #171717 ==
     * #252525) — the ONE opaque glass fill for surfaces that float over
     * arbitrary scrolling content (EXP-165/332/357). Compose has no cheap
     * backdrop blur, so the translucent fills above ghost whatever scrolls
     * behind them; anything that must stay READABLE while content moves under
     * it (menus, floating captions) uses this.
     */
    val OpaqueCardFill: Color = CardFill.compositeOver(DesignTokens.Palette.Card)

    // Hairline strokes.
    val StrokeRow = DesignTokens.Glass.StrokeRow
    val StrokeSection = DesignTokens.Glass.StrokeSection
    val StrokeCard = DesignTokens.Glass.StrokeCard

    /** The heaviest non-active hairline — bars, composers, segmented capsules. */
    val StrokeStrong = DesignTokens.Glass.StrokeStrong
    val StrokeActive = DesignTokens.Glass.StrokeActive
    val Hairline = 0.5.dp

    /**
     * The filled portion of a progress/usage track (the agent rate-limit bar).
     * It is NOT a glass fill: a track has to read as a solid quantity against
     * the glass row it sits in, so it is opaque white at 30% — the one place
     * in the app that draws one, named here so it stays one number.
     */
    val UsageFill: Color = Color.White.copy(alpha = 0.30f)

    // Corner radii (iOS GlassRow 10 / GlassGroup 12 / GlassCard 16).
    val RowRadius = DesignTokens.Radius.Md
    val GroupRadius = DesignTokens.Radius.Lg
    val CardRadius = DesignTokens.Radius.Xl

    /** The one circular/segmented control diameter (iOS 32pt). */
    val ControlSize = DesignTokens.Size.ControlMd

    // Standard row padding.
    val RowPaddingH = 12.dp
    val RowPaddingV = 10.dp
}


/** iOS `TextOpacity` tiers — apply as foreground alpha over onSurface / white. */
object TextEmphasis {
    const val Primary = 1.0f
    const val Secondary = 0.7f
    const val Tertiary = 0.5f
    const val Quaternary = 0.3f
}

/**
 * Full-bleed dark zinc gradient that every screen floats on (iOS `AppBackground`).
 * Place once behind the NavHost; screens then use transparent Scaffolds / top
 * bars so the gradient shows through.
 */
@Composable
fun AppBackground(
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit,
) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(GlassTokens.BackgroundTop, GlassTokens.BackgroundBottom),
                ),
            ),
        content = content,
    )
}

/**
 * Frosted pill/row surface — iOS `.glassRow()`. [opaque] lays a solid Card
 * fill beneath the glass tint (same shape) for rows floating over scrolling
 * content, where the low-alpha fill alone lets it bleed through (EXP-165 —
 * the steer screen's floating Latest-changes chip).
 */
fun Modifier.glassRow(active: Boolean = false, opaque: Boolean = false): Modifier {
    val shape = RoundedCornerShape(GlassTokens.RowRadius)
    return this
        .clip(shape)
        .then(if (opaque) Modifier.background(DesignTokens.Palette.Card, shape) else Modifier)
        .background(if (active) GlassTokens.RowFillActive else GlassTokens.RowFill, shape)
        .border(GlassTokens.Hairline, if (active) GlassTokens.StrokeActive else GlassTokens.StrokeRow, shape)
}

/**
 * Frosted grouped-row container — iOS `.glassGroup()`. BORDERLESS on purpose
 * (EXP-698): a group is a stack of rows separated by hairlines, so an outer
 * stroke around it drew a second, competing edge. A bordered panel around FREE
 * content is [glassCard] instead.
 */
fun Modifier.glassGroup(): Modifier {
    val shape = RoundedCornerShape(GlassTokens.GroupRadius)
    return this
        .clip(shape)
        .background(GlassTokens.RowFill, shape)
}

/**
 * Frosted elevated card — iOS `.glassCard()`. [opaque] swaps the translucent
 * tint for [GlassTokens.OpaqueCardFill] so the card can float over scrolling
 * content without the rows underneath ghosting through it (EXP-357).
 */
fun Modifier.glassCard(opaque: Boolean = false): Modifier {
    val shape = RoundedCornerShape(GlassTokens.CardRadius)
    return this
        .clip(shape)
        .background(if (opaque) GlassTokens.OpaqueCardFill else GlassTokens.CardFill, shape)
        .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
}

/**
 * Capsule glass button / filter pill — iOS `.glassButton()`. [opaque] lays a
 * solid Card fill beneath the glass tint (same shape) for pills floating over
 * scrolling content, where the low-alpha fill alone lets it bleed through
 * (EXP-165).
 *
 * EXP-698: a BUTTON is the card rung (fill + hairline), a notch above the row
 * rung it used to borrow — a pill has to read as a control against the rows
 * around it, not as another row. This modifier is the chrome UNDER
 * `components/GlassPill.kt` and has no other caller: a capsule with a label in
 * it is a `GlassPill`, never a hand-rolled Row with this on it.
 */
fun Modifier.glassButton(
    active: Boolean = false,
    opaque: Boolean = false,
    /**
     * The ONE emphatic capsule (EXP-698 r4, mirrored on every client — web
     * `primary`, desktop `.primary()`, iOS `primary:`): the solid near-white
     * [DesignTokens.Palette.Primary] with dark content and NO hairline, for
     * the single call to action on a surface (the issue card's "Watch"). It is
     * PAINT only — orthogonal to the pill's size and mode — and never applies
     * to a disabled pill, which keeps the ordinary dimmed glass.
     */
    primary: Boolean = false,
    /** Held down: the primary fill dips, since it has no glass to brighten. */
    pressed: Boolean = false,
): Modifier {
    val shape = RoundedCornerShape(percent = 50)
    if (primary) {
        return this
            .clip(shape)
            .background(
                if (pressed) {
                    DesignTokens.Palette.Primary.copy(alpha = PrimaryPressedAlpha)
                } else {
                    DesignTokens.Palette.Primary
                },
                shape,
            )
    }
    return this
        .clip(shape)
        .then(if (opaque) Modifier.background(DesignTokens.Palette.Card, shape) else Modifier)
        .background(if (active) GlassTokens.RowFillActive else GlassTokens.CardFill, shape)
        .border(GlassTokens.Hairline, if (active) GlassTokens.StrokeActive else GlassTokens.StrokeCard, shape)
}

/** How far a pressed primary capsule dims — the glass rungs use fill/stroke
 *  swaps for this, which a solid fill has nothing to swap to. */
private const val PrimaryPressedAlpha = 0.85f

/**
 * Let an element escape its parent's horizontal padding and run edge to edge
 * (EXP-327 — the rule above the activity timeline). Compose has no negative
 * padding, so the layout is re-measured [inset] wider on each side and placed
 * back at `-inset`.
 */
fun Modifier.fullBleed(inset: Dp): Modifier = layout { measurable, constraints ->
    val extra = inset.roundToPx() * 2
    val placeable = measurable.measure(
        constraints.copy(
            minWidth = (constraints.minWidth + extra).coerceAtLeast(0),
            maxWidth = if (constraints.maxWidth == Constraints.Infinity) {
                Constraints.Infinity
            } else {
                constraints.maxWidth + extra
            },
        )
    )
    // Report the ORIGINAL width so siblings keep laying out inside the padding.
    layout(placeable.width - extra, placeable.height) {
        placeable.place(-inset.roundToPx(), 0)
    }
}
