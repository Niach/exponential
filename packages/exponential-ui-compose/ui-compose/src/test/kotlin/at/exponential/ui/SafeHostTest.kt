package at.exponential.ui

import at.exponential.ui.compose.LeafImages
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.MediaRequest
import at.exponential.ui.host.MemoryTransport
import at.exponential.ui.host.NoHost
import at.exponential.ui.host.PaintError
import at.exponential.ui.host.RENDER_FAILED
import at.exponential.ui.host.UrlPolicy
import at.exponential.ui.host.policedMediaRequest
import at.exponential.ui.host.safeHref
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.SurfaceMeasurer
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.fire
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownBlockKind
import at.exponential.ui.primitives.MarkdownInline
import at.exponential.ui.primitives.MarkdownStyles
import at.exponential.ui.primitives.MarkdownTextMeasure
import at.exponential.ui.primitives.MarkdownTextSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.io.ByteArrayInputStream

/**
 * VAPP-103 (safe hosts): every href through the URL policy, every src
 * through the media policy, the image loader's limits, `onPaintError` →
 * RENDER_FAILED, and the markdown parser's CommonMark destinations, nested
 * lists and block images. Plain JVM.
 */
class SafeHostTest {
    private val core = "https://ui.exponential.at/catalogs/core/v1"

    private fun msg(json: String) = JsonValue.parse(json)

    // Hrefs

    @Test
    fun deniedHrefsNeverNavigate() {
        assertEquals("https://x.example/a", safeHref(NoHost, "https://x.example/a"))
        assertEquals("mailto:a@b.c", safeHref(NoHost, "mailto:a@b.c"))
        assertNull(safeHref(NoHost, "javascript:alert(1)"))
        assertNull(safeHref(NoHost, "JaVaScRiPt:alert(1)"))
        assertNull(safeHref(NoHost, "file:///etc/passwd"))
        assertNull(safeHref(NoHost, "data:text/html,<b>x</b>"))
        assertNull("relative without a base", safeHref(NoHost, "/help"))
        assertNull(safeHref(NoHost, ""))
        val based = object : HostPlugin {
            override val urlPolicy = UrlPolicy(hosts = listOf("*.example.com"))
            override val mediaOptions = MediaOptions(baseUrl = "https://app.example.com/")
        }
        assertEquals("https://app.example.com/help", safeHref(based, "/help"))
        assertNull(safeHref(based, "https://evil.test/"))

        // A Link (and an openUrl function) on a plain HostPlugin: the model polices before the host opens.
        val host = RecordingHost()
        val m = SurfaceModel("links", host = host, scope = CoroutineScope(Dispatchers.Unconfined), shaper = noShaper)
        m.fixedMeasure = true
        m.setViewport(390f, 800f)
        m.setComponents(
            """[{"id":"root","component":"Stack","children":["ok","bad","fn"]},
               {"id":"ok","component":"Link","label":"Docs","href":"https://ui.exponential.at"},
               {"id":"bad","component":"Link","label":"Evil","href":"javascript:alert(1)","external":true},
               {"id":"fn","component":"Button","label":"Go","on":{"press":{"functionCall":{"call":"openUrl","args":{"url":"file:///etc/passwd"}}}}}]""",
        )
        m.fire(m.indexOf("ok")!!, "press")
        m.fire(m.indexOf("bad")!!, "press")
        m.fire(m.indexOf("fn")!!, "press")
        assertEquals(listOf("https://ui.exponential.at/"), host.urls)
        assertNull(m.href("javascript:alert(1)"))
    }

    // Media

