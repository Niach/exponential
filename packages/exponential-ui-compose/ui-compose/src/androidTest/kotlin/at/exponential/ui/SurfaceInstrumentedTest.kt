package at.exponential.ui

import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeRight
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.ClosureHost
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.host.SurfaceInputEvent
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.sliderValue
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CopyOnWriteArrayList

/**
 * VAPP-103: the Compose painter on a real device / emulator (CI:
 * `exponential-ui.yml` `compose-emulator`). A surface renders, a host-owned
 * field takes typed text with its `type`'s IME, a Slider drags through its
 * real (narrow, continuous) range, and TalkBack reads roles and names.
 */
@RunWith(AndroidJUnit4::class)
class SurfaceInstrumentedTest {
    @get:Rule
    val compose = createComposeRule()

    private val actions = CopyOnWriteArrayList<SurfaceActionEvent>()
    private val inputs = CopyOnWriteArrayList<SurfaceInputEvent>()

    private fun model(tree: String): SurfaceModel {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        ExponentialUi.appContext = context.applicationContext
        val density = context.resources.displayMetrics.density
        val fallback = TextShaper(
            TextMeasurer(createFontFamilyResolver(context), Density(density, 1f), LayoutDirection.Ltr),
            density,
        ) { null }
        val m = SurfaceModel(
            id = "instrumented",
            options = SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Light),
            host = ClosureHost(actions = { actions += it }, inputs = { inputs += it }),
            shaper = { fallback },
        )
        m.setNested(tree)
        compose.setContent { ExponentialSurface(m) }
        compose.waitForIdle()
        return m
    }

    private fun stack(vararg children: String) =
        """{"id":"root","component":"Stack","props":{"gap":"md"},"style":{"padding":16},"children":[${children.joinToString(",")}]}"""

    @Test
    fun aSurfaceRendersAndAPressReachesTheHost() {
        model(
            stack(
                """{"id":"title","component":"Heading","props":{"text":"Hello device","level":"h2"}}""",
                """{"id":"save","component":"Button","props":{"label":"Save"},"on":{"press":{"event":{"name":"save"}}}}""",
            ),
        )
        compose.onAllNodes(hasText("Hello device", substring = true) or hasContentDescription("Hello device")).onFirst().assertIsDisplayed()
        compose.onNode(hasText("Save") or hasContentDescription("Save")).performClick()
        compose.waitUntil(5_000) { actions.any { it.name == "save" } }
        assertEquals("save", actions.first().componentId)
    }

    @OptIn(ExperimentalTestApi::class)
    @Test
    fun typingIntoAnEmailFieldReachesTheHost() {
        model(stack("""{"id":"email","component":"Input","props":{"label":"Email","name":"email","type":"email","placeholder":"you@example.com"}}"""))
        val matcher = hasSetTextAction() and hasContentDescription("Email")
        compose.waitUntilAtLeastOneExists(matcher, 10_000)
        val field = compose.onNode(matcher)
        field.performClick()
        field.performTextInput("ada@example.com")
        compose.waitUntil(5_000) { inputs.any { it.name == "email" && it.value.displayText == "ada@example.com" } }
        assertEquals("ada@example.com", field.fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text)
    }

    @Test
    fun aSliderDragsThroughANarrowContinuousRange() {
        val m = model(stack("""{"id":"vol","component":"Slider","props":{"label":"Opacity","min":0,"max":0.5,"step":0,"value":0}}"""))
        val slider = SemanticsMatcher.keyIsDefined(SemanticsProperties.ProgressBarRangeInfo) and hasContentDescription("Opacity")
        val info = compose.onNode(slider, useUnmergedTree = true).fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo]
        assertEquals("the platform range is the author's, not min..min+1", 0.5f, info.range.endInclusive, 1e-4f)
        compose.onNode(slider, useUnmergedTree = true).performTouchInput { swipeRight(startX = left + 2f, endX = centerX) }
        compose.waitForIdle()
        val v = m.sliderValue(m.indexOf("vol.track")!!)
        assertTrue("dragged into the range: $v", v > 0.05 && v < 0.5)
        // Continuous: no snap to a whole step.
        assertTrue("not snapped to an integer step: $v", v != Math.rint(v))
    }

    @Test
    fun talkBackReadsRolesAndNames() {
        model(
            stack(
                """{"id":"save","component":"Button","props":{"label":"Save"}}""",
                """{"id":"agree","component":"Checkbox","props":{"label":"Agree","checked":true}}""",
                """{"id":"deco","component":"Image","props":{"src":"https://example.com/x.png"}}""",
            ),
        )
        val button = compose.onNode(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Button) and (hasText("Save") or hasContentDescription("Save"))).fetchSemanticsNode()
        assertEquals(Role.Button, button.config.getOrNull(SemanticsProperties.Role))
        val toggles = compose.onAllNodes(SemanticsMatcher.expectValue(SemanticsProperties.ToggleableState, ToggleableState.On), useUnmergedTree = true).fetchSemanticsNodes()
        assertTrue("the checked Checkbox reads as on", toggles.isNotEmpty())
        // An unnamed Image is decorative: nothing announces an English "image".
        assertEquals(0, compose.onAllNodes(hasContentDescription("image", ignoreCase = true)).fetchSemanticsNodes().size)
    }
}
