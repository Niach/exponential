package at.exponential.ui

import android.util.Base64
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.ClosureHost
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream

/**
 * VAPP-103: an AudioPlayer really plays its policed `src` on a device (a
 * `data:` WAV, fetched under `media.limits` into a temporary file, opened by
 * ExoPlayer): the press starts it and the item's own length replaces
 * `0:00`. A denied src keeps the control inert.
 */
@RunWith(AndroidJUnit4::class)
class MediaInstrumentedTest {
    @get:Rule
    val compose = createComposeRule()

    private fun model(tree: String): SurfaceModel {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val density = context.resources.displayMetrics.density
        val fallback = TextShaper(
            TextMeasurer(createFontFamilyResolver(context), Density(density, 1f), LayoutDirection.Ltr),
            density,
        ) { null }
        val m = SurfaceModel(
            id = "media",
            options = SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Light),
            host = ClosureHost(),
            shaper = { fallback },
        )
        m.setNested(tree)
        compose.setContent { ExponentialSurface(m) }
        compose.waitForIdle()
        return m
    }

    /** [seconds] of 8 kHz mono 8-bit silence as a WAV `data:` url. */
    private fun silentWav(seconds: Int): String {
        val rate = 8_000
        val samples = rate * seconds
        val out = ByteArrayOutputStream()
        fun le32(v: Int) = out.write(byteArrayOf(v.toByte(), (v shr 8).toByte(), (v shr 16).toByte(), (v shr 24).toByte()))
        fun le16(v: Int) = out.write(byteArrayOf(v.toByte(), (v shr 8).toByte()))
        out.write("RIFF".toByteArray()); le32(36 + samples); out.write("WAVE".toByteArray())
        out.write("fmt ".toByteArray()); le32(16); le16(1); le16(1); le32(rate); le32(rate); le16(1); le16(8)
        out.write("data".toByteArray()); le32(samples); out.write(ByteArray(samples) { 0x80.toByte() })
        return "data:audio/wav;base64," + Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
    }

    private val buttons = SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Button)

    @Test
    fun anAudioPlayerPlaysItsPolicedSrc() {
        model("""{"id":"root","component":"Stack","style":{"padding":16},"children":[{"id":"a","component":"AudioPlayer","props":{"src":"${silentWav(3)}","title":"Silence"}}]}""")
        compose.onAllNodes(hasText("0:00 / 0:00", substring = true), useUnmergedTree = true).onFirst().assertExists()
        compose.onAllNodes(buttons, useUnmergedTree = true).onFirst().performClick()
        // The player read the item's length: 0:03.
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("/ 0:03", substring = true), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
    }

    @Test
    fun aDeniedSrcStaysInert() {
        model("""{"id":"root","component":"Stack","style":{"padding":16},"children":[{"id":"a","component":"AudioPlayer","props":{"src":"file:///sdcard/a.wav","title":"Local"}}]}""")
        val node = compose.onAllNodes(buttons, useUnmergedTree = true).fetchSemanticsNodes().single()
        assertEquals(true, SemanticsProperties.Disabled in node.config)
    }
}
