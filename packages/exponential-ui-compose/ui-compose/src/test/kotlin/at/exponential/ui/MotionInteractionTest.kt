package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.MotionDriver
import at.exponential.ui.ffi.MotionPlayer
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * EXP-1605 (eui-motion README › Interactivity): a Motion's state machine in
 * the Compose painter. A tap reaches the machine (`click` → state change →
 * `on.state {state}`), then `on.press`; `on.visible` once; a final state
 * and a finished play send `on.complete` once per state; bound inputs drive
 * the states.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class MotionInteractionTest {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun fixture(id: String): Pair<JsonValue, JsonValue> {
        Fixtures.require()
        val f = Fixtures.json("motion-state.json")
        return f["lottie"]!! to f["cases"]!!.array!!.first { it["id"]?.string == id }["machine"]!!
    }

    private fun on(vararg events: String): String =
        events.joinToString(",", "{", "}") { """"$it":{"event":{"name":"$it"}}""" }

    private fun render(json: String, host: RecordingHost): SurfaceModel {
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("motion", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent { Box(Modifier.fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        return m
    }

    private fun motion(machine: JsonValue, lottie: JsonValue, on: String) =
        """{"id":"col","component":"Stack","children":[{"id":"m","component":"Motion","props":{"lottie":${lottie.json},"stateMachine":${machine.json},"size":[160,160],"label":"Toggle"},"on":$on}]}"""

    @Test
    fun aTapTogglesTheMachineThenPresses() {
        val (lottie, machine) = fixture("toggle-on-click")
        val host = RecordingHost()
        render(motion(machine, lottie, on("press", "state", "visible", "hover")), host)
        assertEquals(listOf("visible", "state"), host.actions.map { it.name })
        assertEquals("off", host.actions.last().context["state"]?.string)
        val node = compose.onNodeWithContentDescription("Toggle")
        assertEquals(Role.Button, node.fetchSemanticsNode().config.getOrNull(SemanticsProperties.Role))
        host.actions.clear()
        node.performClick()
        compose.waitForIdle()
        assertEquals(listOf("state", "press"), host.actions.map { it.name })
        assertEquals("on", host.actions.first().context["state"]?.string)
        host.actions.clear()
        node.performClick()
        compose.waitForIdle()
        assertEquals("off", host.actions.first { it.name == "state" }.context["state"]?.string)
    }

    @Test
    fun aMachineWithoutPressTakesThePointerItself() {
        val (lottie, machine) = fixture("toggle-on-click")
        val host = RecordingHost()
        val m = render(motion(machine, lottie, on("state")), host)
        assertTrue(!m.node(m.indexOf("m")!!)!!.pressable)
        host.actions.clear()
        compose.onNodeWithContentDescription("Toggle").performTouchInput { click() }
        compose.waitForIdle()
        assertEquals(listOf("state"), host.actions.map { it.name })
        assertEquals("on", host.actions.first().context["state"]?.string)
    }

    @Test
    fun inputsDriveTheMachineAndAFinalStateCompletesOnce() {
        val (lottie, machine) = fixture("progress-thresholds")
        val host = RecordingHost()
        val m = makeModel(host = host)
        m.setNested("""{"id":"m","component":"Motion","props":{"lottie":${lottie.json},"stateMachine":${machine.json}},"on":${on("state", "complete")}}""")
        val d = MotionDriver("m")
        d.attach(m)
        d.start(MotionPlayer(lottie.json, ""), machine.json)
        assertTrue(d.running)
        host.actions.clear()
        d.setInputs(JsonValue.parse("""{"level":60}"""))
        d.setInputs(JsonValue.parse("""{"level":60}"""))
        val before = d.generation
        d.setInputs(JsonValue.parse("""{"level":100}"""))
        assertTrue("the state changed: the clock restarts", d.generation > before)
        assertEquals(listOf("state", "state", "complete"), host.actions.map { it.name })
        assertEquals(listOf("high", "done", null), host.actions.map { it.context["state"]?.string })
        d.finished(d.generation)
        assertEquals("complete once per state", 3, host.actions.size)
    }
}