    @Test
    fun deniedSrcsLoadNothing() {
        assertEquals(MediaRequest("https://cdn.example/a.png"), policedMediaRequest(NoHost, "https://cdn.example/a.png"))
        assertNotNull(policedMediaRequest(NoHost, "data:image/png;base64,iVBORw0KGgo="))
        assertNull("no local file", policedMediaRequest(NoHost, "file:///etc/passwd"))
        assertNull(policedMediaRequest(NoHost, "content://media/external/images/1"))
        assertNull(policedMediaRequest(NoHost, "javascript:alert(3)"))
        assertNull("relative without a base", policedMediaRequest(NoHost, "/api/attachments/1"))
        // resolveUrl is a rewrite BEFORE the policy, never around it.
        val rewriting = object : HostPlugin {
            override fun resolveUrl(src: String): String = "file:///data/$src"
        }
        assertNull(policedMediaRequest(rewriting, "a.png"))
        // A host that lists `file` and an allow-list of hosts.
        val listed = object : HostPlugin {
            override val mediaOptions = MediaOptions(schemes = listOf("https", "file"), hosts = listOf("cdn.example"))
        }
        assertNotNull(policedMediaRequest(listed, "file:///tmp/a.png"))
        assertNull(policedMediaRequest(listed, "https://other.example/a.png"))
        assertNull("http not listed", policedMediaRequest(listed, "http://cdn.example/a.png"))
        // ExponentialHost without media options: the defaults, never the raw src.
        val h = ExponentialHost(HostOptions(), CoroutineScope(Dispatchers.Unconfined))
        assertNull(h.mediaRequest("file:///etc/passwd"))
        assertEquals("https://cdn.example/a.png", h.mediaRequest("https://cdn.example/a.png")?.url)
        val hosted = ExponentialHost(HostOptions(policy = HostPolicy(media = MediaOptions(hosts = listOf("*.example.com")))), CoroutineScope(Dispatchers.Unconfined))
        assertNull(hosted.mediaRequest("https://evil.test/a.png"))
        assertNotNull(hosted.mediaRequest("https://cdn.example.com/a.png"))
    }

    @Test
    fun mediaLimitsComeFromTheContract() {
        val l = MediaLimits.contract
        assertEquals(20_971_520L, l.maxBytes)
        assertEquals(30_000L, l.timeoutMs)
        assertEquals(33_554_432L, l.maxPixels)
    }

    private fun failure(block: () -> Unit): LeafImages.Failure {
        try {
            block()
        } catch (e: LeafImages.LoadFailure) {
            return e.reason
        }
        fail("expected a LoadFailure")
        error("unreachable")
    }

    @Test
    fun oversizeBodiesAreRefused() {
        // A raw-socket HTTP/1.0 server (com.sun.net.httpserver is not on the Android classpath).
        val server = java.net.ServerSocket(0, 50, java.net.InetAddress.getByName("127.0.0.1"))
        val thread = Thread {
            while (!server.isClosed) {
                val sock = runCatching { server.accept() }.getOrNull() ?: break
                sock.use { c ->
                    val path = c.getInputStream().bufferedReader().readLine()?.split(" ")?.getOrNull(1) ?: ""
                    val out = c.getOutputStream()
                    fun head(lines: String) = out.write("HTTP/1.0 $lines\r\n\r\n".toByteArray())
                    runCatching {
                        when (path) {
                            // Claims 30 MB; the loader must refuse on the header alone.
                            "/big" -> head("200 OK\r\nContent-Length: ${30L * 1024 * 1024}")
                            // No length: refused as the body streams.
                            "/stream" -> {
                                head("200 OK")
                                repeat(64) { out.write(ByteArray(1024)) }
                            }
                            "/small" -> {
                                head("200 OK\r\nContent-Length: 100")
                                out.write(ByteArray(100))
                            }
                            "/redirect" -> head("302 Found\r\nLocation: file:///etc/passwd")
                            else -> head("404 Not Found\r\nContent-Length: 0")
                        }
                        out.flush()
                    }
                }
            }
        }.apply { isDaemon = true; start() }
        try {
            val base = "http://127.0.0.1:${server.localPort}"
            assertEquals(LeafImages.Failure.TooLarge, failure { LeafImages.fetch(MediaRequest("$base/big")) })
            val small = MediaLimits(maxBytes = 16 * 1024, timeoutMs = 5_000, maxPixels = 1_000)
            assertEquals("the body as it streams", LeafImages.Failure.TooLarge, failure { LeafImages.fetch(MediaRequest("$base/stream"), small) })
            assertEquals(100, LeafImages.fetch(MediaRequest("$base/small"), small).size)
            assertEquals(LeafImages.Failure.Http, failure { LeafImages.fetch(MediaRequest("$base/missing"), small) })
            assertEquals("a redirect passes the policy again", LeafImages.Failure.Denied, failure { LeafImages.fetch(MediaRequest("$base/redirect")) { policedMediaRequest(NoHost, it) } })
        } finally {
            server.close()
            thread.join(1_000)
        }
        assertEquals(LeafImages.Failure.Denied, failure { LeafImages.fetch(MediaRequest("file:///etc/passwd")) })
        assertEquals(LeafImages.Failure.TooLarge, failure { LeafImages.readCapped(ByteArrayInputStream(ByteArray(2048)), 1024) })
        assertEquals(LeafImages.Failure.Timeout, failure { LeafImages.readCapped(ByteArrayInputStream(ByteArray(10)), 1024, deadline = System.nanoTime() - 1) })
        assertEquals(LeafImages.Failure.TooManyPixels, failure { LeafImages.checkPixels(40_000, 30_000, MediaLimits.contract) })
        LeafImages.checkPixels(8192, 4096, MediaLimits.contract)
    }

