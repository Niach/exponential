package at.exponential.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.LeafImages
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.press
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ResolvedTextStyle
import at.exponential.ui.theme.ThemeHandle
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.RoborazziTaskType
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * EXP-1495 (docs/round-6-contract.md §3 + §4 Compose): the Carousel
 * `counter` pill and `thumbnails` strip painted from the core's frames
 * (tabular figures, hidden from TalkBack, a thumbnail press pages through
 * the Carousel's press path, the current thumbnail kept in view, the
 * picture covering the box inside its border); the Outline and Mosaic
 * macros through the painter's Box / Text / Image paths (`aspectRatio`,
 * absolute `inset` / `insetInlineStart` under RTL, an Image `fit: cover`
 * clipped by a rounded box, press → `select`).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class Round6Test {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private val density: Float get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    private fun render(json: String, host: RecordingHost = RecordingHost()): SurfaceModel {
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("r6", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent {
            Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) { ExponentialSurface(m) }
        }
        compose.waitForIdle()
        return m
    }

    private fun capture(name: String): Bitmap {
        val file = File("build/tmp/round6/$name.png")
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(file.path, RoborazziOptions(taskType = RoborazziTaskType.Record))
        return BitmapFactory.decodeFile(file.absolutePath)
    }

    private fun near(a: Int, b: Int, tol: Int = 12) = listOf(16, 8, 0).all { abs((a shr it and 255) - (b shr it and 255)) <= tol }

    private fun assertClose(what: String, want: androidx.compose.ui.geometry.Rect, got: androidx.compose.ui.geometry.Rect) {
        val d = listOf(want.left - got.left, want.top - got.top, want.right - got.right, want.bottom - got.bottom).maxOf { abs(it) }
        assertTrue("$what: $want vs $got", d < 0.05f)
    }

    private fun carousel(pages: Int, extra: String, width: Int? = null): String {
        val kids = (0 until pages).joinToString(", ") { """{"id": "p$it", "component": "Box", "style": {"height": 120, "backgroundColor": "#333333"}}""" }
        val style = width?.let { """, "style": {"width": $it}""" } ?: ""
        return """{"id": "c", "component": "Carousel", "props": {$extra}$style, "on": {"change": {"event": {"name": "change"}}}, "children": [$kids]}"""
    }

    /** A solid red 8 x 8 PNG. */
    private val red = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEklEQVR4nGP4z8CAFWEXHbQSACj/P8Fu7N9hAAAAAElFTkSuQmCC"

    private fun awaitImage(url: String) {
        compose.waitUntil(10_000) { LeafImages.cached(url) != null }
        compose.waitForIdle()
    }

    @Test
    fun theCounterIsAPillOverTheTracksTopStartCornerInTabularFigures() {
        val m = render(carousel(3, """"page": 0, "counter": true"""))
        val page = m.frame(m.idx("c.page"))
        val pill = m.frame(m.idx("c.counter"))
        assertEquals("inset sm from the start", 8f, pill.left - page.left, 0.01f)
        assertEquals("inset sm from the top", 8f, pill.top - page.top, 0.01f)
        assertEquals(20f, pill.height, 0.01f)
        val counter = m.node(m.idx("c.counter"))!!
        assertEquals("1 / 3", counter.props["text"]?.string)
        // Tabular figures, in the measurer and the leaf alike.
        val ts = m.textStyle(m.idx("c.counter"))
        assertEquals("tnum", ts.forPart(counter.ownerComponent, counter.part).fontFeatures)
        assertNull(ts.forPart("Carousel", "page").fontFeatures)
        assertEquals("tnum", robolectricShaper().style(ResolvedTextStyle(12f, 500, 20f, null, fontFeatures = "tnum")).fontFeatureSettings)
        // The recipe's fill: black 0.55 over the dark track.
        val px = capture("counter")
        val d = density
        val inside = px.getPixel(((pill.left + 3f) * d).roundToInt(), ((pill.top + pill.height / 2f) * d).roundToInt())
        assertTrue("the pill is darker than the track (${Integer.toHexString(inside)})", (inside shr 16 and 255) < 0x33 - 8)
        // Hidden from TalkBack: each page already says `page i of n`.
        // (the merged tree = what the accessibility delegate hands TalkBack).
        assertTrue(compose.onAllNodes(hasText("1 / 3")).fetchSemanticsNodes().isEmpty())
        // It counts the page the core holds.
        m.press("c.next")
        compose.waitForIdle()
        assertEquals("2 / 3", m.node(m.idx("c.counter"))!!.props["text"]?.string)
        // No counter without the prop, none on one page.
        assertNull(makeModel().also { it.setNested(carousel(3, """"page": 0""")) }.indexOf("c.counter"))
    }

    @Test
    fun aThumbnailPressPagesThroughTheCarouselAndTheCurrentOneIsSelected() {
        val host = RecordingHost()
        val m = render(carousel(3, """"page": 0, "thumbnails": ["$red", "", "$red", "$red"]"""), host)
        val strip = m.idx("c.thumbnails")
        assertEquals(listOf("c.page", "c.thumbnails", "c.controls"), m.children[m.idx("c")].map { m.node(it)!!.id })
        assertEquals(listOf("c.thumbnail.0", "c.thumbnail.1", "c.thumbnail.2"), m.children[strip].map { m.node(it)!!.id })
        val t0 = m.frame(m.idx("c.thumbnail.0"))
        val t1 = m.frame(m.idx("c.thumbnail.1"))
        assertEquals(52f, t0.width, 0.01f)
        assertEquals(52f, t0.height, 0.01f)
        assertEquals("gap sm", 60f, t1.left - t0.left, 0.01f)
        assertTrue(m.node(m.idx("c.thumbnail.0"))!!.states.contains("selected"))
        assertNull("a missing URL = the empty square", m.indexOf("c.thumbnailImage.1"))
        // The picture fills the box inside the 2 px border, covering it.
        val img = m.frame(m.idx("c.thumbnailImage.0"))
        assertEquals(48f, img.width, 0.6f)
        assertEquals(t0.left + 2f, img.left, 0.6f)
        assertEquals("cover", m.node(m.idx("c.thumbnailImage.0"))!!.props["fit"]?.string)
        // One button per page, the current one selected for TalkBack.
        val first = compose.onAllNodes(hasContentDescription("Page 1 of 3")).fetchSemanticsNodes()
        assertTrue(first.any { it.config.getOrElseNullable(SemanticsProperties.Selected) { null } == true })
        val third = compose.onAllNodes(hasContentDescription("Page 3 of 3"))
        assertFalse(third.fetchSemanticsNodes().any { it.config.getOrElseNullable(SemanticsProperties.Selected) { null } == true })

        // A real tap: the Carousel's press path (the core reads `target`).
        third[0].performClick()
        compose.waitForIdle()
        assertEquals(listOf("change"), host.actions.map { it.name })
        val payload = host.actions[0].let { it.payload ?: it.context }
        assertEquals(2.0, payload["page"]?.number ?: 0.0, 0.0)
        assertTrue(m.node(m.idx("c.thumbnail.2"))!!.states.contains("selected"))
        assertFalse(m.node(m.idx("c.thumbnail.0"))!!.states.contains("selected"))
        assertEquals(2.0, m.node(m.idx("c.indicator"))!!.props["page"]?.number ?: 0.0, 0.0)

        // The picture paints red in the middle of the selected thumbnail; its
        // border (the foreground) frames it.
        awaitImage(red)
        val px = capture("thumbnails")
        val d = density
        val t2 = m.frame(m.idx("c.thumbnail.2"))
        fun at(x: Float, y: Float) = px.getPixel(((t2.left + x) * d).roundToInt(), ((t2.top + y) * d).roundToInt())
        assertTrue("the picture (${Integer.toHexString(at(26f, 26f))})", near(at(26f, 26f), Color.Red.toArgb(), 24))
        val fg = m.color("foreground")!!.toArgb()
        assertTrue("the selected border (${Integer.toHexString(at(1f, 26f))})", near(at(1f, 26f), fg, 40))
    }

    @Test
    fun theStripKeepsTheCurrentThumbnailInView() {
        val m = render(carousel(10, """"page": 9, "thumbnails": []""", width = 200))
        val strip = m.idx("c.thumbnails")
        assertTrue("the strip scrolls sideways", m.scrolls[strip]?.scrollX == true)
        compose.waitForIdle()
        val f = m.frame(strip)
        val last = m.frame(m.idx("c.thumbnail.9"))
        val offset = m.viewScroll[strip]?.x ?: 0f
        assertEquals("the last thumbnail's end edge at the strip's", last.right - f.left - f.width, offset, 1f)
        m.press("c.thumbnail.0")
        compose.waitForIdle()
        assertEquals("back to the start", 0f, m.viewScroll[strip]?.x ?: 0f, 1f)
    }

    @Test
    fun theOutlineMarksTheActiveRowAtTheInlineStartAndSelects() {
        val outline = """{"id": "o", "component": "Outline", "props": {"caption": "Outline", "value": "b", "items": [{"id": "a", "title": "Current state"}, {"id": "b", "title": "Part A", "count": 5}]}, "on": {"select": {"event": {"name": "select"}}}}"""
        val host = RecordingHost()
        val m = render(outline, host)
        val row = m.frame(m.idx("o.row.1"))
        val bar = m.frame(m.idx("o.row.1.bar"))
        assertEquals("\$control.rowCompact tall", m.control("rowCompact", 32f), row.height, 0.01f)
        assertEquals(2f, bar.width, 0.01f)
        assertEquals(row.left, bar.left, 0.01f)
        assertEquals(row.height, bar.height, 0.01f)
        assertNull("only the active row has the bar", m.indexOf("o.row.0.bar"))
        // The row is named by the core's label (the macro's `$a11y`), the active one selected (`current: location`).
        val label = m.node(m.idx("o.row.1"))!!.accessibility?.get("label")?.string ?: error("a label")
        assertTrue(label.startsWith("Part A"))
        val active = compose.onAllNodes(hasContentDescription(label)).fetchSemanticsNodes()
        assertEquals(1, active.size)
        assertEquals(true, active[0].config.getOrElseNullable(SemanticsProperties.Selected) { null })
        compose.onAllNodes(hasContentDescription("Current state"))[0].performClick()
        compose.waitForIdle()
        assertEquals(listOf("select"), host.actions.map { it.name })
        assertEquals("a", host.actions[0].let { it.payload ?: it.context }["id"]?.string ?: host.actions[0].context["id"]?.string)

        // RTL: the bar mirrors to the row's right edge.
        val rtl = makeModel().also { it.setNested("""{"id": "r", "component": "Box", "style": {"direction": "rtl"}, "children": [$outline]}""") }
        val rrow = rtl.frame(rtl.idx("o.row.1"))
        val rbar = rtl.frame(rtl.idx("o.row.1.bar"))
        assertEquals(rrow.right, rbar.right, 0.01f)
    }

    @Test
    fun theMosaicIsAWideGroupOfCoveredRoundedTilesThatSelect() {
        val pics = (0 until 4).joinToString(", ") { """{"src": "$red", "alt": "P$it"}""" }
        val host = RecordingHost()
        val m = render("""{"id": "g", "component": "Mosaic", "props": {"label": "Screens", "pictures": [$pics]}, "on": {"select": {"event": {"name": "select"}}}}""", host)
        val root = m.frame(m.idx("g"))
        assertEquals("aspectRatio 1.6", root.width / 1.6f, root.height, 0.6f)
        val lead = m.frame(m.idx("g.lead"))
        assertEquals("the lead = 60 % of the row", root.width * 0.6f, lead.width, 1f)
        val more = m.frame(m.idx("g.side.bottom.more"))
        val bottom = m.frame(m.idx("g.side.bottom"))
        // `inset: 0` = the padding box (inside the tile's hairline border), as in CSS.
        val hair = more.left - bottom.left
        assertTrue("a hairline ($hair)", hair in 0f..1.01f)
        assertClose("the +N cover = inset 0", bottom.deflate(hair), more)
        assertEquals("+1", m.node(m.idx("g.side.bottom.more.moreLabel"))!!.props["text"]?.string)
        assertClose("the picture = inset 0", lead.deflate(hair), m.frame(m.idx("g.lead.image")))
        assertEquals("cover", m.node(m.idx("g.lead.image"))!!.props["fit"]?.string)

        compose.onAllNodes(hasContentDescription("P1"))[0].performClick()
        compose.waitForIdle()
        assertEquals(listOf("select"), host.actions.map { it.name })
        assertEquals(1.0, host.actions[0].let { it.payload ?: it.context }["index"]?.number ?: -1.0, 0.0)

        // The picture covers the tile and the tile's radius clips it.
        awaitImage(red)
        val px = capture("mosaic")
        val d = density
        fun at(x: Float, y: Float) = px.getPixel(((lead.left + x) * d).roundToInt(), ((lead.top + y) * d).roundToInt())
        assertTrue("the picture (${Integer.toHexString(at(lead.width / 2f, lead.height / 2f))})", near(at(lead.width / 2f, lead.height / 2f), Color.Red.toArgb(), 24))
        assertFalse("the rounded corner clips it (${Integer.toHexString(at(0.6f, 0.6f))})", near(at(0.6f, 0.6f), Color.Red.toArgb(), 60))
    }

    @Test
    fun theTabularStyleReachesTheShaper() {
        val shaper = robolectricShaper()
        assertNull(shaper.style(ResolvedTextStyle.BODY).fontFeatureSettings)
        assertEquals(FontFamily.Default, shaper.family(null))
    }
}
