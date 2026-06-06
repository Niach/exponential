package com.exponential.app.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

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

private val ExpoTypography = Typography(
    displayLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 57.sp, lineHeight = 64.sp, letterSpacing = (-0.25).sp),
    displayMedium = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 45.sp, lineHeight = 52.sp),
    displaySmall = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 36.sp, lineHeight = 44.sp),

    headlineLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.SemiBold, fontSize = 32.sp, lineHeight = 40.sp),
    headlineMedium = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.SemiBold, fontSize = 28.sp, lineHeight = 36.sp),
    headlineSmall = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.SemiBold, fontSize = 24.sp, lineHeight = 32.sp),

    titleLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.SemiBold, fontSize = 22.sp, lineHeight = 28.sp),
    titleMedium = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 16.sp, lineHeight = 24.sp, letterSpacing = 0.15.sp),
    titleSmall = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.1.sp),

    bodyLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 16.sp, lineHeight = 24.sp, letterSpacing = 0.5.sp),
    bodyMedium = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.25.sp),
    bodySmall = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Normal, fontSize = 12.sp, lineHeight = 16.sp, letterSpacing = 0.4.sp),

    labelLarge = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.1.sp),
    labelMedium = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 12.sp, lineHeight = 16.sp, letterSpacing = 0.5.sp),
    labelSmall = TextStyle(fontFamily = FontFamily.Default, fontWeight = FontWeight.Medium, fontSize = 11.sp, lineHeight = 16.sp, letterSpacing = 0.5.sp),
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
    MaterialTheme(
        colorScheme = ZincDarkColors,
        typography = ExpoTypography,
        shapes = ExpoShapes,
        content = content,
    )
}
