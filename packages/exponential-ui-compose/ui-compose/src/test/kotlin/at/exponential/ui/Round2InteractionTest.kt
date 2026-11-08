package at.exponential.ui

import at.exponential.ui.compose.handleKey
import at.exponential.ui.format.IcuFormatter
import at.exponential.ui.host.FilePickRequest
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceEnvironment
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.SurfaceSettings
import at.exponential.ui.model.command
import at.exponential.ui.model.focus
import at.exponential.ui.model.press
import at.exponential.ui.model.resizeDrag
import at.exponential.ui.model.scrollTo
import at.exponential.ui.model.scrollToIndex
import at.exponential.ui.model.isSwipeableTabContent
import at.exponential.ui.model.setOpen
import at.exponential.ui.model.swipeBlockedAt
import at.exponential.ui.model.tabSwipe
import at.exponential.ui.model.toastHold
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.TransformOp
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlin.math.abs

/**
 * Round 1 + 2 behaviour on the model (the core's fixed measure): settings
 * and the ICU formatter, built-in strings, events (focus, announce, copy,
 * pick files), toast timers, Resizable drags and keys, scroll containers,
 * sticky pins, `scrollToIndex`, the hardware keyboard and the animation
 * visuals. Robolectric for `android.icu`.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class Round2InteractionTest {
    @Before
    fun setUp() = Fixtures.require()

    private class Host : HostPlugin {
        val copied = ArrayList<String>()
        val picks = ArrayList<FilePickRequest>()
        val actions = ArrayList<String>()
        override fun copy(text: String) { copied.add(text) }
        override fun pickFiles(request: FilePickRequest) { picks.add(request) }
        override fun onAction(event: at.exponential.ui.host.SurfaceActionEvent) { actions.add("${event.componentId}:${event.event}") }
    }

    private fun model(tree: String, settings: SurfaceSettings = SurfaceSettings(), host: HostPlugin = Host(), scope: CoroutineScope = CoroutineScope(Dispatchers.Unconfined), width: Float = 390f, height: Float = 800f): SurfaceModel {
        val m = SurfaceModel("r2", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light, settings = settings), host, scope, noShaper)
        m.fixedMeasure = true
        m.setNested(tree)
        m.setViewport(width, height)
        return m
    }

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    @Test
    fun theFormatterFollowsTheLocaleAndTheStringsTheOverrides() {
        val table = """{"id":"t","component":"Table","props":{"columns":[{"key":"n","label":"N","type":"number"},{"key":"p","label":"P","type":"percent","decimals":1}],"rows":[{"id":"a","n":1234.5,"p":0.256}]}}"""
        val en = model(table, SurfaceSettings(locale = "en-US", timeZone = "UTC"))
        assertEquals("1,234.5", en.node(en.idx("t.cell.a.0"))!!.props["text"]?.string)
        assertEquals("25.6%", en.node(en.idx("t.cell.a.1"))!!.props["text"]?.string)
        val de = model(table, SurfaceSettings(locale = "de-DE", timeZone = "UTC"))
        assertEquals("1.234,5", de.node(de.idx("t.cell.a.0"))!!.props["text"]?.string)
        // The environment's locale applies when the host sets none.
        val env = model(table)
        env.setEnvironment(SurfaceEnvironment(locale = "fr-FR", timeZone = "UTC"))
        assertEquals("1 234,5", env.node(env.idx("t.cell.a.0"))!!.props["text"]?.string?.replace(' ', ' ')?.replace(' ', ' '))
        // Built-in strings and the host's overrides.
        val s = model("""{"id":"x","component":"Text","props":{"text":"x"}}""", SurfaceSettings(strings = mapOf("loading" to "Laden")))
        assertEquals("Laden", s.string("loading"))
        assertEquals("Page 2 of 5", s.string("pageOf", mapOf("page" to 2, "total" to 5)))
        assertNotNull(IcuFormatter.create("ar-EG", "UTC"))
    }

    @Test
    fun resizableDragsFromTheStartSizesAndKeysStep() {
        val tree = """{"id":"root","component":"Box","children":[{"id":"split","component":"Resizable","style":{"height":200},"children":[{"id":"a","component":"Text","props":{"text":"A"}},{"id":"b","component":"Text","props":{"text":"B"}}]}]}"""
        val m = model(tree)
        val handle = m.idx("split.handle.0")
        val panel = m.idx("split.panel.0")
        val before = m.frame(panel).width
        assertEquals(195f, before, 1f)
        m.resizeDrag(handle, "start", 0f)
        m.resizeDrag(handle, "move", 39f)
        m.resizeDrag(handle, "move", 78f)
        m.resizeDrag(handle, "end", 78f)
        val after = m.frame(m.idx("split.panel.0")).width
        assertEquals(before + 78f, after, 1.5f)
        // Keys: ArrowLeft moves the handle back by 10 % (resizeStep).
        m.focus("split.handle.0", true)
        assertTrue(handleKey(m, "left"))
        val keyed = m.frame(m.idx("split.panel.0")).width
        assertEquals(after - 39f, keyed, 1.5f)
    }

    @Test
    fun copyFocusAnnounceAndFilePicksReachTheHost() {
        val host = Host()
        val m = model("""{"id":"root","component":"Box","children":[{"id":"code","component":"CodeBlock","props":{"code":"let x = 1","language":"ts"}},{"id":"up","component":"FileUpload","props":{"label":"Files","name":"f"}},{"id":"in","component":"Input","props":{"label":"Name"}}]}""", host = host)
        val spoken = ArrayList<String>()
        m.announcer = { text, _ -> spoken.add(text) }
        m.press(m.idx("code.copy"))
        assertEquals(listOf("let x = 1"), host.copied)
        m.press(m.idx("up.dropzone"))
        assertEquals(1, host.picks.size)
        host.picks[0].done(listOf(at.exponential.ui.host.PickedFile("a.png", 1200, "image/png")))
        assertTrue(m.nodes.any { it.id.startsWith("up.file") })
        m.command("""{"focus":{"id":"in"}}""")
        assertNotNull(m.focusRequest)
        m.command("""{"announce":{"text":"Saved","live":"polite"}}""")
        assertEquals("Saved", spoken.last())
    }

    @Test
    fun anAnsiCodeBlockCopiesTheStrippedText() {
        // EXP-1324: the `copy` part's `text` (escapes stripped) wins over the raw `code`.
        val host = Host()
        val code = "\u001b[32mok\u001b[0m done"
        val m = model(JsonValue.Obj(mapOf("id" to JsonValue.Str("code"), "component" to JsonValue.Str("CodeBlock"), "props" to JsonValue.Obj(mapOf("code" to JsonValue.Str(code), "language" to JsonValue.Str("ansi"))))).json, host = host)
        val copyText = m.node(m.idx("code.copy"))?.props?.get("text")?.string
        m.press(m.idx("code.copy"))
        assertEquals(listOf(copyText ?: code), host.copied)
        assertFalse(host.copied[0].contains('\u001b'))
    }

    @Test
    fun aTraceItemPaintsItsRailPendingLineAndChevron() {
        val tree = """{"id":"t","component":"Trace","children":[
            {"id":"a","component":"TraceItem","props":{"title":"Ran bun test","icon":"trace-terminal","status":"pending","collapsible":true,"open":true},"children":[{"id":"a-out","component":"Text","props":{"text":"ok"}}]},
            {"id":"b","component":"TraceItem","props":{"title":"Editing","icon":"trace-edit","status":"running","collapsible":true,"open":false}}]}"""
        val m = model(tree)
        assertEquals(m.issues.toString(), 0, m.issues.size)
        // The rail: a left border only, in the theme's border colour.
        val rail = m.boxStyle(m.idx("a.content"))
        assertNotNull(rail.borderColor)
        assertTrue("left-only ${rail.sides}", rail.sides[3] > 0f && rail.sides[0] == 0f && rail.sides[1] == 0f && rail.sides[2] == 0f)
        // A pending line dims to 0.6; a running one does not.
        assertEquals(0.6f, m.boxStyle(m.idx("a.line")).opacity ?: 1f, 0.001f)
        assertEquals(1f, m.boxStyle(m.idx("b.line")).opacity ?: 1f, 0.001f)
        // The chevron turns 90deg while open (the recipe's transform).
        assertEquals(listOf(at.exponential.ui.theme.TransformOp.Rotate(90f)), m.boxStyle(m.idx("a.line.chevron")).transform)
        assertTrue(m.boxStyle(m.idx("b.line.chevron")).transform.isEmpty())
        // A running title shimmers; the closed item has no content.
        assertEquals(true, m.node(m.idx("b.line.title"))?.props?.get("shimmer")?.bool)
        assertEquals(null, m.indexOf("b.content")?.takeIf { m.frame(it).height > 0f })
    }

    @Test
    fun theTextShimmerFixtureReplaysThroughTheFacade() {
        val cases = Round2Suites.textShimmer()
        assertTrue(cases.size >= 4)
        assertEquals(emptyList<String>(), cases.filter { it.second != null }.map { "${it.first}: ${it.second}" })
        // The painter's parse of the core's answer.
        val spec = at.exponential.ui.compose.TextShimmerSpec.of(androidx.compose.ui.graphics.Color(0xFFA1A1A1), dark = true)
        assertNotNull(spec)
        assertEquals("#a1a1a1ff", at.exponential.ui.compose.TextShimmerSpec.hex(spec!!.base))
        assertEquals("#ffffffff", at.exponential.ui.compose.TextShimmerSpec.hex(spec.highlight))
        assertEquals(2000, spec.durationMs)
    }

    @Test
    fun theTextClustersFixtureReplays() {
        val cases = Round2Suites.textClusters()
        assertTrue(cases.size >= 4)
        assertEquals(emptyList<String>(), cases.filter { it.second != null }.map { "${it.first}: ${it.second}" })
    }

    @Test
    fun theVoiceWaveFixtureReplaysThroughTheFacade() {
        val cases = Round2Suites.voiceWave()
        assertTrue(cases.size >= 7)
        assertEquals(emptyList<String>(), cases.filter { it.second != null }.map { "${it.first}: ${it.second}" })
    }

    /** EXP-1586: every fixtures/zoom.json group through the painter's request builder and `zoomJson`. */
    @Test
    fun theZoomFixtureReplaysThroughTheFacade() {
        val cases = Round2Suites.zoom()
        val groups = cases.map { it.first.substringBefore(' ') }.toSet()
        assertEquals(
            setOf("rest", "clamp", "zoomTo", "wheel", "scroll", "ladder", "steps", "doubleTap", "keys", "rubberBand", "drag", "pinch", "momentum", "momentumActive", "panRoom", "resize", "percent", "menu", "bounds"),
            groups,
        )
        assertTrue(cases.size >= 90)
        assertEquals(emptyList<String>(), cases.filter { it.second != null }.map { "${it.first}: ${it.second}" })
    }

    /** FEED-200: fixtures/swipe.json's lock, offset, settle and blocks groups through the facade. */
    @Test
    fun theSwipeFixtureReplaysThroughTheFacade() {
        val cases = Round2Suites.swipe()
        assertEquals(setOf("lock", "offset", "settle", "blocks"), cases.map { it.first.substringBefore(' ') }.toSet())
        assertTrue(cases.size >= 30)
        assertEquals(emptyList<String>(), cases.filter { it.second != null }.map { "${it.first}: ${it.second}" })
    }

    private fun swipeTabs(swipeable: Boolean = true, panel: String = """{"id":"panel_b","component":"Text","props":{"text":"B"}}"""): String =
        """{"id":"root","component":"Box","children":[{"id":"swipe_tabs","component":"Tabs","style":{"width":360},"props":{"swipeable":$swipeable,"value":{"@path":"/tab"},"tabs":[{"label":"One","value":"a"},{"label":"Two","value":"b"},{"label":"Three","value":"c"}]},
           "on":{"change":{"event":{"name":"tab"}}},
           "children":[{"id":"panel_a","component":"Text","props":{"text":"A"}},$panel,{"id":"panel_c","component":"Text","props":{"text":"C"}}]}]}"""

    private fun SurfaceModel.tabContent(owner: String): Int = nodes.first { it.owner == owner && it.part == "content" }.index

    private fun SurfaceModel.translateX(id: String): Float =
        style(idx(id)).transform.filterIsInstance<TransformOp.Translate>().sumOf { it.x.toDouble() }.toFloat()

    /** FEED-200: swipe events on a swipeable Tabs' `content` move the panels, settle through the core and fire `change`. */
    @Test
    fun aSwipeableTabsPagesBySwipeEvents() {
        val host = RecordingHost()
        val m = model(swipeTabs(), host = host)
        m.setData("/tab", JsonValue.Str("a"))
        val content = m.tabContent("swipe_tabs")
        assertTrue(m.isSwipeableTabContent(content))
        val width = m.frame(content).width
        assertTrue("the content is measured", width > 0f)

        // The finger drags: the active panel follows it, nothing changes yet.
        m.tabSwipe(content, "start", -10f)
        m.tabSwipe(content, "move", -60f)
        assertEquals(-60f, m.translateX("panel_a"), 0.5f)
        assertTrue(host.actions.isEmpty())

        // Let go: a cancel snaps back, no change.
        m.tabSwipe(content, "cancel", -60f)
        assertEquals(0f, m.translateX("panel_a"), 0.5f)
        assertTrue(host.actions.isEmpty())

        // A slow drag short of half a page stays.
        m.tabSwipe(content, "start", -20f)
        m.tabSwipe(content, "end", -20f, -0.1f)
        assertTrue(host.actions.isEmpty())
        assertEquals("a", m.data["tab"]?.string)

        // A leftwards fling pages forward: `change {value}` and the bound value.
        m.tabSwipe(content, "start", -12f)
        m.tabSwipe(content, "move", -40f)
        m.tabSwipe(content, "end", -40f, -0.8f)
        assertEquals(listOf("tab"), host.actions.map { it.name })
        assertEquals("b", (host.actions[0].payload ?: host.actions[0].context)["value"]?.string)
        assertEquals("b", m.data["tab"]?.string)

        // A slow drag past half a page to the right pages back.
        m.tabSwipe(content, "start", 10f)
        m.tabSwipe(content, "end", width * 0.75f, 0f)
        assertEquals(listOf("tab", "tab"), host.actions.map { it.name })
        assertEquals("a", m.data["tab"]?.string)

        // Never past the first page.
        m.tabSwipe(content, "start", 10f)
        m.tabSwipe(content, "end", width * 0.9f, 1.2f)
        assertEquals(2, host.actions.size)
    }

    /** FEED-200: a Tabs without `swipeable` ignores swipes (no pager, no gesture). */
    @Test
    fun aPlainTabsIgnoresSwipes() {
        val host = RecordingHost()
        val m = model(swipeTabs(swipeable = false), host = host)
        m.setData("/tab", JsonValue.Str("a"))
        val content = m.tabContent("swipe_tabs")
        assertFalse(m.isSwipeableTabContent(content))
        m.tabSwipe(content, "start", -40f)
        m.tabSwipe(content, "end", -40f, -1f)
        assertTrue(host.actions.isEmpty())
        assertEquals("a", m.data["tab"]?.string)
    }

    /** FEED-200: a touch starting inside a sideways control (a Slider) is that control's; text is the pager's. */
    @Test
    fun aSwipeStartingOnASliderIsTheSlidersOwn() {
        val panel = """{"id":"panel_b","component":"Box","children":[{"id":"panel_b_text","component":"Text","props":{"text":"B"}}]}"""
        val first = """{"id":"panel_a","component":"Stack","props":{"direction":"vertical"},"children":[{"id":"volume","component":"Slider","props":{"label":"Volume","value":{"@path":"/volume"},"min":0,"max":10}},{"id":"panel_a_text","component":"Text","props":{"text":"A"}}]}"""
        val m = model(swipeTabs(panel = panel).replace("""{"id":"panel_a","component":"Text","props":{"text":"A"}}""", first))
        m.setData("/tab", JsonValue.Str("a"))
        val content = m.tabContent("swipe_tabs")
        val origin = m.frame(content).topLeft
        fun at(id: String) = m.frame(m.idx(id)).center.let { it - origin }
        val slider = at("volume")
        assertTrue("a Slider drags sideways", m.swipeBlockedAt(content, slider.x, slider.y))
        val text = at("panel_a_text")
        assertFalse("text is the pager's", m.swipeBlockedAt(content, text.x, text.y))
    }

    /**
     * EXP-1586: the typed [at.exponential.ui.compose.CoreZoomMath] the
     * painter drives (Float views) lands on the fixture's numbers: the rest
     * views, the double taps, the pinches and the menus.
     */
    @Test
    fun theTypedZoomMathAnswersLikeTheFixture() {
        val f = Fixtures.json("zoom.json")
        val math = at.exponential.ui.compose.CoreZoomMath
        fun frame(c: JsonValue): at.exponential.ui.primitives.ZoomFrame {
            val fr = f["frames"]!![c["frame"]!!.string!!]!!
            val vp = c["viewport"] ?: fr["viewport"]!!
            return at.exponential.ui.primitives.ZoomFrame(
                fr["natural"]!!["width"]!!.number!!.toFloat(), fr["natural"]!!["height"]!!.number!!.toFloat(),
                vp["width"]!!.number!!.toFloat(), vp["height"]!!.number!!.toFloat(),
                at.exponential.ui.primitives.ZoomInitial.of(fr["initial"]?.string), fr["maxScale"]?.number?.toFloat(),
            )
        }
        fun from(c: JsonValue, fr: at.exponential.ui.primitives.ZoomFrame) = c["from"]?.takeIf { it is JsonValue.Obj }?.let {
            at.exponential.ui.primitives.ZoomView(it["scale"]!!.number!!.toFloat(), it["x"]!!.number!!.toFloat(), it["y"]!!.number!!.toFloat())
        } ?: math.rest(fr).view
        fun near(name: String, want: JsonValue, got: at.exponential.ui.primitives.ZoomView) {
            assertEquals("$name scale", want["scale"]!!.number!!, got.scale.toDouble(), 1e-3)
            assertEquals("$name x", want["x"]!!.number!!, got.x.toDouble(), 1e-2)
            assertEquals("$name y", want["y"]!!.number!!, got.y.toDouble(), 1e-2)
        }
        for (c in f["rest"]!!.array!!) {
            val fr = frame(c)
            val r = math.rest(fr)
            val want = c["expected"]!!
            near("rest ${c["name"]}", want["view"]!!, r.view)
            assertEquals(want["restScale"]!!.number!!, r.restScale.toDouble(), 1e-5)
            assertEquals(want["tall"]!!.bool, r.tall)
        }
        for (c in f["doubleTap"]!!.array!!) {
            val fr = frame(c)
            val p = c["point"]!!
            near("doubleTap ${c["name"]}", c["expected"]!!, math.doubleTap(from(c, fr), androidx.compose.ui.geometry.Offset(p["x"]!!.number!!.toFloat(), p["y"]!!.number!!.toFloat()), fr))
        }
        for (c in f["pinch"]!!.array!!) {
            val fr = frame(c)
            val a = c["anchor"]!!
            near("pinch ${c["name"]}", c["expected"]!!, math.pinch(from(c, fr), c["factor"]!!.number!!.toFloat(), androidx.compose.ui.geometry.Offset(a["x"]!!.number!!.toFloat(), a["y"]!!.number!!.toFloat()), fr))
        }
        for (c in f["menu"]!!.array!!) {
            val fr = frame(c)
            val got = math.menu(from(c, fr), fr, "Fit")
            val want = c["expected"]!!.array!!
            assertEquals(want.map { it["label"]!!.string }, got.map { it.label })
            assertEquals(want.map { it["checked"]!!.bool }, got.map { it.checked })
            assertEquals(want.map { it["scale"]?.takeIf { s -> s is JsonValue.Num }?.number?.toFloat() }, got.map { it.scale })
        }
        assertEquals("35%", math.percent(0.347222f))
        assertTrue(math.isTall(780f, 25094f))
        assertTrue(!math.isTall(1440f, 900f))
    }

    /** EXP-1586: the painter's copy of style.json `zoomableImage` (the pill geometry, autoHideMs) and the strings. */
    @Test
    fun theZoomPillNumbersAreStyleJsons() {
        val style = JsonValue.parse(java.io.File(Fixtures.dir, "../catalog/style.json").readText())["zoomableImage"]!!
        val pill = style["pill"]!!
        val spec = at.exponential.ui.compose.ZoomSpec
        assertEquals(pill["height"]!!.number!!.toFloat(), spec.PILL_HEIGHT)
        assertEquals(pill["inset"]!!.number!!.toFloat(), spec.PILL_INSET)
        assertEquals(pill["padding"]!!.number!!.toFloat(), spec.PILL_PADDING)
        assertEquals(pill["gap"]!!.number!!.toFloat(), spec.PILL_GAP)
        assertEquals(pill["percentMinWidth"]!!.number!!.toFloat(), spec.PERCENT_MIN_WIDTH)
        assertEquals(pill["dividerHeight"]!!.number!!.toFloat(), spec.DIVIDER_HEIGHT)
        assertEquals(style["autoHideMs"]!!.number!!.toLong(), spec.AUTO_HIDE_MS)
        // The primitives' defaults (an app embedding the view) are the same numbers.
        val d = at.exponential.ui.primitives.ZoomPillMetrics()
        assertEquals(spec.PILL_HEIGHT, d.height.value)
        assertEquals(spec.PILL_INSET, d.inset.value)
        assertEquals(spec.PERCENT_MIN_WIDTH, d.percentMinWidth.value)
        assertEquals(spec.AUTO_HIDE_MS, d.autoHideMs)
        val m = model("""{"id":"x","component":"Text","props":{"text":"x"}}""")
        val labels = at.exponential.ui.primitives.ZoomLabels()
        assertEquals(listOf(labels.zoomIn, labels.zoomOut, labels.zoomToFit, labels.zoomLevel, labels.zoomFit), listOf("zoomIn", "zoomOut", "zoomToFit", "zoomLevel", "zoomFit").map { m.string(it) })
    }

    /**
     * EXP-1586 THE BOX (the measurer's rule): as wide as its column, the
     * natural aspect; a TALL picture no taller than wide; no natural size
     * = 16:9; a style height wins.
     */
    @Test
    fun aZoomableImageBoxTakesTheNaturalAspect() {
        val shaper = robolectricShaper(at.exponential.ui.measure.FontResolver { null }, 1f)
        fun box(props: String, style: String = "{}", width: Float = 400f): androidx.compose.ui.geometry.Rect {
            val m = SurfaceModel("z", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), Host(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
            m.setNested("""{"id":"root","component":"Stack","children":[{"id":"z","component":"ZoomableImage","props":{"src":"https://example.com/a.png","alt":"A",$props},"style":$style}]}""")
            m.setViewport(width, 800f)
            return m.frame(m.idx("z"))
        }
        val wide = box(""""naturalWidth":1440,"naturalHeight":900""")
        assertEquals(400f, wide.width, 0.5f)
        assertEquals(250f, wide.height, 0.5f)
        val tall = box(""""naturalWidth":780,"naturalHeight":25094""")
        assertEquals(400f, tall.height, 0.5f)
        val portrait = box(""""naturalWidth":600,"naturalHeight":900""")
        assertEquals(600f, portrait.height, 0.5f)
        val unknown = box(""""controls":"none"""")
        assertEquals(225f, unknown.height, 0.5f)
        val fixed = box(""""naturalWidth":1440,"naturalHeight":900""", """{"height":300}""")
        assertEquals(300f, fixed.height, 0.5f)
    }

    @Test
    fun aVoiceWaveTakesItsRecipeBoxOpacityAndPartColours() {
        val tree = """{"id":"root","component":"Box","children":[
            {"id":"w","component":"VoiceWave","props":{"activity":"idle","size":"lg","label":"Voice"},"children":[{"id":"w.child","component":"Text","props":{"text":"Hi"}}]},
            {"id":"s","component":"VoiceWave","props":{"activity":"listening","size":"sm"}}]}"""
        val m = model(tree)
        val w = m.idx("w")
        assertEquals(160f, m.frame(w).width, 0.01f)
        assertEquals(160f, m.frame(w).height, 0.01f)
        assertEquals(0.35f, m.boxStyle(w).opacity ?: 1f, 0.001f)
        assertEquals(32f, m.frame(m.idx("s")).width, 0.01f)
        assertEquals(1f, m.boxStyle(m.idx("s")).opacity ?: 1f, 0.001f)
        // idle / muted rings are muted; listening: info, primary, primary.
        val idle = mapOf("activity" to JsonValue.Str("idle"), "size" to JsonValue.Str("lg"))
        val live = mapOf("activity" to JsonValue.Str("listening"), "size" to JsonValue.Str("lg"))
        assertEquals(m.color("mutedForeground"), m.part("VoiceWave", "outer", idle).color)
        assertEquals(m.color("info"), m.part("VoiceWave", "outer", live).color)
        assertEquals(m.color("primary"), m.part("VoiceWave", "inner", live).color)
        assertEquals(m.color("foreground"), m.part("VoiceWave", "child", live).color)
    }

    @Test
    fun toastsDismissAfterTheirDurationUnlessHeld() {
        val scope = TestScope(StandardTestDispatcher())
        val tree = """{"id":"root","component":"Box","children":[{"id":"t","component":"Toast","props":{"title":"Saved","open":true,"duration":4000}}]}"""
        val m = model(tree, scope = scope)
        assertTrue(m.toasts.any { it.id == "t" })
        scope.advanceTimeBy(2000)
        scope.runCurrent()
        m.toastHold("t", true, "hover")
        scope.advanceTimeBy(5000)
        scope.runCurrent()
        assertTrue("held toasts stay", m.toasts.any { it.id == "t" })
        // A press under the hovering mouse: released, the hover still holds it.
        m.toastHold("t", true, "press")
        m.toastHold("t", false, "press")
        scope.advanceTimeBy(5000)
        scope.runCurrent()
        assertTrue("the hover still holds it", m.toasts.any { it.id == "t" })
        m.toastHold("t", false, "hover")
        // The REST of the duration (2 s), never the full 4 s again.
        scope.advanceTimeBy(1800)
        scope.runCurrent()
        assertTrue("2 s were left", m.toasts.any { it.id == "t" })
        scope.advanceTimeBy(300)
        scope.runCurrent()
        assertFalse("the timer dismissed it", m.toasts.any { it.id == "t" })
        assertTrue("gone toasts are forgotten", m.toastLeft.isEmpty() && m.toastHeld.isEmpty())
    }

    @Test
    fun scrollContainersReportTheirOffsetsAndStickyPins() {
        val rows = (0 until 30).joinToString(",") { """{"id":"r$it","component":"Text","props":{"text":"Row $it"},"style":{"height":40,"flexShrink":0}}""" }
        val tree = """{"id":"root","component":"Box","children":[{"id":"area","component":"Box","style":{"height":200,"overflowY":"scroll","display":"flex","flexDirection":"column"},"children":[{"id":"pin","component":"Text","props":{"text":"Header"},"style":{"position":"sticky","top":0,"height":30,"flexShrink":0}},$rows]}]}"""
        val m = model(tree)
        val area = m.idx("area")
        val scroll = m.scrolls[area]
        assertNotNull(scroll)
        assertTrue(scroll!!.scrollY)
        assertTrue("content ${scroll.contentHeight}", scroll.contentHeight > 1000f)
        m.scrollTo("area", 0f, 300f)
        assertEquals(300f, m.scrolls[m.idx("area")]!!.offsetY, 0.5f)
        val pin = m.sticky[m.idx("pin")]
        assertNotNull("the sticky header is pinned", pin)
        // The core pins it inside the container's own box (see the README: CSS keeps it pinned through the content).
        assertTrue("pinned ${pin!!.y}", pin.y > 0f)
        // A key on the focused scroll container scrolls it.
        m.focus("area", true)
        assertTrue(handleKey(m, "home"))
        assertEquals(0f, m.scrolls[m.idx("area")]!!.offsetY, 0.5f)
    }

    @Test
    fun aPlainScrollContainerScrollsWithoutAPassAndTheCoreCanMoveItsView() {
        val rows = (0 until 30).joinToString(",") { """{"id":"r$it","component":"Text","props":{"text":"Row $it"},"style":{"height":40,"flexShrink":0}}""" }
        val tree = """{"id":"root","component":"Box","children":[{"id":"area","component":"Box","style":{"height":200,"overflowY":"scroll","display":"flex","flexDirection":"column"},"children":[$rows]}]}"""
        val m = model(tree)
        val area = m.idx("area")
        val passes = m.passCount
        // The view's own scroll (a fling): no layout pass, no node read.
        for (y in listOf(40f, 80f, 120f, 300f)) m.scrollTo("area", 0f, y, fromView = true)
        assertEquals("a plain container only moves paint", passes, m.passCount)
        // The core moves it (a key): one pass, and the view is told.
        val epoch = m.scrollEpoch
        m.focus("area", true)
        assertTrue(handleKey(m, "home"))
        assertEquals(0f, m.scrolls[area]!!.offsetY, 0.5f)
        assertTrue("the view follows the core's offset", m.scrollEpoch > epoch)
        // A sticky node inside: every step lays out (the pin moves).
        val pinned = model(tree.replace(""""children":[{"id":"r0"""", """"children":[{"id":"pin","component":"Text","props":{"text":"H"},"style":{"position":"sticky","top":0,"height":30,"flexShrink":0}},{"id":"r0""""))
        val p0 = pinned.passCount
        pinned.scrollTo("area", 0f, 120f, fromView = true)
        assertTrue("a pinning container lays out", pinned.passCount > p0)
        assertTrue(pinned.sticky[pinned.idx("pin")]!!.y > 0f)
    }

    @Test
    fun chartNumbersGoThroughTheSurfaceFormatter() {
        val de = IcuFormatter.create("de-DE", "UTC")
        assertEquals("1.234,5", at.exponential.ui.paint.ChartModel.format(1234.5, de))
        assertEquals("1,234.5", at.exponential.ui.paint.ChartModel.format(1234.5, IcuFormatter.create("en-US", "UTC")))
        assertEquals("a missing point has no label", "", at.exponential.ui.paint.ChartModel.format(Double.NaN, de))
        val chart = at.exponential.ui.paint.ChartModel(
            JsonValue.parse("""{"kind":"line","series":[{"name":"Runs","values":[3,null,8]},{"name":"","values":[-1]}]}""").obj!!,
        )
        assertTrue("null stays a gap", chart.series[0].values[1].isNaN())
        assertEquals(Triple("Runs", -1.0, 8.0), chart.summaryParams())
    }

    @Test
    fun scrollToIndexRendersTheItemOfAWindowedList() {
        val tree = """{"id":"root","component":"Box","children":[{"id":"l","component":"List","style":{"height":400},"template":{"component":"item","path":"/items"},"children":[{"id":"item","component":"Text","props":{"text":{"@path":"t"}},"style":{"height":40,"flexShrink":0}}]}]}"""
        val m = model(tree)
        m.setData("/items", JsonValue.Arr((0 until 1000).map { JsonValue.Obj(mapOf("t" to JsonValue.Str("Item $it"))) }))
        assertTrue(m.lists["l"]?.windowed == true)
        assertFalse(m.nodes.any { it.props["text"]?.string == "Item 600" && !it.hidden })
        m.scrollToIndex("l", 600, "start")
        assertTrue(m.nodes.any { it.props["text"]?.string == "Item 600" && !it.hidden })
    }

    @Test
    fun theKeyboardRovesTabsAndEscapeClosesLayers() {
        val m = model(Fixtures.text("kitchen-sink.json"))
        m.focus("tabs.tab.0", true)
        assertTrue(handleKey(m, "right"))
        assertEquals("tabs.tab.1", m.focusRequest)
        assertTrue(m.node(m.idx("tabs.tab.1"))!!.selected)
        m.setOpen("dialog", true)
        assertTrue(m.layers.any { it.owner == "dialog" })
        assertTrue(handleKey(m, "escape"))
        assertFalse(m.layers.any { it.owner == "dialog" })
    }

    @Test
    fun animationsReachTheVisualAndReducedMotionStillsThem() {
        val tree = """{"id":"root","component":"Box","children":[{"id":"s","component":"Box","style":{"animation":"pulse","width":20,"height":20}}]}"""
        val m = model(tree)
        val a = m.style(m.idx("s")).animation
        assertNotNull(a)
        assertEquals("pulse", a!!.name)
        assertTrue(a.infinite)
        assertFalse(a.reduced)
        val still = model(tree, SurfaceSettings(reducedMotion = true))
        assertTrue(still.style(still.idx("s")).animation!!.reduced)
        assertTrue(still.reducedMotion)
    }

    @Test
    fun directionReachesTheLeafVisuals() {
        val tree = """{"id":"root","component":"Box","style":{"direction":"rtl"},"children":[{"id":"t","component":"Text","props":{"text":"x","align":"end"}}]}"""
        val m = model(tree)
        val st = m.style(m.idx("t"))
        assertEquals("rtl", st.direction)
        assertEquals("left", st.textAlign)
        assertEquals("rtl", m.direction)
    }
}
