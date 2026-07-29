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
    // Background gradient — iOS Zinc 950 -> 900 (top -> bottom).
    val BackgroundTop = Color(0xFF09090B)
    val BackgroundBottom = Color(0xFF18181B)

    // Surface fills (approximating .ultraThinMaterial over dark).
    val RowFill = Color.White.copy(alpha = 0.05f)
    val RowFillActive = Color.White.copy(alpha = 0.15f)
    val SectionFill = Color.White.copy(alpha = 0.04f)
    val CardFill = Color.White.copy(alpha = 0.06f)

    /**
     * [CardFill] composited over the opaque card base (white .06 over #171717 ==
     * #252525) — the ONE opaque glass fill for surfaces that float over
     * arbitrary scrolling content (EXP-165/332/357). Compose has no cheap
     * backdrop blur, so the translucent fills above ghost whatever scrolls
     * behind them; anything that must stay READABLE while content moves under
     * it (menus, floating captions, the review bar's failure banner) uses this.
     */
    val OpaqueCardFill: Color = CardFill.compositeOver(DesignTokens.Palette.Card)

    // Hairline strokes.
    val StrokeRow = Color.White.copy(alpha = 0.06f)
    val StrokeSection = Color.White.copy(alpha = 0.08f)
    val StrokeCard = Color.White.copy(alpha = 0.10f)
    val StrokeActive = Color.White.copy(alpha = 0.20f)
    val Hairline = 0.5.dp

    // Corner radii (iOS GlassRow 10 / GlassSection 12 / GlassCard 16).
    val RowRadius = 10.dp
    val SectionRadius = 12.dp
    val CardRadius = 16.dp

    // Standard row padding.
    val RowPaddingH = 12.dp
    val RowPaddingV = 10.dp
}

/** The brand indigo (#6366f1) — iOS `Accent.indigo`: selection checkmarks, branch tints, send accents. */
val AccentIndigo = Color(0xFF6366F1)

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

/** Frosted pill/row surface — iOS `.glassRow()`. */
fun Modifier.glassRow(active: Boolean = false): Modifier {
    val shape = RoundedCornerShape(GlassTokens.RowRadius)
    return this
        .clip(shape)
        .background(if (active) GlassTokens.RowFillActive else GlassTokens.RowFill, shape)
        .border(GlassTokens.Hairline, if (active) GlassTokens.StrokeActive else GlassTokens.StrokeRow, shape)
}

/** Frosted grouped-section container — iOS `.glassSection()`. */
fun Modifier.glassSection(): Modifier {
    val shape = RoundedCornerShape(GlassTokens.SectionRadius)
    return this
        .clip(shape)
        .background(GlassTokens.SectionFill, shape)
        .border(GlassTokens.Hairline, GlassTokens.StrokeSection, shape)
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
 */
fun Modifier.glassButton(active: Boolean = false, opaque: Boolean = false): Modifier {
    val shape = RoundedCornerShape(percent = 50)
    return this
        .clip(shape)
        .then(if (opaque) Modifier.background(DesignTokens.Palette.Card, shape) else Modifier)
        .background(if (active) GlassTokens.RowFillActive else GlassTokens.RowFill, shape)
        .border(GlassTokens.Hairline, if (active) GlassTokens.StrokeActive else GlassTokens.StrokeRow, shape)
}

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