    // onPaintError

    @Test
    fun paintErrorsAreSentOnceAsRenderFailed() {
        val t = MemoryTransport()
        val seen = ArrayList<PaintError>()
        val h = ExponentialHost(
            HostOptions(transport = t, plugin = object : HostPlugin {
                override fun onPaintError(error: PaintError) {
                    seen.add(error)
                }
            }),
            CoroutineScope(Dispatchers.Unconfined),
        )
        h.connect()
        t.feed(msg("""{"version":"v0.9","createSurface":{"surfaceId":"s1","catalogId":"$core"}}"""))
        t.feed(msg("""{"version":"v0.9","updateComponents":{"surfaceId":"s1","components":[{"id":"root","component":"Text","text":"hi"}]}}"""))
        val m = h.surface("s1")!!
        m.paintFailed("chart", "Chart: bad series")
        m.paintFailed("chart", "Chart: bad series")
        assertEquals(1, t.sent.size)
        val err = t.sent.single()["error"]!!
        assertEquals(RENDER_FAILED, err["code"]?.string)
        assertEquals("s1", err["surfaceId"]?.string)
        assertEquals("Chart: bad series", err["message"]?.string)
        assertEquals("/components/chart", err["path"]?.string)
        assertEquals(listOf(PaintError("s1", "chart", "Chart: bad series")), seen)
        assertEquals("Chart: bad series", m.paintFailures["chart"])
        // Straight to the host: deduped there too.
        h.paintError(PaintError("s1", "chart", "Chart: bad series"))
        assertEquals(1, t.sent.size)
        // Another message is another error.
        m.paintFailed("chart", "Chart: other")
        assertEquals(2, t.sent.size)
        // New components clear the dedupe (the model's and the host's).
        t.feed(msg("""{"version":"v0.9","updateComponents":{"surfaceId":"s1","components":[{"id":"root","component":"Text","text":"again"}]}}"""))
        assertTrue(m.paintFailures.isEmpty())
        m.paintFailed("chart", "Chart: bad series")
        assertEquals(3, t.sent.size)
        assertEquals(RENDER_FAILED, t.sent.last()["error"]?.get("code")?.string)
    }

    @Test
    fun selectSkipsNullOptions() {
        val props = mapOf(
            "options" to JsonValue.parse("""[null, {"value":"a","label":"Alpha"}, 3]"""),
            "value" to JsonValue.Str("a"),
        )
        assertEquals("Alpha", SurfaceMeasurer.selectLabel(props))
        assertEquals("Choose", SurfaceMeasurer.selectLabel(mapOf("options" to JsonValue.parse("[null]"))))
        val m = SurfaceModel("sel", host = RecordingHost(), scope = CoroutineScope(Dispatchers.Unconfined), shaper = noShaper)
        m.fixedMeasure = true
        m.setViewport(390f, 800f)
        m.setComponents("""[{"id":"root","component":"Select","name":"s","label":"Pick","options":[null,{"value":"a","label":"Alpha"}]}]""")
        assertTrue(m.nodes.isNotEmpty())
    }

    // Markdown

    private fun inlines(md: String): List<MarkdownInline> = Markdown.parse(md).single().inlines

