package at.exponential.ui.primitives

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * The colours and sizes the generic primitives paint with. The painter
 * fills one from the surface's resolved theme; an embedding app fills one
 * from its own tokens (the Exponential app does, SLOP-18). Every primitive
 * reads it from [LocalPrimitiveTokens], so one value themes a whole subtree.
 */
@Immutable
data class PrimitiveTokens(
    val foreground: Color = Color(0xFF1C1C1E),
    val mutedForeground: Color = Color(0xFF8E8E93),
    val background: Color = Color.Transparent,
    val card: Color = Color.Transparent,
    val muted: Color = GRAY.copy(alpha = 0.15f),
    val border: Color = GRAY.copy(alpha = 0.3f),
    val input: Color = GRAY.copy(alpha = 0.25f),
    val primary: Color = Color(0xFF007AFF),
    val primaryForeground: Color = Color.White,
    val accent: Color = GRAY.copy(alpha = 0.2f),
    val destructive: Color = Color(0xFFFF3B30),
    val success: Color = Color(0xFF34C759),
    val warning: Color = Color(0xFFFF9500),
    val info: Color = Color(0xFF007AFF),
    val ring: Color = Color(0xFF007AFF),
    /** The theme's hairline width (`control.hairline`). */
    val hairline: Dp = 1.dp,
    /** The pill (`control.pill`), input (`control.input`) and row heights. */
    val pillHeight: Dp = 24.dp,
    val inputHeight: Dp = 36.dp,
    val rowHeight: Dp = 32.dp,
    val radiusSm: Dp = 4.dp,
    val radiusMd: Dp = 6.dp,
    val radiusLg: Dp = 8.dp,
    /** The sans and mono families by NAME (null = the system font). */
    val sansFamily: String? = null,
    val monoFamily: String? = null,
) {
    companion object {
        private val GRAY = Color(0.5f, 0.5f, 0.5f)

        /** Tokens that read as the system's own look (the default). */
        val SYSTEM = PrimitiveTokens()
    }
}

/** The tokens every primitive below reads. */
val LocalPrimitiveTokens = staticCompositionLocalOf { PrimitiveTokens.SYSTEM }

/** Theme every primitive inside [content] with [tokens]. */
@Composable
fun ProvidePrimitiveTokens(tokens: PrimitiveTokens, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalPrimitiveTokens provides tokens, content = content)
}

/** The semantic tone a primitive may be tinted with. */
enum class PrimitiveTone(val wire: String) {
    Neutral("neutral"), Primary("primary"), Success("success"), Warning("warning"), Danger("danger"), Info("info");

    /** The tone's colour in [tokens]. */
    fun color(tokens: PrimitiveTokens): Color = when (this) {
        Neutral -> tokens.mutedForeground
        Primary -> tokens.primary
        Success -> tokens.success
        Warning -> tokens.warning
        Danger -> tokens.destructive
        Info -> tokens.info
    }

    companion object {
        /** The tone named `wire` (null when unknown). */
        fun of(wire: String?): PrimitiveTone? = entries.firstOrNull { it.wire == wire }
    }
}
