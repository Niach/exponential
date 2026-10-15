package at.exponential.ui.compose

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.LinearGradientShader
import androidx.compose.ui.graphics.Shader
import androidx.compose.ui.graphics.ShaderBrush
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalDensity
import at.exponential.ui.ffi.textShimmerJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.flag
import at.exponential.ui.primitives.parseHexColor
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin

/**
 * EXP-1324: the Text `shimmer` colours + timing for one text colour, from
 * the core (`textShimmerJson`, catalog/style.json `textShimmer`).
 */
internal data class TextShimmerSpec(
    val base: Color,
    val highlight: Color,
    val mid: Color,
    val durationMs: Int,
    val angleDeg: Float,
    val spreadPx: Float,
) {
    companion object {
        /** `#rrggbbaa` of [c] (the facade's colour input). */
        fun hex(c: Color): String {
            val argb = c.toArgb()
            return "#%02x%02x%02x%02x".format((argb shr 16) and 0xff, (argb shr 8) and 0xff, argb and 0xff, (argb ushr 24) and 0xff)
        }

        /** The spec of [color] in light / [dark] mode; null when the core refuses it. */
        fun of(color: Color, dark: Boolean): TextShimmerSpec? = try {
            parse(JsonValue.parse(textShimmerJson(hex(color), dark)))
        } catch (_: Throwable) {
            null
        }

        fun parse(v: JsonValue): TextShimmerSpec? = TextShimmerSpec(
            base = v["base"]?.string?.let(::parseHexColor) ?: return null,
            highlight = v["highlight"]?.string?.let(::parseHexColor) ?: return null,
            mid = v["mid"]?.string?.let(::parseHexColor) ?: return null,
            durationMs = (v["durationMs"]?.number ?: 2000.0).toInt(),
            angleDeg = (v["angleDeg"]?.number ?: 20.0).toFloat(),
            spreadPx = (v["spreadPx"]?.number ?: 60.0).toFloat(),
        )
    }
}

/**
 * The shimmer fill at CSS `background-position` [position] (1 = the
 * gradient's right edge on the box's right edge, 0 = its left edge on the
 * box's left edge): a linear gradient at 90deg + `angleDeg` (CSS angle),
 * 200% of the text box + 2 × spread wide, stops base / mid / highlight /
 * mid / base at 50% ∓ spread, ∓ spread / 2 and 50%.
 */
internal class TextShimmerBrush(
    private val spec: TextShimmerSpec,
    private val position: Float,
    private val density: Float,
) : ShaderBrush() {
    override fun createShader(size: Size): Shader {
        val spread = spec.spreadPx * density
        val w = size.width
        val h = size.height
        val imageW = 2f * w + 2f * spread
        val left = (w - imageW) * position
        val cx = left + imageW / 2f
        val cy = h / 2f
        // CSS: 0deg points up, 90deg right; the gradient line spans the image.
        val theta = Math.toRadians(90.0 + spec.angleDeg)
        val dx = sin(theta).toFloat()
        val dy = -cos(theta).toFloat()
        val length = (abs(imageW * dx) + abs(h * dy)).coerceAtLeast(1f)
        val start = Offset(cx - dx * length / 2f, cy - dy * length / 2f)
        val end = Offset(cx + dx * length / 2f, cy + dy * length / 2f)
        val f = (spread / length).coerceAtMost(0.5f)
        return LinearGradientShader(
            from = start,
            to = end,
            colors = listOf(spec.base, spec.mid, spec.highlight, spec.mid, spec.base),
            colorStops = listOf(0.5f - f, 0.5f - f / 2f, 0.5f, 0.5f + f / 2f, 0.5f + f),
            tileMode = TileMode.Clamp,
        )
    }

    override fun equals(other: Any?): Boolean =
        other is TextShimmerBrush && other.spec == spec && other.position == position && other.density == density

    override fun hashCode(): Int = (spec.hashCode() * 31 + position.hashCode()) * 31 + density.hashCode()
}

/**
 * The moving shimmer brush of a `shimmer` Text inked [ink]; null = paint
 * plain (no `shimmer`, reduced motion, or the core refused the colour).
 * The fill slides from position 100% to 0% over `durationMs`, forever; rtl
 * reverses.
 */
@Composable
internal fun textShimmerBrush(cx: LeafContext, ink: Color = cx.ink): Brush? {
    if (!cx.props.flag("shimmer") || cx.model.reducedMotion) return null
    val dark = cx.dark
    val spec = remember(ink, dark) { TextShimmerSpec.of(ink, dark) } ?: return null
    val transition = rememberInfiniteTransition(label = "shimmer")
    val t by transition.animateFloat(0f, 1f, infiniteRepeatable(tween(spec.durationMs.coerceAtLeast(1), easing = LinearEasing)), label = "shimmer")
    val density = LocalDensity.current.density
    val position = if (cx.rtl) t else 1f - t
    return TextShimmerBrush(spec, position, density)
}
