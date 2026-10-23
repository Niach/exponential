package at.exponential.ui

import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.SystemClock
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
import androidx.media3.common.Player
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.MediaFiles
import at.exponential.ui.compose.MediaPlayback
import at.exponential.ui.host.ClosureHost
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * VAPP-103: an AudioPlayer really plays its policed `src` on a device (a
 * `data:` WAV, fetched under `media.limits` into a temporary file, opened by
 * ExoPlayer) and reads the item's own length. The POLICY half (the
 * request, the file the player gets, a READY player) needs no audio and
 * always runs: CI starts the emulator with `-noaudio`. Only playing
 * (isPlaying, the position advancing) assumes an audio output. Every wait
 * is a bounded poll that fails with the last state it saw. A denied src
 * keeps the control inert.
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

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext

    /** A real output to play into (the CI emulator runs with `-noaudio`). */
    private fun hasAudioOutput(): Boolean {
        val real = setOf(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, AudioDeviceInfo.TYPE_WIRED_HEADPHONES, AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_USB_DEVICE)
        return context.getSystemService(AudioManager::class.java).getDevices(AudioManager.GET_DEVICES_OUTPUTS).any { it.type in real }
    }

    /** Read [read] on the main thread every 50 ms until [done], at most [timeoutMs]; else fail with the last value seen. */
    private fun <T> poll(what: String, timeoutMs: Long = 10_000, read: () -> T, done: (T) -> Boolean): T {
        val deadline = SystemClock.uptimeMillis() + timeoutMs
        while (true) {
            val last = runBlocking(Dispatchers.Main) { read() }
            if (done(last)) return last
            if (SystemClock.uptimeMillis() >= deadline) fail("$what within $timeoutMs ms; last seen: $last")
            Thread.sleep(50)
        }
    }

    private fun stateName(state: Int) = when (state) {
        Player.STATE_IDLE -> "IDLE"
        Player.STATE_BUFFERING -> "BUFFERING"
        Player.STATE_READY -> "READY"
        Player.STATE_ENDED -> "ENDED"
        else -> "$state"
    }

    @Test
    fun anAudioPlayerPlaysItsPolicedSrc() {
        val src = silentWav(3)
        val m = model("""{"id":"root","component":"Stack","style":{"padding":16},"children":[{"id":"a","component":"AudioPlayer","props":{"src":"$src","title":"Silence"}}]}""")
        // The policy: a `data:` src is allowed as is and carries no headers; a local file never is.
        val request = m.mediaRequest(src)
        assertNotNull("a data: src is allowed", request)
        request!!
        assertEquals(src, request.url)
        assertEquals(emptyMap<String, String>(), request.headers)
        assertNull(m.mediaRequest("file:///sdcard/a.wav"))
        // The leaf: nothing loaded yet, its play control enabled.
        compose.onAllNodes(hasText("0:00 / 0:00", substring = true), useUnmergedTree = true).onFirst().assertExists()
        val button = compose.onAllNodes(buttons, useUnmergedTree = true).fetchSemanticsNodes().first()
        assertEquals(false, SemanticsProperties.Disabled in button.config)

        // The player gets the POLICED request, written to a MediaFiles file, and prepares (no audio output needed).
        val playback = MediaPlayback(context)
        try {
            runBlocking(Dispatchers.Main) { playback.play(request, police = m::mediaRequest) }
            assertEquals(request, playback.opened)
            val opened = playback.openedUri
            assertNotNull("the player got a uri", opened)
            opened!!
            assertEquals("file", opened.scheme)
            assertEquals(MediaFiles.dir(context).canonicalPath, File(opened.path!!).parentFile!!.canonicalPath)
            poll("the player READY", read = {
                val p = playback.player
                if (p == null) "no player" to null else stateName(p.playbackState) to p.playerError?.errorCodeName
            }) { it.first == "READY" }
            // The item's own length: 0:03.
            val length = poll("the item's length", read = { playback.tick(); playback.durationMs }) { it != null }!!
            assertTrue("length $length ms", length in 2_900L..3_100L)

            // Playing needs a real audio output.
            assumeTrue("no audio output on this emulator", hasAudioOutput())
            poll("the player playing", read = { playback.player?.isPlaying to playback.player?.playerError?.errorCodeName }) { it.first == true }
            val from = poll("a position", read = { playback.tick(); playback.positionMs }) { true }
            poll("the position advancing from $from ms", read = { playback.tick(); playback.positionMs }) { it > from }
        } finally {
            runBlocking(Dispatchers.Main) { playback.stop() }
        }
    }

    @Test
    fun aDeniedSrcStaysInert() {
        model("""{"id":"root","component":"Stack","style":{"padding":16},"children":[{"id":"a","component":"AudioPlayer","props":{"src":"file:///sdcard/a.wav","title":"Local"}}]}""")
        val node = compose.onAllNodes(buttons, useUnmergedTree = true).fetchSemanticsNodes().single()
        assertEquals(true, SemanticsProperties.Disabled in node.config)
    }
}
