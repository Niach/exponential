package com.exponential.app.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import at.exponential.ui.primitives.LocalPrimitiveTokens

// Dark-only, by design — matches iOS and web. Maps the full Material 3 role set
// onto the shared palette (DesignTokens.Palette, generated from the web OKLCH
// theme in packages/design-tokens) so every M3 surface lands on the right tone
// without per-component overrides. The few roles with no web equivalent — the
// intermediate surface-container tiers, the opaque outline, and the error
// container tints — stay as literals (Material-3-specific interpolations) and
// are commented as such.
private val ZincDarkColors = darkColorScheme(
    primary = DesignTokens.Palette.Primary,
    onPrimary = DesignTokens.Palette.PrimaryForeground,
    primaryContainer = DesignTokens.Palette.Secondary,
    onPrimaryContainer = DesignTokens.Palette.Foreground,
    inversePrimary = DesignTokens.Palette.PrimaryForeground,

    secondary = DesignTokens.Palette.MutedForeground,
    onSecondary = DesignTokens.Palette.Background,
    secondaryContainer = DesignTokens.Palette.Secondary,
    onSecondaryContainer = DesignTokens.Palette.Foreground,

    tertiary = Color(0xFFD4D4D8), // zinc-300; no web role — kept as a literal
    onTertiary = DesignTokens.Palette.Background,
    tertiaryContainer = DesignTokens.Palette.Secondary,
    onTertiaryContainer = DesignTokens.Palette.Foreground,

    background = DesignTokens.Palette.Background,
    onBackground = DesignTokens.Palette.Foreground,

    surface = DesignTokens.Palette.Background,
    onSurface = DesignTokens.Palette.Foreground,
    surfaceVariant = DesignTokens.Palette.Card,
    onSurfaceVariant = DesignTokens.Palette.MutedForeground,
    surfaceTint = DesignTokens.Palette.Foreground,
    inverseSurface = DesignTokens.Palette.Foreground,
    inverseOnSurface = DesignTokens.Palette.Background,

    // Material 3 tonal surface containers — used by NavigationBar, Card,
    // BottomSheet, Dialog, FilterChip selected, etc. The endpoints come from the
    // shared palette; the two intermediate tiers are interpolations (no web role).
    surfaceContainerLowest = DesignTokens.Palette.Background,
    surfaceContainerLow = Color(0xFF0F0F11),
    surfaceContainer = DesignTokens.Palette.Card,
    surfaceContainerHigh = Color(0xFF1F1F22),
    surfaceContainerHighest = DesignTokens.Palette.Secondary,

    outline = Color(0xFF3F3F46), // zinc-700; web border is white/10%, too faint for M3 outlines
    outlineVariant = DesignTokens.Palette.Secondary,

    error = DesignTokens.Palette.Destructive,
    onError = DesignTokens.Palette.Foreground,
    errorContainer = Color(0xFF7F1D1D), // red-900; no web role
    onErrorContainer = Color(0xFFFECDD3), // rose-200; no web role

    scrim = Color(0xFF000000),
)

// #EXP-2215: Inter (ExpSans) on web's scale: every slot sits on the shared
// token pairs (design-tokens `type`, Tailwind's text-xs…2xl) with web's
// normal tracking (letterSpacing 0, never Material 3's 0.1–0.5 sp); the
// display and large headline slots, which web has no pair for, keep
// Material's sizes at that tracking.
private val ExpoTypography = Typography(
    displayLarge = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Normal, fontSize = 57.sp, lineHeight = 64.sp, letterSpacing = 0.sp),
    displayMedium = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Normal, fontSize = 45.sp, lineHeight = 52.sp, letterSpacing = 0.sp),
    displaySmall = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Normal, fontSize = 36.sp, lineHeight = 44.sp, letterSpacing = 0.sp),

    headlineLarge = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.SemiBold, fontSize = 32.sp, lineHeight = 40.sp, letterSpacing = 0.sp),
    headlineMedium = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.SemiBold, fontSize = 28.sp, lineHeight = 36.sp, letterSpacing = 0.sp),
    headlineSmall = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.SemiBold, fontSize = DesignTokens.Typography.Size.Xl2.sp, lineHeight = DesignTokens.Typography.LineHeight.Xl2.sp, letterSpacing = 0.sp),

    titleLarge = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.SemiBold, fontSize = DesignTokens.Typography.Size.Xl.sp, lineHeight = DesignTokens.Typography.LineHeight.Xl.sp, letterSpacing = 0.sp),
    titleMedium = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Medium, fontSize = DesignTokens.Typography.Size.Base.sp, lineHeight = DesignTokens.Typography.LineHeight.Base.sp, letterSpacing = 0.sp),
    titleSmall = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Medium, fontSize = DesignTokens.Typography.Size.Sm.sp, lineHeight = DesignTokens.Typography.LineHeight.Sm.sp, letterSpacing = 0.sp),

    bodyLarge = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Normal, fontSize = DesignTokens.Typography.Size.Base.sp, lineHeight = DesignTokens.Typography.LineHeight.Base.sp, letterSpacing = 0.sp),
    bodyMedium = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Normal, fontSize = DesignTokens.Typography.Size.Sm.sp, lineHeight = DesignTokens.Typography.LineHeight.Sm.sp, letterSpacing = 0.sp),
    bodySmall = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Normal, fontSize = DesignTokens.Typography.Size.Xs.sp, lineHeight = DesignTokens.Typography.LineHeight.Xs.sp, letterSpacing = 0.sp),

    labelLarge = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Medium, fontSize = DesignTokens.Typography.Size.Sm.sp, lineHeight = DesignTokens.Typography.LineHeight.Sm.sp, letterSpacing = 0.sp),
    labelMedium = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Medium, fontSize = DesignTokens.Typography.Size.Xs.sp, lineHeight = DesignTokens.Typography.LineHeight.Xs.sp, letterSpacing = 0.sp),
    labelSmall = TextStyle(fontFamily = ExpSans, fontWeight = FontWeight.Medium, fontSize = DesignTokens.Typography.Size.Xs.sp, lineHeight = DesignTokens.Typography.LineHeight.Xs.sp, letterSpacing = 0.sp),
)

// Matches the web's `rounded-*` radii so dialogs/sheets/cards/chips feel
// visually consistent across platforms.
private val ExpoShapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

@Composable
fun ExponentialTheme(content: @Composable () -> Unit) {
    // EXP-523: one read of the OS animation scale for the whole tree. Provided
    // HERE and not in MainActivity so previews, instrumentation tests and any
    // future entry point cannot forget it — they all go through the theme.
    // VAPP-89: the SDK primitives' tokens ride the same root ([AppPrimitiveTokens]).
    CompositionLocalProvider(
        LocalReduceMotion provides rememberReduceMotion(),
        LocalPrimitiveTokens provides AppPrimitiveTokens,
    ) {
        MaterialTheme(
            colorScheme = ZincDarkColors,
            typography = ExpoTypography,
            shapes = ExpoShapes,
            content = content,
        )
    }
}
