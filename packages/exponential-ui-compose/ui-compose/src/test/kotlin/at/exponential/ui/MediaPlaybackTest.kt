package at.exponential.ui

import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.createComposeRule
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.MediaPlayback
import at.exponential.ui.compose.PlaybackSource
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.MediaRequest
import at.exponential.ui.host.MediaRule
import at.exponential.ui.host.NoHost
import at.exponential.ui.host.policedMediaRequest
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import java.io.File
import java.net.InetAddress
import java.net.ServerSocket
import java.util.concurrent.CopyOnWriteArrayList

/**
 * VAPP-103: Video / AudioPlayer play their `src` ONLY through the media
 * policy. An allowed src opens an ExoPlayer on the request (streamed, or —
 * with headers / a `data:` url — fetched under `media.limits` into a
 * temporary file, the headers sent); a denied or over-limit src opens
 * nothing and the leaf's play control stays inert.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
class MediaPlaybackTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: ServerSocket
    private lateinit var thread: Thread
    private val seen = CopyOnWriteArrayList<String>()
    private val base get() = "http://127.0.0.1:${server.localPort}"

    @Before
    fun serve() {
        // A raw-socket HTTP/1.0 server (com.sun.net.httpserver is not on the Android classpath).
        server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
        thread = Thread {
            while (!server.isClosed) {
                val sock = runCatching { server.accept() }.getOrNull() ?: break
                sock.use { c ->
                    val reader = c.getInputStream().bufferedReader()
                    val lines = generateSequence { reader.readLine()?.takeIf { it.isNotEmpty() } }.toList()
                    seen += lines
                    val path = lines.firstOrNull()?.split(" ")?.getOrNull(1) ?: ""
                    val out = c.getOutputStream()
                    fun head(h: String) = out.write("HTTP/1.0 $h\r\n\r\n".toByteArray())
                    runCatching {
                        when (path) {
                            "/clip.mp4" -> {
                                head("200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 100")
                                out.write(ByteArray(100))
                            }
                            // Claims 30 MB: refused on the header alone.
                            "/big.mp4" -> head("200 OK\r\nContent-Length: ${30L * 1024 * 1024}")
                            else -> head("404 Not Found\r\nContent-Length: 0")
                        }
                        out.flush()
                    }
                }
            }
        }.apply { isDaemon = true; start() }
    }

    @After
    fun close() {
        server.close()
        thread.join(1_000)
    }

    /** A host whose media rule signs every request to the test server. */
    private fun signing() = object : HostPlugin {
        override val mediaOptions = MediaOptions(rules = listOf(MediaRule("$base/", mapOf("Authorization" to "Bearer t0k"))))
    }

    private val context get() = RuntimeEnvironment.getApplication()

    @Test
    fun aRequestStreamsUnlessItCarriesHeadersOrData() {
        assertEquals(PlaybackSource.Stream("https://cdn.example/a.mp4"), PlaybackSource.of(MediaRequest("https://cdn.example/a.mp4")))
        val signed = MediaRequest("https://cdn.example/a.mp4", mapOf("Authorization" to "Bearer x"))
        assertEquals(PlaybackSource.Fetch(signed), PlaybackSource.of(signed))
        val data = MediaRequest("data:audio/wav;base64,UklGRg==")
        assertEquals(PlaybackSource.Fetch(data), PlaybackSource.of(data))
    }

    @Test
    fun anAllowedSrcOpensAPlayerOnTheRequest() = runBlocking {
        // Plain: the url itself goes to the player.
        val plain = MediaPlayback(context)
        val stream = policedMediaRequest(NoHost, "https://cdn.example/clip.mp4")
        plain.play(stream)
        val p = plain.player!!
        assertEquals("https://cdn.example/clip.mp4", p.currentMediaItem?.localConfiguration?.uri.toString())
        plain.stop()
        assertNull(plain.player)

        // Signed: fetched WITH the rule's headers into a file the player opens.
        val host = signing()
        val signed = policedMediaRequest(host, "$base/clip.mp4")!!
        assertEquals("Bearer t0k", signed.headers["Authorization"])
        val playback = MediaPlayback(context)
        playback.play(signed, police = { policedMediaRequest(host, it) })
        val uri = playback.openedUri!!
        assertEquals("file", uri.scheme)
        assertEquals(100L, File(uri.path!!).length())
        assertEquals(uri, playback.player?.currentMediaItem?.localConfiguration?.uri)
        assertTrue("the header reached the server: $seen", seen.any { it.equals("Authorization: Bearer t0k", ignoreCase = true) })
        playback.stop()
    }

    @Test
    fun aDeniedOrOverLimitSrcOpensNothing() = runBlocking {
        val playback = MediaPlayback(context)
        for (src in listOf("file:///sdcard/clip.mp4", "content://media/1", "javascript:alert(1)", "/relative.mp4")) {
            val request = policedMediaRequest(NoHost, src)
            assertNull(src, request)
            playback.play(request)
            assertNull(src, playback.player)
        }
        // Over the byte cap (Content-Length) and over a small cap as it streams.
        val host = signing()
        playback.play(policedMediaRequest(host, "$base/big.mp4"))
        assertNull(playback.player)
        assertNull(playback.openedUri)
        val tiny = MediaLimits(maxBytes = 50, timeoutMs = 5_000, maxPixels = 1)
        playback.play(policedMediaRequest(host, "$base/clip.mp4"), limits = tiny)
        assertNull(playback.player)
        // A 404 too.
        playback.play(policedMediaRequest(host, "$base/missing.mp4"))
        assertNull(playback.player)
    }

    @Test
    fun aDeniedSrcLeavesThePlayControlsInert() {
        Fixtures.require()
        val m = makeModel(host = NoHost)
        m.setNested(
            """{"id":"root","component":"Stack","children":[
              {"id":"ok","component":"Video","props":{"src":"https://cdn.example/a.mp4"}},
              {"id":"bad","component":"Video","props":{"src":"file:///sdcard/a.mp4"}},
              {"id":"song","component":"AudioPlayer","props":{"src":"https://cdn.example/a.mp3","title":"Song"}},
              {"id":"mute","component":"AudioPlayer","props":{"src":"javascript:alert(1)","title":"Mute"}}
            ]}""",
        )
        compose.setContent { ExponentialSurface(m) }
        compose.waitForIdle()
        val buttons = compose.onAllNodes(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Button), useUnmergedTree = true).fetchSemanticsNodes()
        val enabled = buttons.map { SemanticsProperties.Disabled !in it.config }
        // Document order: ok, bad, song, mute.
        assertEquals(listOf(true, false, true, false), enabled)
    }
}
