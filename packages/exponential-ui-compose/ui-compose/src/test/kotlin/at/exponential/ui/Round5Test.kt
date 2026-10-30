package at.exponential.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.text.font.FontFamily
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.LocalQrScanner
import at.exponential.ui.compose.QrMatrix
import at.exponential.ui.compose.QrScannerHost
import at.exponential.ui.compose.VoiceWaveBarsSpec
import at.exponential.ui.compose.qrColours
import at.exponential.ui.compose.segmentedBadge
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.hold
import at.exponential.ui.model.press
import at.exponential.ui.model.pressDown
import at.exponential.ui.model.pressUp
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.RoborazziTaskType
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
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
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * VAPP-162 (docs/round-5-contract.md): the round-5 pieces of the Compose
 * painter. The fixtures through the facade (`qr-code.json` bit for bit,
 * `voice-wave.json bars`, `interactions-round1.json buttonHold`), then the
 * painted QrCode, QrScanner (placeholder and a host camera), the VoiceWave
 * meter, the rail's count badges, a hold Button under touch and `ui-serif`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class Round5Test {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private val density: Float get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    private fun render(json: String, host: RecordingHost = RecordingHost(), wrap: @Composable (@Composable () -> Unit) -> Unit = { it() }): SurfaceModel {
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("r5", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent {
            wrap {
                Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) { ExponentialSurface(m) }
            }
        }
        compose.waitForIdle()
        return m
    }

    private fun capture(name: String): Bitmap {
        val file = File("build/tmp/round5/$name.png")
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(file.path, RoborazziOptions(taskType = RoborazziTaskType.Record))
        return BitmapFactory.decodeFile(file.absolutePath)
    }

    private fun near(a: Int, b: Int, tol: Int = 10) = listOf(16, 8, 0).all { abs((a shr it and 255) - (b shr it and 255)) <= tol }

    @Test
    fun theQrFixtureReplaysBitForBitThroughTheFacade() {
        val cases = Fixtures.json("qr-code.json")["cases"]!!.array!!
        assertEquals(13, cases.size)
        val failed = cases.mapNotNull { c ->
            val name = c["name"]!!.string!!
            val got = QrMatrix.of(c["value"]?.string ?: "", c["level"]?.string ?: "M")
            val want = c["expected"]
            when {
                want == null || want.isNull -> if (got != null) "$name: a matrix for no matrix" else null
                got == null -> "$name: no matrix"
                got.size != want["size"]!!.number!!.toInt() -> "$name: size ${got.size}"
                got.version != want["version"]!!.number!!.toInt() -> "$name: version ${got.version}"
                got.mask != want["mask"]!!.number!!.toInt() -> "$name: mask ${got.mask}"
                got.rows != want["rows"]!!.array!!.map { it.string } -> "$name: rows differ"
                else -> null
            }
        }
        assertEquals(emptyList<String>(), failed)
    }

    @Test
    fun theBarsFixtureReplaysThroughTheFacade() {
        val cases = Fixtures.json("voice-wave.json")["bars"]!!.array!!
        assertEquals(8, cases.size)
        val failed = cases.mapNotNull { c ->
            val name = c["name"]!!.string!!
            val i = c["input"]!!
            val got = VoiceWaveBarsSpec.of(
                VoiceWaveBarsSpec.input(
                    i["size"]?.string ?: "md",
                    i["activity"]?.string ?: "idle",
                    i["level"]?.number ?: 0.0,
                    i["bands"]?.array?.mapNotNull { it.number } ?: emptyList(),
                    i["elapsed"]?.number ?: 0.0,
                    i["reducedMotion"]?.bool ?: false,
                ),
            ) ?: return@mapNotNull "$name: refused"
            val want = c["expected"]!!
            val bars = want["bars"]!!.array!!
            if (abs(got.width - want["width"]!!.number!!) > 1e-3 || abs(got.height - want["height"]!!.number!!) > 1e-3) return@mapNotNull "$name: box ${got.width}×${got.height}"
            if (abs(got.phase - want["phase"]!!.number!!) > 1e-3) return@mapNotNull "$name: phase ${got.phase}"
            if (got.bars.size != bars.size) return@mapNotNull "$name: ${got.bars.size} bars"
            got.bars.zip(bars).withIndex().firstNotNullOfOrNull { (k, p) ->
                val (g, w) = p
                val diff = listOf(g.x to "x", g.y to "y", g.width to "width", g.height to "height", g.radius to "radius").firstOrNull { (v, key) -> abs(v - w[key]!!.number!!) > 1e-3 }
                diff?.let { "$name: bar $k ${it.second} ${it.first} / ${w[it.second]!!.json}" }
            }
        }
        assertEquals(emptyList<String>(), failed)
    }

    /**
     * `interactions-round1.json buttonHold` (10 cases) through the model's
     * `hold` (THE painter entry), the fixed measure at 120 × 40: after every
     * step the fired actions, `pressed` (the core's state, or the host's for
     * `hold: false`) and the painted label.
     */
    @Test
    fun theButtonHoldFixtureReplaysThroughTheModel() {
        val cases = Fixtures.json("interactions-round1.json")["buttonHold"]!!.array!!
        assertEquals(10, cases.size)
        val failed = ArrayList<String>()
        for (c in cases) {
            val name = c["name"]!!.string!!
            val props = linkedMapOf<String, JsonValue>("label" to JsonValue.Str("Hold to talk"), "holdingLabel" to JsonValue.Str("Holding…"), "hold" to JsonValue.Bool(true))
            c["props"]?.obj?.forEach { (k, v) -> if (v.isNull) props.remove(k) else props[k] = v }
            val plain = props["hold"]?.bool != true
            val on = JsonValue.parse("""{"press": {"event": {"name": "press"}}, "holdStart": {"event": {"name": "holdStart"}}, "holdEnd": {"event": {"name": "holdEnd"}}, "holdCancel": {"event": {"name": "holdCancel"}}}""")
            val tree = JsonValue.Obj(mapOf("id" to JsonValue.Str("root"), "component" to JsonValue.Str("Box"), "children" to JsonValue.Arr(listOf(JsonValue.Obj(mapOf("id" to JsonValue.Str("b"), "component" to JsonValue.Str("Button"), "props" to JsonValue.Obj(props), "on" to on, "style" to JsonValue.parse("""{"width": 120, "height": 40}""")))))))
            val host = RecordingHost()
            val m = makeModel("hold", fixed = true, host = host)
            m.setNested(tree.json)
            val b = m.idx("b")
            assertEquals("$name width", 120f, m.frame(b).width, 0.01f)
            for ((k, step) in c["steps"]!!.array!!.withIndex()) {
                val before = host.actions.size
                val input = JsonValue.Obj(step.obj!!.filterKeys { it !in setOf("events", "pressed", "label") })
                if (plain) {
                    when (step["do"]?.string) {
                        "pointerdown" -> m.pressDown("b")
                        "pointerup" -> {
                            m.pressUp("b")
                            val x = step["x"]!!.number!!
                            val y = step["y"]!!.number!!
                            if (x in 0.0..120.0 && y in 0.0..40.0) m.press(b)
                        }
                    }
                } else {
                    m.hold(b, input)
                }
                val fired = host.actions.drop(before).map { it.name }
                val want = step["events"]!!.array!!.map { it.string }
                if (fired != want) failed.add("$name step $k: events $fired / $want")
                val node = m.node(m.idx("b"))!!
                val pressed = node.states.contains("pressed") || m.statesOf("b").contains("pressed")
                if (pressed != step["pressed"]!!.bool) failed.add("$name step $k: pressed $pressed")
                if (node.props["label"]?.string != step["label"]!!.string) failed.add("$name step $k: label ${node.props["label"]?.string}")
            }
        }
        assertEquals(emptyList<String>(), failed)
    }

    /** A hold Button under real touch: down + up = holdStart, holdEnd (never press); a slide away cancels. */
    @Test
    fun aHoldButtonTakesTheTouchAndCancelsOnTheSlideAway() {
        val host = RecordingHost()
        val m = render("""{"id": "b", "component": "Button", "props": {"label": "Hold to talk", "holdingLabel": "Holding…", "hold": true}, "style": {"width": 160, "height": 40}, "on": {"press": {"event": {"name": "press"}}, "holdStart": {"event": {"name": "holdStart"}}, "holdEnd": {"event": {"name": "holdEnd"}}, "holdCancel": {"event": {"name": "holdCancel"}}}}""", host)
        val f = m.frame(m.idx("b"))
        val d = density
        val centre = Offset(f.center.x * d, f.center.y * d)
        compose.onNodeWithTag("shot").performTouchInput { down(centre) }
        compose.waitForIdle()
        assertEquals(listOf("holdStart"), host.actions.map { it.name })
        assertEquals("Holding…", m.node(m.idx("b"))!!.props["label"]?.string)
        compose.onNodeWithTag("shot").performTouchInput { up() }
        compose.waitForIdle()
        assertEquals(listOf("holdStart", "holdEnd"), host.actions.map { it.name })
        assertEquals("Hold to talk", m.node(m.idx("b"))!!.props["label"]?.string)
        host.actions.clear()
        compose.onNodeWithTag("shot").performTouchInput {
            down(centre)
            moveTo(Offset(centre.x, centre.y + (f.height / 2f + 60f) * d))
            up()
        }
        compose.waitForIdle()
        assertEquals(listOf("holdStart", "holdCancel"), host.actions.map { it.name })
    }

    /**
     * A QrCode in the DARK theme still scans: the plate is the lighter of
     * the two recipe colours, the top-left finder module dark; the frame is
     * the plate over the label; one image named by the label.
     */
    @Test
    fun aQrCodePaintsDarkModulesOnALightPlateAndReadsAsItsLabel() {
        val m = render("""{"id": "q", "component": "QrCode", "props": {"value": "https://exponential.at", "label": "Scan to pair"}}""")
        val box = m.frame(m.idx("q"))
        // The leaf stretches across the surface; the plate is centred at its top.
        val f = androidx.compose.ui.geometry.Rect(box.left + (box.width - 160f) / 2f, box.top, box.left + (box.width + 160f) / 2f, box.bottom)
        assertTrue("the plate and the label (${f.height})", f.height > 160f + 8f + 10f && f.height < 160f + 8f + 30f)
        val (plate, module) = qrColours(m.part("QrCode", "plate", emptyMap()).style.background!!, m.part("QrCode", "module", emptyMap()).color!!)
        assertTrue("the plate is the light one", plate.luminance() > module.luminance())
        val matrix = QrMatrix.of("https://exponential.at", "M")!!
        val px = capture("qr-code")
        val d = density
        val module1 = 160f / (matrix.size + 8)
        fun at(mx: Float, my: Float) = px.getPixel(((f.left + mx * module1) * d).roundToInt(), ((f.top + my * module1) * d).roundToInt())
        assertTrue("the quiet zone is the plate", near(at(2f, 2.5f), plate.toArgb()))
        assertTrue("the finder's corner module is dark", near(at(4.5f, 4.5f), module.toArgb()))
        assertTrue("the finder's light ring is the plate", near(at(5.5f, 5.5f), plate.toArgb()))
        assertEquals(1, compose.onAllNodes(hasContentDescription("Scan to pair")).fetchSemanticsNodes().size)
    }

    /** No host camera = the placeholder (`Camera unavailable` in the name); a host feed reports each distinct value once. */
    @Test
    fun aQrScannerShowsThePlaceholderWithoutACameraAndReportsDistinctValues() {
        val plain = render("""{"id": "s", "component": "QrScanner", "props": {"hint": "Point at the code"}}""")
        val f = plain.frame(plain.idx("s"))
        assertTrue("as wide as the surface (${f.width})", f.width >= 280f)
        assertTrue("the viewfinder and the hint (${f.height})", f.height > 280f + 12f)
        val name = "${plain.string("scanQrCode")}, ${plain.string("cameraUnavailable")}"
        assertEquals(1, compose.onAllNodes(hasContentDescription(name)).fetchSemanticsNodes().size)
    }

    @Test
    fun aHostCameraFiresScannedOncePerDistinctValue() {
        val host = RecordingHost()
        val camera = object : QrScannerHost {
            @Composable
            override fun Feed(formats: List<String>, report: (String) -> Unit, modifier: Modifier) {
                LaunchedEffect(Unit) {
                    report("pair:abc")
                    report("pair:abc")
                    report("pair:def")
                }
                Box(modifier.background(Color.DarkGray))
            }
        }
        val m = render("""{"id": "s", "component": "QrScanner", "on": {"scanned": {"event": {"name": "scanned"}}}}""", host) { content ->
            CompositionLocalProvider(LocalQrScanner provides camera) { content() }
        }
        compose.waitForIdle()
        assertEquals(listOf("scanned", "scanned"), host.actions.map { it.name })
        val values = host.actions.map { a -> (a.payload ?: a.context)["value"]?.string ?: a.context["value"]?.string }
        assertEquals(listOf("pair:abc", "pair:def"), values)
        assertEquals(1, compose.onAllNodes(hasContentDescription(m.string("scanQrCode"))).fetchSemanticsNodes().size)
    }

    /** VoiceWave `bars`: the recipe box (156 × 40 at lg) with primary bars at the centre, the background between them at the ends. */
    @Test
    fun aVoiceWaveBarsMeterPaintsItsBars() {
        val m = render("""{"id": "w", "component": "VoiceWave", "props": {"variant": "bars", "activity": "speaking", "level": 1, "size": "lg", "label": "Voice"}}""")
        val f = m.frame(m.idx("w"))
        assertEquals(156f, f.width, 0.01f)
        assertEquals(40f, f.height, 0.01f)
        val px = capture("voice-wave-bars")
        val d = density
        val bar = m.part("VoiceWave", "bar", mapOf("activity" to JsonValue.Str("speaking"), "size" to JsonValue.Str("lg"), "variant" to JsonValue.Str("bars"))).color!!
        // Bar 10 (x = 80..84) at the vertical centre.
        assertTrue("a bar at the centre", near(px.getPixel(((f.left + 82f) * d).roundToInt(), ((f.top + 20f) * d).roundToInt()), bar.toArgb(), 16))
        assertTrue("the gap between bars is the background", near(px.getPixel(((f.left + 7f) * d).roundToInt(), ((f.top + 20f) * d).roundToInt()), m.color("background")!!.toArgb(), 16))
        assertEquals(1, compose.onAllNodes(hasContentDescription("Voice, speaking")).fetchSemanticsNodes().size)
    }

    /** THE rail: a vertical `bar` is 72 wide, items × 56 tall; a count reads after the label; 0 hides, above 99 = `99+`. */
    @Test
    fun theRailStacksItsItemsAndBadgesTheCounts() {
        val m = render("""{"id": "r", "component": "Segmented", "props": {"variant": "bar", "orientation": "vertical", "value": "chats", "items": [{"value": "chats", "label": "Chats", "icon": "message-circle", "count": 3}, {"value": "calls", "label": "Calls", "icon": "phone", "count": 0}, {"value": "inbox", "label": "Inbox", "icon": "inbox", "count": 120}]}}""")
        val f = m.frame(m.idx("r"))
        assertEquals(72f, f.width, 0.01f)
        assertEquals(168f, f.height, 0.01f)
        val items = m.node(m.idx("r"))!!.props["items"]!!.array!!
        assertEquals(listOf("3", null, "99+"), items.map { segmentedBadge(it) })
        assertEquals(1, compose.onAllNodes(hasContentDescription("Chats, 3")).fetchSemanticsNodes().size)
        assertEquals(1, compose.onAllNodes(hasContentDescription("Calls")).fetchSemanticsNodes().size)
        assertEquals(1, compose.onAllNodes(hasContentDescription("Inbox, 99+")).fetchSemanticsNodes().size)
        assertNull(segmentedBadge(JsonValue.parse("""{"label": "x"}""")))
    }

    @Test
    fun uiSerifIsTheSystemSerif() {
        val shaper = robolectricShaper()
        assertEquals(FontFamily.Serif, shaper.family("ui-serif"))
        assertEquals(FontFamily.Monospace, shaper.family("ui-monospace"))
    }
}
