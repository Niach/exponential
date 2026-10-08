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
import at.exponential.ui.model.setOpen
import at.exponential.ui.model.toastHold
import at.exponential.ui.theme.Mode
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
        val tree = """{"id":"root","component":"Box","children":[{"id":"l","component":"List","style":{"height":400},"template":{"component":"item","path":"/items"},"children":[{"id":"item","component":"Text","props":{"text":{"path":"t"}},"style":{"height":40,"flexShrink":0}}]}]}"""
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
