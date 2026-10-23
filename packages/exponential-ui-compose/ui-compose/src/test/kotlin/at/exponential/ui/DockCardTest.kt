package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.click
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.modalLayers
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * N-12: the `DockCard` macro on the Compose painter: the core expands it
 * to a pressable trigger row over a bottom `Drawer` whose `open` the
 * trigger's `$set` writes; the specimen (open, with a footer slot) and a
 * closed card opened by a tap both paint the sheet's body and footer.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
class DockCardTest {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    /** The painter's own measurer; the indices composed land in the returned trace. */
    private fun render(json: String, overlays: OverlayPresentation, data: JsonValue? = null): Pair<SurfaceModel, MutableSet<Int>> {
        Fixtures.require()
        val options = SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark, overlays = overlays)
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("dock", options, RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        val trace = LinkedHashSet<Int>()
        m.paintTrace = trace
        val out = m.setNested(json)
        assertEquals("[]", out.issuesJson)
        if (data != null) m.setData("/ui", data)
        // A phone's window: the sheet's bottom placement and max height read it.
        m.setViewport(390f, 800f)
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        return m to trace
    }

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    private fun specimen(): String =
        Fixtures.json("specimens.json")["specimens"]!!.array!!.first { it["id"]?.string == "exponential-ui-dock-card" }["node"]!!.json

    private val closed = """{"id":"root","component":"Box","children":[{"id":"dc","component":"DockCard",
        "props":{"label":"Plan ready","title":"Fold the composer","icon":"coding-plan","tone":"primary","open":{"path":"/ui/dockOpen"}},
        "children":[{"id":"dc-body","component":"Text","props":{"text":"The plan body"}}],
        "slots":{"footer":{"id":"dc-yes","component":"Button","props":{"label":"Approve"}}}}]}"""

    @Test
    fun theSpecimenPaintsTheTriggerAndTheOpenSheetWithItsFooter() {
        val (m, painted) = render(specimen(), OverlayPresentation.Painted)
        assertTrue(m.nodes.none { it.component == "Unknown" })
        for (part in listOf("trigger", "trigger.icon", "trigger.body.header.label", "trigger.body.header.caption", "trigger.body.header.meta", "trigger.body.row.title", "trigger.body.row.chevron")) {
            assertTrue("c0-dock-card.$part painted", m.idx("c0-dock-card.$part") in painted)
        }
        val drawer = m.layers.single()
        assertEquals("Drawer", drawer.kind)
        assertTrue(drawer.isModal)
        for (id in listOf("c0-p", "c0-footer", "c0-no", "c0-yes")) {
            val n = m.node(m.idx(id))!!
            assertEquals("$id in the sheet's layer", drawer.layer, n.layer)
            assertTrue("$id painted", n.index in painted)
        }
        // The footer sits under the body.
        assertTrue(m.frame(m.idx("c0-yes")).top >= m.frame(m.idx("c0-p")).bottom)
    }

    @Test
    fun aTapOnTheTriggerOpensTheBottomSheet() {
        // `$set` writes back to a BOUND `open` only (the kitchen sink's `/ui/dockOpen`).
        val (m, _) = render(closed, OverlayPresentation.Native, JsonValue.Obj(mapOf("dockOpen" to JsonValue.Bool(false))))
        assertTrue(m.layers.isEmpty())
        val f = m.frame(m.idx("dc.trigger"))
        compose.onNodeWithTag("shot").performTouchInput { click(Offset(f.center.x * density, f.center.y * density)) }
        compose.waitForIdle()
        val drawer = m.modalLayers.single()
        assertEquals("Drawer", drawer.kind)
        val yes = m.node(m.idx("dc-yes"))!!
        assertEquals(drawer.layer, yes.layer)
        assertFalse(yes.hidden)
        assertNotNull(m.node(m.idx("dc-body")))
        compose.onNode(hasContentDescription("Approve"), useUnmergedTree = true).assertExists()
    }
}
