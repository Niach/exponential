package at.exponential.ui

import androidx.compose.ui.geometry.Size
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.ControlBox
import at.exponential.ui.measure.SurfaceMeasurer
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.paint.DateModel
import at.exponential.ui.theme.ResolvedTextStyle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The measurer's rules (the gpui / Swift ones) on a REAL Compose text
 * measurer (Robolectric's native graphics, density 1, fontScale 1), then a
 * real-measure kitchen-sink pass: batched upcalls and a warm memo.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class MeasurerRulesTest {
    private val ts = ResolvedTextStyle(14f, 400, 20f, null)

    private fun shaper(): TextShaper = robolectricShaper()

    @Test
    fun measurerRules() {
        val s = shaper()
        val one = s.measure("Hello world", ts, wrap = null, lines = null)
        assertEquals(20f, one.height, 0f)
        assertTrue("width ${one.width}", one.width > 40f)
        val wrapped = s.measure("Hello world again and again", ts, wrap = 60f, lines = null)
        assertTrue("height ${wrapped.height}", wrapped.height >= 40f)
        assertEquals("n lines = n × lineHeight", 0f, wrapped.height % 20f, 0f)
        assertEquals("a one-line text is nowrap: min-content = the whole line", s.maxContent("Hello world", ts), s.measure("Hello world", ts, wrap = 0f, lines = 1).width, 0f)
        assertEquals(s.minContent("Hello world", ts), s.measure("Hello world", ts, wrap = 0f, lines = null).width, 0f)
        assertEquals("min-content = the widest word", s.maxContent("world", ts), s.minContent("Hello world", ts), 0f)
        assertEquals("clamped to the lines prop", 40f, s.measure("a\nb\nc", ts, wrap = null, lines = 2).height, 0f)
        assertEquals(60f, s.measure("a\nb\nc", ts, wrap = null, lines = null).height, 0f)
        // A one-line text never wraps: the line height at any width.
        assertEquals(20f, s.measure("Hello world again and again", ts, wrap = 30f, lines = 1).height, 0f)
        // Widths are the FRACTIONAL shaped extent (never ceiled to a px, as the
        // web and gpui), and max-content is the line width.
        assertTrue("fractional width ${one.width}", one.width != kotlin.math.ceil(one.width))
        assertEquals(s.maxContent("Hello world", ts), one.width, 0f)
    }

    @Test
    fun controlBoxAndLabels() {
        val c = ControlBox(paddingHorizontal = 16f, paddingVertical = 0f, borderWidth = 1f, gap = 8f, minWidth = 80f, minHeight = null, width = null, height = 36f)
        assertEquals(34f, c.insets.first, 0f)
        assertEquals(2f, c.insets.second, 0f)
        assertEquals(Size(80f, 36f), c.borderBox(Size(20f, 20f)))
        assertEquals(Size(134f, 36f), c.borderBox(Size(100f, 20f)))
        assertEquals(100f, c.innerWrap(134f)!!, 0f)
        assertEquals(0f, c.innerWrap(0f)!!, 0f)
        assertNull(c.innerWrap(null))
        val options = JsonValue.Arr(
            listOf(
                JsonValue.Obj(mapOf("label" to JsonValue.Str("A"), "value" to JsonValue.Str("a"))),
                JsonValue.Obj(mapOf("label" to JsonValue.Str("B"), "value" to JsonValue.Str("b"))),
            ),
        )
        assertEquals("B", SurfaceMeasurer.selectLabel(mapOf("options" to options, "value" to JsonValue.Str("b"))))
        assertEquals("Pick", SurfaceMeasurer.selectLabel(mapOf("options" to JsonValue.Arr(emptyList()), "placeholder" to JsonValue.Str("Pick"))))
        assertEquals("Oct 14, 2026", SurfaceMeasurer.dateLabel("2026-10-14"))
        assertNull(SurfaceMeasurer.dateLabel("nope"))
        assertEquals(0.3, SurfaceModel.snap(0.30000000004, 0.0, 1.0, 0.1), 0.0)
        assertEquals(40.0, SurfaceModel.snap(42.0, 0.0, 100.0, 5.0), 0.0)
        assertEquals(100.0, SurfaceModel.snap(140.0, 0.0, 100.0, 5.0), 0.0)
        assertEquals("1:02:05", DateModel.formatDuration(3_725_000.0))
    }

    @Test
    fun snapReachesAMaxThatIsNoStop() {
        // step 3 over 0…10: the stops end at 9, yet a value nearer 10 than 9 snaps to 10 (Swift's rule).
        assertEquals(10.0, SurfaceModel.snap(10.0, 0.0, 10.0, 3.0), 0.0)
        assertEquals(9.0, SurfaceModel.snap(9.4, 0.0, 10.0, 3.0), 0.0)
        assertEquals(10.0, SurfaceModel.snap(9.6, 0.0, 10.0, 3.0), 0.0)
        assertEquals(10.0, SurfaceModel.snap(25.0, 0.0, 10.0, 3.0), 0.0)
        assertEquals(5.0, SurfaceModel.snap(5.0, 0.0, 10.0, 2.5), 0.0)
        assertEquals(0.0, SurfaceModel.snap(-4.0, 0.0, 10.0, 3.0), 0.0)
    }

    @Test
    fun kitchenSinkUnderRealMeasure() {
        Fixtures.require()
        val m = makeModel("ks-real", fixed = false)
        m.setNested(Fixtures.text("kitchen-sink.json"))
        val cold = m.stats
        assertTrue("upcalls ${cold.upcalls} (batched)", cold.upcalls in 1..3)
        assertTrue("the painter measured ${cold.measureCalls}", cold.measureCalls > 0)
        m.setData("/draft", JsonValue.Obj(mapOf("title" to JsonValue.Str(""))))
        assertEquals(m.issues.toString(), 0, m.issues.size)
        assertTrue("height ${m.surfaceSize.height}", m.surfaceSize.height > 1000f)
        assertTrue("upcalls ${m.stats.upcalls} (batched)", m.stats.upcalls <= 3)
        // Every visible main-tree node fits the surface's width.
        for (n in m.nodes) {
            if (n.hidden || n.layer != 0) continue
            val f = m.frame(n.index)
            assertTrue("${n.id} $f", f.left >= -0.01f && f.right <= m.surfaceSize.width + 0.01f)
        }
        // A warm pass makes no measure calls (the core's memo).
        m.layoutNeeded = true
        m.pass()
        assertEquals(0, m.stats.upcalls)
        assertEquals(0, m.stats.measureCalls)
        // A new measure identity (fonts changed) measures again.
        m.invalidateMeasures()
        assertTrue("re-measured after invalidateMeasures", m.stats.upcalls > 0)
    }
}
