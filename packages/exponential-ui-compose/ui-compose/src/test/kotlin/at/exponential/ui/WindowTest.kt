package at.exponential.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
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

/**
 * VAPP-290: the `Window` macro through the painter's Box / Text paths: a
 * 34 tall chrome strip (three 10 x 10 dots at the start, the title centred
 * in the window by the `end` spacer, hidden from TalkBack) over a body that
 * fills the rest and holds the one child; the root is a group named by the
 * title.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class WindowTest {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private val density: Float get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    private fun render(json: String): SurfaceModel {
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("win", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent {
            Box(Modifier.testTag("shot").fillMaxWidth().background(m.color("background") ?: Color.Black)) { ExponentialSurface(m) }
        }
        compose.waitForIdle()
        return m
    }

    private fun window(title: String?): String {
        val props = title?.let { """"props": {"title": "$it"}, """ } ?: ""
        return """{"id": "w", "component": "Window", $props"style": {"height": 200}, "children": [{"id": "kid", "component": "Text", "props": {"text": "hello"}}]}"""
    }

    @Test
    fun theChromeSitsOnTopWithThreeDotsAndACentredTitleOverTheBody() {
        val m = render(window("Terminal"))
        assertTrue("no Unknown node", m.nodes.none { it.component == "Unknown" })
        val root = m.frame(m.idx("w"))
        assertEquals(200f, root.height, 0.01f)
        val chrome = m.frame(m.idx("w.chrome"))
        val border = chrome.top - root.top
        assertTrue("the hairline border ($border)", border in 0f..1.01f)
        assertEquals("34 tall", 34f, chrome.height, 0.01f)
        assertEquals(root.left + border, chrome.left, 0.01f)
        assertEquals(root.right - border, chrome.right, 0.01f)

        val dots = (0 until 3).map { m.frame(m.idx("w.chrome.dots.dot.$it")) }
        assertNull(m.indexOf("w.chrome.dots.dot.3"))
        dots.forEach {
            assertEquals(10f, it.width, 0.01f)
            assertEquals(10f, it.height, 0.01f)
            // Centred in the chrome's content box (above its hairline bottom border).
            assertEquals("centred in the chrome", chrome.top + (chrome.height - border) / 2f, it.center.y, 0.01f)
        }
        assertEquals("border + padding md", root.left + 1f + 12f, dots[0].left, 0.01f)
        assertEquals("gap 6", dots[0].right + 6f, dots[1].left, 0.01f)
        assertEquals("gap 6", dots[1].right + 6f, dots[2].left, 0.01f)

        val title = m.frame(m.idx("w.chrome.title"))
        assertEquals("the title is centred in the window", root.center.x, title.center.x, 0.5f)
        assertEquals("Terminal", m.node(m.idx("w.chrome.title"))!!.props["text"]?.string)

        val body = m.frame(m.idx("w.body"))
        assertEquals("the body starts under the chrome", chrome.bottom, body.top, 0.01f)
        assertEquals("the body fills the rest", root.bottom - border, body.bottom, 0.01f)
        assertEquals(chrome.left, body.left, 0.01f)
        assertEquals(chrome.right, body.right, 0.01f)
        assertEquals(listOf("kid"), m.children[m.idx("w.body")].map { m.node(it)!!.id })
        val kid = m.frame(m.idx("kid"))
        assertTrue("the child inside the body ($kid in $body)", kid.left >= body.left && kid.top >= body.top && kid.right <= body.right && kid.bottom <= body.bottom)
    }

    @Test
    fun theWindowIsAGroupNamedByItsTitleAndTheChromeIsHidden() {
        render(window("Terminal"))
        // A Text leaf is named by its text: ONE node says "Terminal" = the root group, the chrome's title is hidden.
        val named = compose.onAllNodes(hasContentDescription("Terminal")).fetchSemanticsNodes()
        assertEquals(1, named.size)
        assertTrue("the group = the whole window (${named[0].size})", named[0].size.height >= (200f * density).toInt() - 1)
        assertTrue(compose.onAllNodes(hasText("Terminal")).fetchSemanticsNodes().isEmpty())
        assertEquals("the body's child reads", 1, compose.onAllNodes(hasContentDescription("hello")).fetchSemanticsNodes().size)
    }

    @Test
    fun anUntitledWindowHasNoTitle() {
        val m = render(window(null))
        assertTrue(m.nodes.none { it.component == "Unknown" })
        assertNull(m.indexOf("w.chrome.title"))
        assertEquals(3, (0 until 3).count { m.indexOf("w.chrome.dots.dot.$it") != null })
        assertEquals(34f, m.frame(m.idx("w.chrome")).height, 0.01f)
        assertTrue(compose.onAllNodes(hasContentDescription("Terminal")).fetchSemanticsNodes().isEmpty())
    }
}
