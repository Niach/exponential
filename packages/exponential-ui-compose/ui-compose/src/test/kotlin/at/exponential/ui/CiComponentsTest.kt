package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextReplacement
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import com.github.takahirom.roborazzi.RoborazziOptions
import com.github.takahirom.roborazzi.RoborazziTaskType
import com.github.takahirom.roborazzi.ThresholdValidator
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
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

/**
 * #EXP-2204 (docs/ci-components-contract.md) on the Compose painter:
 * LogLines (toolbar, folding steps, the gutter anchor, search), JobGraph
 * (graph nodes, the phone list), the CodeBlock anchors, and the wrapped
 * `ansi` CodeBlock whose lines take their real height (they overlapped).
 * `EXPONENTIAL_UI_CI_PNG_DIR=<dir>` saves the specimens as painted
 * (`android-log-view.png`, `android-job-graph.png`, `android-code-block.png`).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h2400dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class CiComponentsTest {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private class CiHost : HostPlugin {
        val actions = ArrayList<SurfaceActionEvent>()
        override fun onAction(event: SurfaceActionEvent) {
            actions.add(event)
        }
        override fun icon(name: String, size: Float): (@Composable () -> Unit)? = CiTestIcons.icon(name)
        override fun resolveUrl(src: String): String = ""
    }

    private val density: Float get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun model(host: CiHost): SurfaceModel {
        val shaper = robolectricShaper(FontResolver { null }, density)
        return SurfaceModel("ci", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
    }

    private fun show(m: SurfaceModel) {
        compose.setContent {
            Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) {
                ExponentialSurface(m)
            }
        }
        compose.waitForIdle()
        assertEquals(m.issues.toString(), 0, m.issues.size)
    }

    private fun specimen(id: String): JsonValue =
        Fixtures.json("specimens.json")["specimens"]!!.array!!.first { it["id"]?.string == id }["node"]!!

    private fun save(name: String) {
        val dir = System.getenv("EXPONENTIAL_UI_CI_PNG_DIR")?.takeIf { it.isNotEmpty() } ?: return
        File(dir).mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(File(dir, name).path, RoborazziOptions(taskType = RoborazziTaskType.Record))
    }

    private fun payload(e: SurfaceActionEvent): JsonValue = e.payload ?: e.context

    /** A long ANSI line wrapped at 390: the bug's case (lines painted over each other). */
    private val wrapCase = """{"id":"ansi_wrap","component":"CodeBlock","props":{"language":"ansi","wrap":true,"lineNumbers":true,"title":"swift test · live","code":"$ swift test --parallel\n\u001b[32m✓\u001b[0m TrainStateTests.testSquashOrder (0.011s) and a long tail that has to wrap at a phone's width\n\u001b[33m!\u001b[0m TrainStateTests.testBisect \u001b[2mretried once: conflict on #212\u001b[0m (0.031s)\nExecuting 214 of 380 tests…"}}"""

    @Test
    fun aWrappedAnsiCodeBlockLineTakesItsRealHeight() {
        val m = model(CiHost())
        m.setNested("""{"id":"root","component":"Box","style":{"display":"flex","flexDirection":"column","padding":12},"children":[$wrapCase]}""")
        show(m)
        val lines = m.nodes.filter { it.ownerComponent == "CodeBlock" && it.part == "line" }.map { m.frame(it.index) }
        assertEquals(4, lines.size)
        val one = lines[0].height
        // The long second line wraps onto more lines and is that much taller…
        assertTrue("wrapped ${lines[1].height} vs one line $one", lines[1].height > one * 1.5f)
        // …and every line starts below the previous one: nothing overlaps.
        for (i in 1 until lines.size) assertTrue("line $i at ${lines[i].top} under ${lines[i - 1].bottom}", lines[i].top >= lines[i - 1].bottom - 0.5f)
        val code = m.nodes.filter { it.ownerComponent == "CodeBlock" && it.part == "code" }.map { m.frame(it.index) }
        for ((c, l) in code.zip(lines)) assertTrue("code ${c.height} fits its line ${l.height}", c.height <= l.height + 0.5f)
        val file = File("src/test/snapshots/images/ci/code-block-ansi-wrap.png")
        file.parentFile.mkdirs()
        compose.onNodeWithTag("shot").captureRoboImage(
            file.path,
            RoborazziOptions(
                taskType = if (recording || !file.exists()) RoborazziTaskType.Record else RoborazziTaskType.Verify,
                compareOptions = RoborazziOptions.CompareOptions(resultValidator = ThresholdValidator(0.005f)),
            ),
        )
    }

    @Test
    fun theLogViewFoldsSelectsAndSearches() {
        val host = CiHost()
        val m = model(host)
        m.setNested(specimen("exponential-ui-log-view").json)
        show(m)
        save("android-log-view.png")
        // Below md the core makes it `compact`: two toolbar rows ($control.input
        // 36 each, $spacing.sm 8 apart) and the body still maxLines lines tall.
        for (log in m.nodes.filter { it.component == "LogLines" }) {
            assertEquals(true, log.props["compact"]?.bool)
            val lines = log.props["maxLines"]?.number?.toFloat() ?: 24f
            assertEquals("two rows + gap + the body", 2 * 36f + 8f + 8f + lines * 22.75f + 2 * (8f + 1f), m.frame(log.index).height, 0.5f)
        }
        // The query `store` counts its matches (5, contract fixtures).
        compose.onAllNodesWithContentDescription("Search log", useUnmergedTree = true).fetchSemanticsNodes().let { assertEquals(2, it.size) }
        // A step row is a button named by its title and status.
        val unit = compose.onNodeWithContentDescription("Unit tests, Failed", useUnmergedTree = true)
        assertEquals(Role.Button, unit.fetchSemanticsNode().config[SemanticsProperties.Role])
        // A gutter number selects its line: `select {anchor: L9}`.
        // (the second log, a live one, has a line 9 too: the first is the failed job's)
        compose.onAllNodesWithContentDescription("Line 9", useUnmergedTree = true)[0].performClick()
        compose.waitForIdle()
        val anchor = host.actions.lastOrNull { it.name == "anchor" }
        assertTrue("actions ${host.actions.map { it.name }}", anchor != null)
        assertEquals("L9", payload(anchor!!)["anchor"]?.string)
        // Folding the failed step: `toggle {section: 3, open: false}`.
        unit.performClick()
        compose.waitForIdle()
        val fold = host.actions.lastOrNull { it.name == "fold" }
        assertTrue("actions ${host.actions.map { it.name }}", fold != null)
        assertEquals(3.0, payload(fold!!)["section"]?.number)
        assertEquals(false, payload(fold)["open"]?.bool)
        assertEquals("the folded step hides its lines", 1, compose.onAllNodesWithContentDescription("Line 9", useUnmergedTree = true).fetchSemanticsNodes().size)
        // Typing a query: `search {query}`.
        compose.onAllNodes(hasSetTextAction(), useUnmergedTree = true)[0].performTextReplacement("cart")
        compose.waitForIdle()
        val find = host.actions.lastOrNull { it.name == "find" }
        assertEquals("cart", find?.let { payload(it)["query"]?.string })
    }

    @Test
    @Config(qualifiers = "w1000dp-h1200dp-mdpi")
    fun theWideLogViewKeepsOneToolbarRow() {
        val m = model(CiHost())
        m.setNested(specimen("exponential-ui-log-view").json)
        show(m)
        for (log in m.nodes.filter { it.component == "LogLines" }) {
            assertEquals(false, log.props["compact"]?.bool)
            val lines = log.props["maxLines"]?.number?.toFloat() ?: 24f
            assertEquals("one row + gap + the body", 36f + 8f + lines * 22.75f + 2 * (8f + 1f), m.frame(log.index).height, 0.5f)
        }
    }

    @Test
    fun theJobGraphIsAListOnAPhoneAndSelects() {
        val host = CiHost()
        val m = model(host)
        m.setNested(specimen("exponential-ui-job-graph").json)
        show(m)
        save("android-job-graph.png")
        // The 3/5 matrix collapses into ONE row: name, status, count.
        val test = compose.onNodeWithContentDescription("test, Failed, 3/5", useUnmergedTree = true)
        val node = test.fetchSemanticsNode()
        assertEquals(Role.Button, node.config[SemanticsProperties.Role])
        assertEquals(true, node.config[SemanticsProperties.Selected])
        compose.onNodeWithContentDescription("deploy, Queued", useUnmergedTree = true).performClick()
        compose.waitForIdle()
        val open = host.actions.lastOrNull { it.name == "open_job" }
        assertEquals("deploy", open?.let { payload(it)["id"]?.string })
        // List rows are 40 tall.
        val graph = m.nodes.first { it.component == "JobGraph" }
        assertEquals(true, graph.props["compact"]?.bool)
        assertEquals("six rows, 40 tall", 6 * 40f, m.frame(graph.index).height, 0.5f)
    }

    @Test
    @Config(qualifiers = "w1000dp-h1200dp-mdpi")
    fun theWideJobGraphPlacesNodesInStageColumns() {
        val host = CiHost()
        val m = model(host)
        m.setNested(specimen("exponential-ui-job-graph").json)
        show(m)
        val graph = m.nodes.first { it.component == "JobGraph" }
        assertEquals(false, graph.props["compact"]?.bool)
        // stages build → test → deploy: 3 × 200 + 2 × 48 wide, 24 + 8 + 2 × 40 + 12 tall.
        assertEquals(124f, m.frame(graph.index).height, 0.5f)
        compose.onNodeWithContentDescription("e2e, Running", useUnmergedTree = true).performClick()
        compose.waitForIdle()
        assertEquals("e2e", host.actions.lastOrNull { it.name == "open_job" }?.let { payload(it)["id"]?.string })
        save("android-job-graph-wide.png")
    }

    @Test
    fun theCodeBlockAnchorsSelectAndBlameDraws() {
        val host = CiHost()
        val m = model(host)
        val cases = specimen("exponential-ui-code-block")["children"]!!.array!!
        val picked = cases.filter { c -> c["children"]?.array?.any { it["id"]?.string in setOf("c6_code_block_anchors", "c7_code_block_blame") } == true }
        assertEquals(2, picked.size)
        val wrap = JsonValue.parse(wrapCase)
        m.setNested(JsonValue.Obj(mapOf("id" to JsonValue.Str("root"), "component" to JsonValue.Str("Box"), "style" to JsonValue.parse("""{"display":"flex","flexDirection":"column","gap":16,"padding":12}"""), "children" to JsonValue.Arr(picked + wrap))).json)
        show(m)
        save("android-code-block.png")
        // The blame column names its first block's author.
        assertTrue(m.nodes.any { it.part == "blameAuthor" && it.props["text"]?.string == "Ada Lovelace" })
        // L9-L11 are selected (the `line` part's selected state).
        val selected = m.nodes.filter { it.ownerComponent == "CodeBlock" && it.part == "line" && it.props["selected"]?.bool == true }
        assertEquals(3, selected.size)
        // Pressing an anchored number selects its line.
        compose.onAllNodesWithContentDescription("Line 3", useUnmergedTree = true)[0].performClick()
        compose.waitForIdle()
        val anchor = host.actions.lastOrNull { it.name == "anchor" }
        assertEquals("L3", anchor?.let { payload(it)["anchor"]?.string })
    }
}
