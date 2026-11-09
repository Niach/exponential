package at.exponential.ui.primitives

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
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

/**
 * The avatar fallback rule every Exponential UI renderer shares
 * (packages/exponential-ui/fixtures/avatar.json, #EXP-1629), the app's own
 * (design-tokens `avatar`, EXP-698 r4): a seed (a user id) picks
 * `PALETTE[fnv1a32(utf8(seed)) % 8]`; the disc fills with that hue at
 * [FILL_ALPHA] and the initials draw in it at full alpha, no stroke. No seed
 * = the caller's muted colours.
 */
object AvatarFallback {
    /** design-tokens `avatar` in ITS order (append-only, never reorder: the index is what the hash lands on). */
    val PALETTE: List<String> = listOf("#F87171", "#FB923C", "#FACC15", "#4ADE80", "#2DD4BF", "#60A5FA", "#A78BFA", "#F472B6")

    /** The hue's alpha in the fill. */
    const val FILL_ALPHA: Float = 0.2f

    /** [PALETTE] as colours. */
    val palette: List<Color> = PALETTE.map { requireNotNull(parseHexColor(it)) }

    /**
     * The initials of [name]: an email reduces to its local part, the rest
     * splits on space . _ - +; two or more parts give their first letters,
     * one part its first two characters (code points); uppercased; nothing = `?`.
     */
    fun initials(name: String?): String {
        val base = name.orEmpty().trim()
        val at = base.indexOf('@')
        val local = if (at >= 0) base.substring(0, at) else base
        val parts = local.split(' ', '.', '_', '-', '+').filter { it.isNotBlank() }
        val picked = if (parts.size >= 2) firstCodePoints(parts[0], 1) + firstCodePoints(parts[1], 1) else firstCodePoints(local, 2)
        return if (picked.isEmpty()) "?" else picked.uppercase()
    }

    private fun firstCodePoints(s: String, n: Int): String =
        s.substring(0, s.offsetByCodePoints(0, minOf(n, s.codePointCount(0, s.length))))

    /**
     * What the hue is keyed on (fixtures/avatar.json `keys`, #EXP-1754): the
     * [id], else the trimmed [name], else the trimmed [email]; blank counts
     * as absent. `null` = no key: the muted fallback.
     */
    fun hueKey(id: String?, name: String? = null, email: String? = null): String? {
        if (!id.isNullOrBlank()) return id
        for (label in listOf(name, email)) {
            val trimmed = label.orEmpty().trim()
            if (trimmed.isNotEmpty()) return trimmed
        }
        return null
    }

    /** FNV-1a 32-bit over the seed's UTF-8 bytes, modulo the palette size. */
    fun hueIndex(seed: String): Int {
        var hash = 0x811C9DC5u
        for (byte in seed.toByteArray(Charsets.UTF_8)) {
            hash = (hash xor (byte.toUInt() and 0xFFu)) * 0x01000193u
        }
        return (hash % PALETTE.size.toUInt()).toInt()
    }

    /** The seed's tint: the hue at [FILL_ALPHA] under the letters in the hue. */
    fun tint(seed: String): AvatarTint {
        val hue = palette[hueIndex(seed)]
        return AvatarTint(fill = hue.copy(alpha = FILL_ALPHA), ink = hue)
    }
}

/** A seed's avatar tint: the disc [fill] and the letters' [ink]. */
data class AvatarTint(val fill: Color, val ink: Color)

/**
 * A round avatar: [picture] when given (wearing a [ringWidth] [ring] when
 * one is given, the `Avatar/image` hairline), else the initials on the
 * [seed]'s tint; no seed (null or empty) paints the tokens' `muted` and
 * `mutedForeground`. [fill]/[ink] override either; [initials] draws those
 * letters verbatim; [fontSize] defaults to half of [size]; [cornerRadius]
 * clips a rounded square instead of a circle. The letters sit on a line box
 * trimmed to the glyph, so they centre optically. [textStyle] is the base
 * the letters merge onto. [pictureFill] (#EXP-1629, the `Avatar/image`
 * background) fills the whole round box behind the picture, under the ring.
 */
@Composable
fun AvatarView(
    name: String,
    modifier: Modifier = Modifier,
    seed: String? = null,
    size: Dp = 32.dp,
    fill: Color? = null,
    ink: Color? = null,
    fontSize: TextUnit = TextUnit.Unspecified,
    fontWeight: FontWeight = FontWeight.Normal,
    initials: String? = null,
    cornerRadius: Dp? = null,
    textStyle: TextStyle? = null,
    contentDescription: String? = name,
    ring: Color? = null,
    ringWidth: Dp = 1.dp,
    picture: (@Composable () -> Unit)? = null,
    pictureFill: Color? = null,
) {
    val tokens = LocalPrimitiveTokens.current
    val key = seed.orEmpty()
    val shape = if (cornerRadius != null) RoundedCornerShape(cornerRadius) else CircleShape
    Box(
        modifier = modifier
            .size(size)
            .clip(shape)
            .then(if (picture != null && pictureFill != null) Modifier.background(pictureFill, shape) else Modifier)
            .then(if (picture != null && ring != null && ringWidth > 0.dp) Modifier.border(ringWidth, ring, shape) else Modifier)
            .then(if (contentDescription != null) Modifier.semantics { this.contentDescription = contentDescription } else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        if (picture != null) {
            // The IDE's gpui-component Avatar: the picture sits inside the
            // ring, so a translucent ring reads as a hairline over the ground.
            if (ring != null && ringWidth > 0.dp) {
                Box(Modifier.fillMaxSize().padding(ringWidth).clip(shape), contentAlignment = Alignment.Center) { picture() }
            } else {
                picture()
            }
        } else {
            val tint = remember(key) { if (key.isEmpty()) null else AvatarFallback.tint(key) }
            val letters = initials ?: AvatarFallback.initials(name)
            val bg = fill ?: tint?.fill ?: tokens.muted
            val fg = ink ?: tint?.ink ?: tokens.mutedForeground
            val sizeSp = if (fontSize.isSpecified) fontSize else (size.value / 2f).sp
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
