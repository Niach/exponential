package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performMouseInput
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.model.SurfaceSettings
import at.exponential.ui.model.hover
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The facade's VAPP-100 painter asks as Compose uses them: the leaf's text
 * style (tracking, case, italics, inherited like CSS), number-valued text,
 * the effective theme, the host hover and `hoverStyled`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PainterAsksTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aLeafMeasuresWithTheTrackingCaseAndItalicsItInherits() {
        val m = makeModel(theme = "neutral")
        m.setNested(
            """{"id":"row","component":"Box","style":{"flexDirection":"row","alignItems":"flex-start","textTransform":"uppercase","letterSpacing":2,"fontStyle":"italic"},
               "children":[{"id":"t","component":"Text","props":{"text":"abc"}}]}""",
        )
        val i = m.indexOf("t")!!
        val ts = m.textStyle(i)
        assertEquals("uppercase", ts.textTransform)
        assertEquals(2f, ts.letterSpacing!!, 0.001f)
        assertTrue("italic inherits", ts.italic)
        val shaper = m.textShaper()
        val want = shaper.maxContent("ABC", ts)
        assertTrue("tracking widens the line: $want", want > shaper.maxContent("ABC", ts.copy(letterSpacing = null)))
        assertEquals("measured as painted (case + tracking)", want, m.frame(i).width, 1f)
    }

    @Test
    fun aNumberInATextPropShowsAsItsDisplayString() {
        val m = makeModel(theme = "neutral")
        m.setNested(
            """{"id":"row","component":"Box","style":{"flexDirection":"row","alignItems":"flex-start"},
               "children":[{"id":"n","component":"Text","props":{"text":412}}]}""",
        )
        val i = m.indexOf("n")!!
        assertEquals(m.textShaper().maxContent("412", m.textStyle(i)), m.frame(i).width, 1f)
        assertEquals("412", m.node(i)!!.accessibilityLabel)
    }

    @Test
    fun tokensAndPartsComeFromTheEffectiveTheme() {
        val m = makeModel(theme = "neutral")
        m.setNested("""{"id":"b","component":"Button","props":{"label":"Go","variant":"outline"}}""")
        val plainInput = m.control("input", 0f)
        assertEquals(plainInput, m.theme!!.control("input", 0f), 0f)
        m.setSettings(SurfaceSettings(density = "compact", contrast = "high"))
        val eff = m.effectiveTheme!!
        assertNotEquals("a new handle per density / contrast", m.theme, eff)
        assertTrue("density scales the controls", m.control("input", 0f) < plainInput)
        val b = m.indexOf("b")!!
        val part = m.part("Button", "root", m.node(b)!!.props)
        assertEquals("a painter's part = the core's visual", m.style(b).borderColor, part.style.borderColor)
    }

    @Test
    fun hoverReachesTheCoreAndRestylesTheNode() {
        val m = makeModel(theme = "neutral", mode = at.exponential.ui.theme.Mode.Light)
        m.setNested("""{"id":"pill","component":"Pill","props":{"label":"All","pressable":true}}""")
        val i = m.indexOf("pill")!!
        val rest = m.style(i).background
        m.hover("pill", true)
        assertNotEquals("the recipe's hover look", rest, m.style(i).background)
        m.hover("pill", false)
        assertEquals(rest, m.style(i).background)
    }

    @OptIn(ExperimentalTestApi::class)
    @Test
    fun aHoverStyleOnAPlainNodeFiresUnderAMouse() {
        val m = makeModel(theme = "neutral", mode = at.exponential.ui.theme.Mode.Light, fixed = true)
        m.setNested(
            """{"id":"root","component":"Box","style":{"alignItems":"flex-start"},"children":[
               {"id":"tile","component":"Box","style":{"width":100,"height":100,"backgroundColor":"${'$'}color.muted",":hover":{"opacity":0.5}}},
               {"id":"plain","component":"Box","style":{"width":100,"height":100}}]}""",
        )
        val tile = m.indexOf("tile")!!
        assertTrue("the core marks a :hover style", m.node(tile)!!.hoverStyled)
        assertFalse("a plain box is not tracked", m.node(m.indexOf("plain")!!)!!.hoverStyled)
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        assertEquals(null, m.style(tile).opacity)
        val f = m.frame(tile)
        val d = compose.density.density
        compose.onNodeWithTag("shot").performMouseInput { moveTo(Offset(f.center.x * d, f.center.y * d)) }
        compose.waitForIdle()
        assertEquals("the :hover block applies under the mouse", 0.5f, m.style(tile).opacity!!, 0.001f)
        compose.onNodeWithTag("shot").performMouseInput { moveTo(Offset(f.center.x * d, (f.bottom + 50f) * d)) }
        compose.waitForIdle()
        assertEquals("and leaves with it", null, m.style(tile).opacity)
    }
}
