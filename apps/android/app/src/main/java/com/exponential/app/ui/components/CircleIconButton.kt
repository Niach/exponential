package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Circular glass icon button (iOS .ultraThinMaterial nav circle) — same fill +
 * hairline stroke combination as Modifier.glassRow, just on a circle.
 *
 * Shared by the issue-list nav row (board switcher / filters) and EVERY top
 * bar's back / trailing icon button (EXP-568, EXP-577) — iOS 26 renders each
 * nav-bar item as exactly this circle, so Android's `TopAppBar` slots use it
 * instead of Material's bare `IconButton` (see [TopBarBackButton]).
 *
 * [tint] overrides the secondary-emphasis glyph color (the red kill switch);
 * [enabled] dims the glyph to quaternary and drops the tap.
 *
 * EXP-698: ONE rung everywhere — [GlassTokens.ControlSize] (32dp) circle,
 * 17dp glyph — nav bars, list rows and sheet headers alike, and the card
 * fill/stroke rung a control sits on, so it reads as a button against the rows
 * around it. 17 is iOS's `AppIcon.Size.medium`, the glyph its nav circles draw;
 * web and desktop put a 16 glyph in the same circle. No call site overrides
 * [size]; a handful of slim list rows still drop [glyphSize] to 16. [active] is the
 * pressed/on look (`glassButton(active = true)`'s twin) — no call site takes
 * it today; it is kept so a toggling circle need not re-type the recipe.
 *
 * The 32dp circle is the VISUAL, not the target: the inner box takes
 * [minimumInteractiveComponentSize] so the tap area is the platform's 48dp
 * minimum, while the outer box keeps reporting [size] — the extra 8dp on each
 * edge OVERFLOWS into the row's own padding instead of padding the row taller
 * (M3's `IconButton` grows its parent, which is exactly what made the comment
 * header three times the height of its text in EXP-398). A caller that has
 * waived the minimum via `LocalMinimumInteractiveComponentSize` gets no
 * enforcement, which is the modifier's own contract.
 */
@Composable
fun CircleIconButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tint: Color? = null,
    enabled: Boolean = true,
    active: Boolean = false,
    size: Dp = GlassTokens.ControlSize,
    glyphSize: Dp = 17.dp,
) {
    val glyph = tint ?: MaterialTheme.colorScheme.onSurface.copy(
        alpha = if (enabled) TextEmphasis.Secondary else TextEmphasis.Quaternary,
    )
    Box(
        modifier = modifier.size(size),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .minimumInteractiveComponentSize()
                .size(size)
                .clip(CircleShape)
                .background(if (active) GlassTokens.RowFillActive else GlassTokens.CardFill, CircleShape)
                .border(
                    GlassTokens.Hairline,
                    if (active) GlassTokens.StrokeActive else GlassTokens.StrokeCard,
                    CircleShape,
                )
                .clickable(enabled = enabled, onClick = onClick),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                icon,
                contentDescription = contentDescription,
                modifier = Modifier.size(glyphSize),
                tint = glyph,
            )
        }
    }
}

/**
 * The one back button for `TopAppBar.navigationIcon` slots: [CircleIconButton]
 * with the chevron and the 8dp leading inset that aligns the circle with the
 * screen's 16dp content gutter (the slot already contributes 8dp).
 */
@Composable
fun TopBarBackButton(
    onClick: () -> Unit,
    contentDescription: String = "Back",
    enabled: Boolean = true,
) {
    CircleIconButton(
        ExpIcons.uiBack,
        contentDescription,
        onClick = onClick,
        modifier = Modifier.padding(start = 8.dp),
        enabled = enabled,
    )
}

/** Trailing twin of [TopBarBackButton] for `TopAppBar.actions` slots. */
@Composable
fun TopBarActionButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    tint: Color? = null,
    enabled: Boolean = true,
) {
    CircleIconButton(
        icon,
        contentDescription,
        onClick = onClick,
        modifier = Modifier.padding(end = 8.dp),
        tint = tint,
        enabled = enabled,
    )
}
