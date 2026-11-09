package at.exponential.ui.primitives

import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** The shared avatar fallback rule, the meter clamp and hex parsing. */
class PrimitivesTest {
    /** packages/exponential-ui/fixtures/avatar.json (#EXP-1629), THE avatar rule ×5 renderers. */
    @Suppress("UNCHECKED_CAST")
    private fun avatarFixture(): Map<String, Any?> {
        val file = File("../../exponential-ui/fixtures/avatar.json")
        assumeTrue("$file not in this checkout", file.isFile)
        return MiniJson(file.readText()).parse() as Map<String, Any?>
    }

    @Suppress("UNCHECKED_CAST")
    private fun Any?.pairs(): List<List<Any?>> = this as List<List<Any?>>

    @Test
    fun avatarPaletteIsTheFixtures() {
        val f = avatarFixture()
        assertEquals(f["palette"], AvatarFallback.PALETTE)
        assertEquals((f["fillAlpha"] as Number).toFloat(), AvatarFallback.FILL_ALPHA, 0f)
        assertEquals(parseHexColor("#F87171"), AvatarFallback.palette[0])
    }

    @Test
    fun avatarHuesReplayTheFixture() {
        for ((seed, index) in avatarFixture()["hues"].pairs()) {
            assertEquals("hue of '$seed'", (index as Number).toInt(), AvatarFallback.hueIndex(seed as String))
        }
    }

    @Test
    fun avatarInitialsReplayTheFixture() {
        for ((name, letters) in avatarFixture()["initials"].pairs()) {
            assertEquals("initials of '$name'", letters, AvatarFallback.initials(name as String))
        }
        assertEquals("?", AvatarFallback.initials(null))
    }

    @Test
    fun avatarTintIsTheHueAtTheFillAlpha() {
        val tint = AvatarFallback.tint("demo-mira")
        val hue = AvatarFallback.palette[2]
        assertEquals(hue, tint.ink)
        assertEquals(hue.copy(alpha = 0.2f), tint.fill)
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
