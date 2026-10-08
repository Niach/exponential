package at.exponential.ui.primitives

import androidx.compose.ui.graphics.Color
import kotlin.math.abs

/**
 * `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (what the core's resolved visuals
 * carry) as an sRGB Compose colour. Anything else returns null.
 */
fun parseHexColor(hex: String): Color? = Rgba.fromHex(hex)?.color

/** The RGBA channels (0…1) of a colour, for luminance and HSL math. */
data class Rgba(val r: Double, val g: Double, val b: Double, val a: Double = 1.0) {
    /** The Compose colour. */
    val color: Color get() = Color(r.toFloat(), g.toFloat(), b.toFloat(), a.toFloat())

    /** Relative luminance (sRGB, for dark/light decisions). */
    val luminance: Double get() = 0.2126 * r + 0.7152 * g + 0.0722 * b

    companion object {
        /** Parse `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`; nil otherwise. */
        fun fromHex(hex: String): Rgba? {
            var s = hex.trim()
            if (!s.startsWith("#")) return null
            s = s.substring(1)
            if (s.length == 3 || s.length == 4) s = s.map { "$it$it" }.joinToString("")
            if (s.length != 6 && s.length != 8) return null
            if (!s.all { it.isDigit() || it in 'a'..'f' || it in 'A'..'F' }) return null
            val v = s.toULong(16)
            val a = if (s.length == 8) (v and 0xFFu).toDouble() / 255 else 1.0
            val shift = if (s.length == 8) 8 else 0
            val r = ((v shr (16 + shift)) and 0xFFu).toDouble() / 255
            val g = ((v shr (8 + shift)) and 0xFFu).toDouble() / 255
            val b = ((v shr shift) and 0xFFu).toDouble() / 255
            return Rgba(r, g, b, a)
        }

        /** An HSL colour (hue in degrees), like the React renderer's avatar tint. */
        fun hsl(hue: Double, saturation: Double, lightness: Double, alpha: Double = 1.0): Rgba {
            val h = ((hue % 360) + 360) % 360 / 360
            val s = saturation
            val l = lightness
            val c = (1 - abs(2 * l - 1)) * s
            val x = c * (1 - abs((h * 6) % 2 - 1))
            val m = l - c / 2
            val h6 = h * 6
            val (r1, g1, b1) = when {
                h6 < 1 -> Triple(c, x, 0.0)
                h6 < 2 -> Triple(x, c, 0.0)
                h6 < 3 -> Triple(0.0, c, x)
                h6 < 4 -> Triple(0.0, x, c)
                h6 < 5 -> Triple(x, 0.0, c)
                else -> Triple(c, 0.0, x)
            }
            return Rgba(r1 + m, g1 + m, b1 + m, alpha)
        }
    }
}
