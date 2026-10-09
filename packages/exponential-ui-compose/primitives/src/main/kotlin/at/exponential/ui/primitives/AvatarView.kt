package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

/**
 * The avatar fallback rule every Exponential UI renderer shares: two
 * initials, tinted from a seed (React `seedHue`, gpui `seed_hue`, Swift
 * `AvatarFallback`: `h = h * 31 + utf16unit` as a wrapping UInt32, mod 360).
 */
object AvatarFallback {
    /** The first letters of the first two whitespace-separated words, uppercased. */
    fun initials(name: String): String =
        name.split(Regex("\\s+"))
            .filter { it.isNotEmpty() }
            .take(2)
            .joinToString("") { it.substring(0, it.offsetByCodePoints(0, 1)).uppercase() }

    /** The seed's hue in degrees, 0..359. */
    fun seedHue(seed: String): Double {
        var h = 0u
        for (unit in seed) {
            h = h * 31u + unit.code.toUInt()
        }
        return (h % 360u).toDouble()
    }

    /** The fill (hsl h,0.7,0.55 at 0.22 alpha) and the letter colour (hsl h,0.6, dark ? 0.72 : 0.38) for [seed]. */
    fun tint(seed: String, dark: Boolean): AvatarTint {
        val hue = seedHue(seed)
        return AvatarTint(
            fill = Rgba.hsl(hue, 0.7, 0.55, 0.22).color,
            ink = Rgba.hsl(hue, 0.6, if (dark) 0.72 else 0.38).color,
        )
    }
}

/** A seed's avatar tint: the disc [fill] and the letters' [ink]. */
data class AvatarTint(val fill: Color, val ink: Color)

/**
 * A round avatar: [picture] when given, else tinted initials. [fill]/[ink]
 * override the seed tint (an empty seed falls back to the tokens' `muted`
 * and `foreground`); [initials] draws those letters verbatim; [fontSize]
 * defaults to 40% of [size]; [cornerRadius] clips a rounded square instead
 * of a circle. The letters sit on a line box trimmed to the glyph, so they
 * centre optically. [textStyle] is the base the letters merge onto.
 */
@Composable
fun AvatarView(
    name: String,
    modifier: Modifier = Modifier,
    seed: String? = null,
    size: Dp = 32.dp,
    dark: Boolean = true,
    fill: Color? = null,
    ink: Color? = null,
    fontSize: TextUnit = TextUnit.Unspecified,
    fontWeight: FontWeight = FontWeight.Medium,
    initials: String? = null,
    cornerRadius: Dp? = null,
    textStyle: TextStyle? = null,
    contentDescription: String? = name,
    picture: (@Composable () -> Unit)? = null,
) {
    val tokens = LocalPrimitiveTokens.current
    val key = seed ?: name
    val shape = if (cornerRadius != null) RoundedCornerShape(cornerRadius) else CircleShape
    Box(
        modifier = modifier
            .size(size)
            .clip(shape)
            .then(if (contentDescription != null) Modifier.semantics { this.contentDescription = contentDescription } else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        if (picture != null) {
            picture()
        } else {
            val tint = remember(key) { AvatarFallback.tint(key, dark) }
            val letters = initials ?: AvatarFallback.initials(name).ifEmpty { "?" }
            val bg = fill ?: if (key.isEmpty()) tokens.muted else tint.fill
            val fg = ink ?: if (key.isEmpty()) tokens.foreground else tint.ink
            val sizeSp = if (fontSize.isSpecified) fontSize else (size.value * 0.4f).roundToInt().sp
            Box(Modifier.size(size).background(bg, shape), contentAlignment = Alignment.Center) {
                BasicText(
                    letters,
                    maxLines = 1,
                    style = (textStyle ?: TextStyle()).merge(
                        TextStyle(
                            color = fg,
                            fontSize = sizeSp,
                            fontWeight = fontWeight,
                            lineHeight = sizeSp,
                            platformStyle = PlatformTextStyle(includeFontPadding = false),
                            lineHeightStyle = LineHeightStyle(
                                alignment = LineHeightStyle.Alignment.Center,
                                trim = LineHeightStyle.Trim.Both,
                            ),
                        ),
                    ),
                )
            }
        }
    }
}
