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

// Dark-only, by design — matches iOS and web. The previous DarkColors only
// defined a handful of roles, leaving Material 3 components to fall back to
// their library defaults (a muted purple); this maps the full role set
// against the web's zinc OKLCH palette so every M3 surface lands on the
// right tone without further per-component overrides.
private val ZincDarkColors = darkColorScheme(
    primary = Color(0xFFFAFAFA),
    onPrimary = Color(0xFF18181B),
    primaryContainer = Color(0xFF27272A),
    onPrimaryContainer = Color(0xFFFAFAFA),
    inversePrimary = Color(0xFF18181B),

    secondary = Color(0xFFA1A1AA),
    onSecondary = Color(0xFF18181B),
    secondaryContainer = Color(0xFF27272A),
    onSecondaryContainer = Color(0xFFE4E4E7),

    tertiary = Color(0xFFD4D4D8),
    onTertiary = Color(0xFF18181B),
    tertiaryContainer = Color(0xFF27272A),
    onTertiaryContainer = Color(0xFFE4E4E7),

    background = Color(0xFF09090B),
    onBackground = Color(0xFFFAFAFA),

    surface = Color(0xFF09090B),
    onSurface = Color(0xFFFAFAFA),
    surfaceVariant = Color(0xFF18181B),
    onSurfaceVariant = Color(0xFFA1A1AA),
    surfaceTint = Color(0xFFFAFAFA),
    inverseSurface = Color(0xFFFAFAFA),
    inverseOnSurface = Color(0xFF18181B),

    // Material 3 tonal surface containers — used by NavigationBar, Card,
    // BottomSheet, Dialog, FilterChip selected, etc. Without these mapped,
    // those components silently draw on the wrong tone.
    surfaceContainerLowest = Color(0xFF09090B),
    surfaceContainerLow = Color(0xFF0F0F11),
    surfaceContainer = Color(0xFF18181B),
    surfaceContainerHigh = Color(0xFF1F1F22),
    surfaceContainerHighest = Color(0xFF27272A),

    outline = Color(0xFF3F3F46),
    outlineVariant = Color(0xFF27272A),

    error = Color(0xFFF43F5E),
    onError = Color(0xFFFAFAFA),
    errorContainer = Color(0xFF7F1D1D),
    onErrorContainer = Color(0xFFFECDD3),

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