    @Test
    fun markdownDestinationsBalanceParentheses() {
        val link = inlines("see [a](https://x/A_(b)) now")
        assertEquals("https://x/A_(b)", link.single { it.link != null }.link)
        assertEquals(" now", link.last().text)
        assertEquals("https://x/a(b(c))d", inlines("[a](https://x/a(b(c))d)").single().link)
        assertEquals("an escaped paren", "https://x/a)", inlines("""[a](https://x/a\))""").single().link)
        // Whitespace in a destination: not a link.
        assertTrue(inlines("[a](https://x/a b)").all { it.link == null })
        // Unbalanced: not a link.
        assertTrue(inlines("[a](https://x/a(b)").all { it.link == null })
        // A block image whose src the media policy then denies.
        val img = Markdown.parse("![i](javascript:alert(3))").single().kind
        assertEquals(MarkdownBlockKind.Image("javascript:alert(3)", "i"), img)
        assertNull(policedMediaRequest(NoHost, (img as MarkdownBlockKind.Image).src))
        // Denied: a paragraph of the alt text (no box), dropped without an alt (×4).
        val denied = Markdown.resolveImages(Markdown.parse("![i](javascript:alert(3))\n\n![](javascript:x)\n\n![ok](https://x/y.png)")) { policedMediaRequest(NoHost, it) != null }
        assertEquals(listOf(MarkdownBlockKind.Paragraph, MarkdownBlockKind.Image("https://x/y.png", "ok")), denied.map { it.kind })
        assertEquals("i", Markdown.plain(denied[0].inlines))
        // Links through a policy: a denied href is plain text (same text, no link).
        val policed = Markdown.policeLinks(Markdown.parse("[ok](https://a.example) [bad](javascript:alert(1))")) { safeHref(NoHost, it) }
        assertEquals(listOf("https://a.example/", null), policed.single().inlines.filter { it.text == "ok" || it.text == "bad" }.map { it.link })
    }

    @Test
    fun markdownImages() {
        val blocks = Markdown.parse("Intro\n\n![A chart](https://cdn.example/c.png)\n\n![](https://cdn.example/d.png)\n\ntext ![inline](https://x/y.png) here")
        assertEquals(MarkdownBlockKind.Image("https://cdn.example/c.png", "A chart"), blocks[1].kind)
        assertEquals("A chart", Markdown.plain(blocks[1].inlines))
        assertEquals("an empty alt", MarkdownBlockKind.Image("https://cdn.example/d.png", ""), blocks[2].kind)
        // An inline image inside text stays its alt text.
        assertEquals(MarkdownBlockKind.Paragraph, blocks[3].kind)
        assertEquals("text inline here", Markdown.plain(blocks[3].inlines))
    }

    @Test
    fun markdownNestedLists() {
        val md = "- a\n  - b\n    1. c\n    2. [x] d\n  - e\n- f\n   - g\n - h"
        val items = Markdown.parse(md).map { it.kind as MarkdownBlockKind.ListItem }
        assertEquals(listOf(0, 1, 2, 2, 1, 0, 1, 0), items.map { it.depth })
        assertEquals(listOf("•", "•", "1.", "2.", "•", "•", "•", "•"), items.map { it.marker })
        assertEquals(true, items[3].task)
        // One indent column more is a sibling, not a child.
        assertEquals(listOf(0, 0), Markdown.parse("- a\n - b").map { (it.kind as MarkdownBlockKind.ListItem).depth })
        // Measure = paint: nested items are inset one listIndent per level; images are imageHeight tall.
        val s = MarkdownStyles(MarkdownTextSpec(14f, 20f, 400, null))
        val measure = object : MarkdownTextMeasure {
            override fun height(inlines: List<MarkdownInline>, spec: MarkdownTextSpec, width: Float?): Float =
                spec.lineHeight * if (width == null) 1 else kotlin.math.ceil(Markdown.plain(inlines).length * 8f / width).toInt().coerceAtLeast(1)
            override fun width(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = Markdown.plain(inlines).length * 8f
            override fun widestWord(inlines: List<MarkdownInline>, spec: MarkdownTextSpec): Float = 8f
        }
        val nested = Markdown.parse("- abcdefgh\n    - abcdefgh")
        assertEquals(64f + s.listInset(0), Markdown.maxContentWidth(nested.take(1), s, measure))
        assertEquals(64f + s.listInset(1), Markdown.maxContentWidth(nested, s, measure))
        assertEquals(s.listIndent * 2, s.listInset(1))
        // At a width that fits depth 0 on one line but not depth 1.
        val w = 64f + s.listInset(0)
        val layout = Markdown.layout(nested, s, w, measure)
        assertEquals(20f, layout.blocks[0].height)
        assertEquals(40f, layout.blocks[1].height)
        val withImage = Markdown.layout(Markdown.parse("![a](https://x/a.png)"), s, 300f, measure)
        assertEquals(s.imageHeight, withImage.height)
    }
}
