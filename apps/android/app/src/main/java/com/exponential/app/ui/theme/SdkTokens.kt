package com.exponential.app.ui.theme

import androidx.compose.ui.graphics.Color
import at.exponential.ui.primitives.DrawnSwitchColors
import at.exponential.ui.primitives.PrimitiveTokens

// VAPP-89 / SLOP-18: the app's ONE `PrimitiveTokens`, provided at the theme
// root so every Exponential UI SDK primitive (`PillView`, `SegmentedControl`,
// `fieldChrome`, `AvatarView`, `MeterTrack`, `RingView`, `EmptyStateView`,
// `DisclosureHeader`, `MarkdownView`) paints in the app's palette by default.
// The glass composables still pass their own rung numbers explicitly.

/** The app's palette and control rungs as SDK primitive tokens. */
val AppPrimitiveTokens: PrimitiveTokens = PrimitiveTokens(
    foreground = DesignTokens.Palette.Foreground,
    mutedForeground = DesignTokens.Palette.MutedForeground,
    background = DesignTokens.Palette.Background,
    card = GlassTokens.CardFill,
    muted = GlassTokens.RowFillActive,
    border = GlassTokens.StrokeCard,
    input = GlassTokens.CardFill,
    primary = DesignTokens.Palette.Primary,
    primaryForeground = DesignTokens.Palette.PrimaryForeground,
    accent = DesignTokens.Palette.Accent,
    destructive = DesignTokens.Palette.Destructive,
    success = DesignTokens.Semantic.Green,
    warning = DesignTokens.Semantic.Yellow,
    info = DesignTokens.Semantic.Blue,
    ring = GlassTokens.StrokeActive,
    hairline = GlassTokens.Hairline,
    pillHeight = DesignTokens.Size.ControlSm,
    inputHeight = DesignTokens.Size.InputHeight,
    rowHeight = DesignTokens.Size.RowHeight,
    radiusSm = DesignTokens.Radius.Sm,
    radiusMd = DesignTokens.Radius.Md,
    radiusLg = DesignTokens.Radius.Lg,
)

/** The app's primitive tokens (one value; dark-only like the theme). */
fun appPrimitiveTokens(): PrimitiveTokens = AppPrimitiveTokens

/**
 * THE toggle palette (EXP-698), as the SDK drawn-switch colours: off = the
 * shared active fill with a white knob and no border, on = the primary
 * track with a primary-foreground knob. `glassSwitchColors()` maps it onto
 * M3's `SwitchColors`; a `DrawnSwitch` takes it directly.
 */
val AppSwitchColors: DrawnSwitchColors = DrawnSwitchColors(
    trackOn = DesignTokens.Palette.Primary,
    trackOff = GlassTokens.RowFillActive,
    thumb = Color.White,
    thumbOn = DesignTokens.Palette.PrimaryForeground,
    trackOnDisabled = DesignTokens.Palette.Primary.copy(alpha = TextEmphasis.Tertiary),
    trackOffDisabled = GlassTokens.RowFill,
    thumbDisabled = Color.White.copy(alpha = TextEmphasis.Tertiary),
    thumbOnDisabled = DesignTokens.Palette.PrimaryForeground.copy(alpha = TextEmphasis.Secondary),
)
