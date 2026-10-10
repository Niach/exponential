package at.exponential.ui

import android.net.Uri
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.onAllNodesWithText
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.TransferListener
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.createComposeRule
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.LeafImages
import at.exponential.ui.compose.MediaFiles
import at.exponential.ui.compose.MediaPlayback
import at.exponential.ui.compose.PinnedDataSource
import at.exponential.ui.compose.markdownImageAlignment
import at.exponential.ui.compose.PlaybackSource
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.MediaRequest
import at.exponential.ui.host.MediaRule
import at.exponential.ui.host.NoHost
import at.exponential.ui.host.policedMediaRequest
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
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
 * policy. An allowed http(s) src STREAMS into an ExoPlayer with its headers
 * after a probe resolved its redirects (each hop re-policed, its headers
 * rebuilt, https → http refused); a `data:` src plays from a refcounted
 * temporary file; one open runs per playback; a denied src opens nothing
 * and the leaf's play control stays inert.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
class MediaPlaybackTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: ServerSocket
    private lateinit var thread: Thread

    /** Every request the server saw: its path and header lines. */
    private val requests = CopyOnWriteArrayList<Pair<String, List<String>>>()
    private val base get() = "http://127.0.0.1:${server.localPort}"

    @Before
    fun serve() {
        MediaFiles.reset()
        // A raw-socket HTTP/1.0 server (com.sun.net.httpserver is not on the Android classpath).
        server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
        thread = Thread {
            while (!server.isClosed) {
                val sock = runCatching { server.accept() }.getOrNull() ?: break
                sock.use { c ->
                    val reader = c.getInputStream().bufferedReader()
                    val lines = generateSequence { reader.readLine()?.takeIf { it.isNotEmpty() } }.toList()
                    val path = lines.firstOrNull()?.split(" ")?.getOrNull(1) ?: ""
                    requests += path to lines.drop(1)
                    val out = c.getOutputStream()
                    fun head(h: String) = out.write("HTTP/1.0 $h\r\n\r\n".toByteArray())
                    runCatching {
                        when (path) {
                            "/clip.mp4", "/signed/clip.mp4", "/open/clip.mp4", "/other.mp4" -> {
                                head("200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 100")
                                out.write(ByteArray(100))
                            }
                            // Claims 30 MB: over the image cap, yet a stream plays it.
                            "/big.mp4" -> head("200 OK\r\nContent-Length: ${30L * 1024 * 1024}")
                            // Leaves the signing rule's prefix: the next hop carries no Authorization.
                            "/signed/hop" -> head("302 Found\r\nLocation: /open/clip.mp4\r\nContent-Length: 0")
                            // Leaves the hosts policy.
                            "/signed/evil" -> head("302 Found\r\nLocation: http://evil.example/x.mp4\r\nContent-Length: 0")
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

    /** A host whose media rule signs requests under `/signed/` and that loads only from the test server. */
    private fun signing() = object : HostPlugin {
        override val mediaOptions = MediaOptions(
            rules = listOf(MediaRule("$base/signed/", mapOf("Authorization" to "Bearer t0k"))),
            hosts = listOf("127.0.0.1"),
        )
    }

    private val context get() = RuntimeEnvironment.getApplication()

    private fun headersOf(path: String) = requests.filter { it.first == path }.flatMap { it.second }

    private fun authorized(path: String) = headersOf(path).any { it.equals("Authorization: Bearer t0k", ignoreCase = true) }

    /** A `data:` video of [n] bytes. */
    private fun dataClip(n: Int, seed: Int = 0) =
        MediaRequest("data:video/mp4;base64," + java.util.Base64.getEncoder().encodeToString(ByteArray(n) { (it + seed).toByte() }))

    @Test
    fun anHttpRequestStreamsAndDataGoesToAFile() {
        assertEquals(PlaybackSource.Stream("https://cdn.example/a.mp4"), PlaybackSource.of(MediaRequest("https://cdn.example/a.mp4")))
        val signed = MediaRequest("https://cdn.example/a.mp4", mapOf("Authorization" to "Bearer x"))
        assertEquals(PlaybackSource.Stream(signed.url, signed.headers), PlaybackSource.of(signed))
        val data = MediaRequest("data:audio/wav;base64,UklGRg==")
        assertEquals(PlaybackSource.File(data), PlaybackSource.of(data))
        assertNull(PlaybackSource.of(MediaRequest("file:///sdcard/a.mp4")))
    }

    @Test
    fun aSignedSrcStreamsWithItsHeadersAndNoFile() = runBlocking {
        val host = signing()
        val signed = policedMediaRequest(host, "$base/signed/clip.mp4")!!
        assertEquals("Bearer t0k", signed.headers["Authorization"])
        val playback = MediaPlayback(context)
        playback.play(signed, police = { policedMediaRequest(host, it) })
        assertEquals(PlaybackSource.Stream(signed.url, signed.headers), playback.openedStream)
        val uri = playback.openedUri!!
        assertEquals("http", uri.scheme)
        assertEquals(uri, playback.player?.currentMediaItem?.localConfiguration?.uri)
        assertTrue(MediaFiles.tracked().isEmpty())
        // The probe carried the header and asked for one byte.
        assertTrue(authorized("/signed/clip.mp4"))
        assertTrue(headersOf("/signed/clip.mp4").any { it.equals("Range: bytes=0-0", ignoreCase = true) })
        playback.stop()
        // Over the image byte cap: a stream still opens (catalog/host.json: no byte limit for Video / Audio).
        val big = MediaPlayback(context)
        big.play(policedMediaRequest(host, "$base/big.mp4"), police = { policedMediaRequest(host, it) })
        assertNotNull(big.player)
        big.stop()
    }

    @Test
    fun aRedirectChainIsPolicedPerHop() = runBlocking {
        val host = signing()
        val police = { u: String -> policedMediaRequest(host, u) }
        // The hop leaves the rule's prefix: the final request carries NO rule header.
        val hop = PlaybackSource.resolve(police("$base/signed/hop")!!, police = police)
        assertEquals(PlaybackSource.Stream("$base/open/clip.mp4"), hop)
        assertTrue(authorized("/signed/hop"))
        assertFalse(authorized("/open/clip.mp4"))
        // The image / file fetch follows the same rule.
        requests.clear()
        assertEquals(100, LeafImages.fetch(police("$base/signed/hop")!!, police = police).size)
        assertFalse(authorized("/open/clip.mp4"))
        // A hop to a host the policy denies: nothing opens.
        assertNull(PlaybackSource.resolve(police("$base/signed/evil")!!, police = police))
        val denied = runCatching { LeafImages.fetch(police("$base/signed/evil")!!, police = police) }.exceptionOrNull()
        assertEquals(LeafImages.Failure.Denied, (denied as LeafImages.LoadFailure).reason)
        val playback = MediaPlayback(context)
        playback.play(police("$base/signed/evil"), police = police)
        assertNull(playback.player)
    }

    @Test
    fun anHttpsToHttpHopIsRefused() {
        val any = { u: String -> MediaRequest(u) }
        fun reason(block: () -> Unit) = (runCatching(block).exceptionOrNull() as? LeafImages.LoadFailure)?.reason
        val secure = MediaRequest("https://a.example/v.mp4", mapOf("Authorization" to "x"))
        assertEquals(LeafImages.Failure.Denied, reason { LeafImages.redirectHop(secure, "http://a.example/v.mp4", any) })
        // The policy's rewrite of the target may not downgrade it either.
        assertEquals(LeafImages.Failure.Denied, reason { LeafImages.redirectHop(secure, "/w.mp4") { MediaRequest(it.replace("https:", "http:")) } })
        assertEquals(LeafImages.Failure.Denied, reason { LeafImages.redirectHop(secure, "https://b.example/v.mp4") { null } })
        assertEquals(MediaRequest("https://a.example/w.mp4"), LeafImages.redirectHop(secure, "/w.mp4", any))
        assertEquals(MediaRequest("https://a.example/up.mp4"), LeafImages.redirectHop(MediaRequest("http://a.example/v"), "https://a.example/up.mp4", any))
    }

    @Test
    fun dataPlaysFromAFileWithoutTheByteCap() = runBlocking {
        val tiny = MediaLimits(maxBytes = 50, timeoutMs = 5_000, maxPixels = 1)
        val playback = MediaPlayback(context)
        playback.play(dataClip(300), limits = tiny)
        val uri = playback.openedUri!!
        assertEquals("file", uri.scheme)
        val file = File(uri.path!!)
        assertEquals(300L, file.length())
        assertNull(playback.openedStream)
        playback.stop()
        assertFalse("the last holder deletes the file", file.exists())
    }

    @Test
    fun mediaFilesAreRefcountedBoundedAndSwept() = runBlocking {
        val dir = MediaFiles.dir(context).apply { mkdirs() }
        val stale = File(dir, "stale.media").apply { writeText("left over") }
        val clip = dataClip(64)
        val a = MediaPlayback(context)
        val b = MediaPlayback(context)
        a.play(clip)
        assertFalse("the first use empties the directory", stale.exists())
        b.play(clip)
        val file = File(a.openedUri!!.path!!)
        assertEquals(file.path, b.openedUri!!.path)
        a.stop()
        assertTrue("another playback still holds it", file.exists())
        b.stop()
        assertFalse(file.exists())
        // At most CAPACITY files: the oldest goes.
        val held = (0..MediaFiles.CAPACITY).map { MediaFiles.acquire(context, dataClip(16, seed = it))!! }
        assertEquals(MediaFiles.CAPACITY, MediaFiles.tracked().size)
        assertFalse(held.first().exists())
        assertTrue(held.last().exists())
        held.forEach(MediaFiles::release)
        assertTrue(MediaFiles.tracked().isEmpty())
        assertTrue(held.none { it.exists() })
    }

    @Test
    fun oneOpenAtATime() = runBlocking {
        val host = signing()
        val police = { u: String -> policedMediaRequest(host, u) }
        val clip = police("$base/clip.mp4")!!
        // Two presses while it opens: ONE probe, ONE player.
        val playback = MediaPlayback(context)
        val first = async { playback.play(clip, police = police) }
        yield()
        assertTrue(playback.loading)
        val second = async { playback.play(clip, police = police) }
        first.await()
        second.await()
        assertFalse(playback.loading)
        assertEquals(1, playback.playersBuilt)
        assertEquals(1, requests.count { r -> r.first == "/clip.mp4" && r.second.any { it.equals("Range: bytes=0-0", ignoreCase = true) } })
        assertNotNull(playback.player)
        // Another src replaces the player and releases the old one.
        playback.play(police("$base/other.mp4"), police = police)
        assertEquals(2, playback.playersBuilt)
        assertEquals(1, playback.playersReleased)
        assertEquals("$base/other.mp4", playback.openedStream?.url)
        playback.stop()
        assertEquals(2, playback.playersReleased)
        // Another src while one opens cancels the open: only the second builds.
        val switching = MediaPlayback(context)
        val a = async { switching.play(clip, police = police) }
        yield()
        val b = async { switching.play(police("$base/other.mp4"), police = police) }
        a.await()
        b.await()
        assertEquals(1, switching.playersBuilt)
        assertEquals("$base/other.mp4", switching.openedStream?.url)
        switching.stop()
    }

    @Test
    fun aDeniedOrMissingSrcOpensNothing() = runBlocking {
        val playback = MediaPlayback(context)
        for (src in listOf("file:///sdcard/clip.mp4", "content://media/1", "javascript:alert(1)", "/relative.mp4")) {
            val request = policedMediaRequest(NoHost, src)
            assertNull(src, request)
            playback.play(request)
            assertNull(src, playback.player)
        }
        // A 404 (the probe fails).
        playback.play(policedMediaRequest(signing(), "$base/missing.mp4"))
        assertNull(playback.player)
        assertNull(playback.openedUri)
    }

    @Test
    fun aPinnedStreamRefusesToMoveOrigin() {
        val origin = Uri.parse("$base/clip.mp4")
        class Moving(val to: Uri) : DataSource {
            var closed = false
            override fun addTransferListener(transferListener: TransferListener) {}
            override fun open(dataSpec: DataSpec): Long = 100
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int = -1
            override fun getUri(): Uri = to
            override fun close() {
                closed = true
            }
        }
        val same = Moving(Uri.parse("$base/clip.mp4?part=2"))
        assertEquals(100L, PinnedDataSource(same, origin).open(DataSpec(origin)))
        val away = Moving(Uri.parse("http://evil.example/clip.mp4"))
        assertTrue(runCatching { PinnedDataSource(away, origin).open(DataSpec(origin)) }.exceptionOrNull() is java.io.IOException)
        assertTrue(away.closed)
        assertFalse(PinnedDataSource.sameOrigin(Uri.parse("https://a.example/x"), Uri.parse("http://a.example/x")))
        assertTrue(PinnedDataSource.sameOrigin(Uri.parse("https://a.example:443/x"), Uri.parse("https://A.example/y")))
    }

    @Test
    fun anImageWithoutAltPaintsNoLabel() {
        Fixtures.require()
        val m = makeModel(host = NoHost)
        m.setNested(
            """{"id":"root","component":"Stack","children":[
              {"id":"bare","component":"Image","props":{"src":"file:///denied.png"}},
              {"id":"named","component":"Image","props":{"src":"file:///denied.png","alt":"Cat"}}
            ]}""",
        )
        compose.setContent { ExponentialSurface(m) }
        compose.waitForIdle()
        compose.onAllNodesWithText("image", ignoreCase = true, useUnmergedTree = true).assertCountEquals(0)
        compose.onAllNodesWithText("Cat", useUnmergedTree = true).assertCountEquals(1)
    }

    @Test
    fun markdownBlockImagesAlignTop() {
        // Swift and React align a fitted block image to the top of its box.
        assertEquals(Alignment.TopStart, markdownImageAlignment(rtl = false))
        assertEquals(Alignment.TopEnd, markdownImageAlignment(rtl = true))
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
