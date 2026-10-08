package at.exponential.ui.primitives

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The shared avatar fallback rule, the meter clamp and hex parsing. */
class PrimitivesTest {
    @Test
    fun initialsTakeTheFirstTwoWords() {
        assertEquals("AC", AvatarFallback.initials("Alex Chen"))
        assertEquals("AB", AvatarFallback.initials("  ada   bea  carl "))
        assertEquals("M", AvatarFallback.initials("mono"))
        assertEquals("", AvatarFallback.initials("   "))
    }

    @Test
    fun seedHueMatchesTheReactAndGpuiRule() {
        // h = h * 31 + utf16unit (wrapping UInt32), % 360; computed by hand.
        assertEquals(72.0, AvatarFallback.seedHue("Alex Chen"), 0.0)
        assertEquals(97.0, AvatarFallback.seedHue("a"), 0.0)
        assertEquals(0.0, AvatarFallback.seedHue(""), 0.0)
        assertEquals(257.0, AvatarFallback.seedHue("danny@yourev.at"), 0.0)
        assertEquals(166.0, AvatarFallback.seedHue("Zoë"), 0.0)
    }

    @Test
    fun tintIsTheSharedHsl() {
        val dark = AvatarFallback.tint("Alex Chen", dark = true)
        val light = AvatarFallback.tint("Alex Chen", dark = false)
        assertEquals(0.22f, dark.fill.alpha, 0.001f)
        assertEquals(1f, dark.ink.alpha, 0.001f)
        assertEquals(dark.fill, light.fill)
        // hsl(72, 0.6, 0.72): r = 0.8208, g = 0.888, b = 0.552.
        assertEquals(0.8208f, dark.ink.red, 0.003f)
        assertEquals(0.888f, dark.ink.green, 0.003f)
        assertEquals(0.552f, dark.ink.blue, 0.003f)
    }

    @Test
    fun meterWidthClamps() {
        assertEquals(0f, widthOf(-1f, 100f), 0f)
        assertEquals(50f, widthOf(0.5f, 100f), 0f)
        assertEquals(100f, widthOf(2f, 100f), 0f)
        assertEquals(0f, widthOf(Float.NaN, 100f), 0f)
    }

    @Test
    fun parsesHexColors() {
        assertEquals(Color(0x25 / 255f, 0x63 / 255f, 0xEB / 255f), parseHexColor("#2563eb"))
        assertNull(parseHexColor("2563eb"))
    }
}
