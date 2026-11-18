package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.compose.LeafImages
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.ImageTarget
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.originalImageSrc
import at.exponential.ui.primitives.ImageSource
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownBlockKind
import at.exponential.ui.primitives.MarkdownHooks
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * #EXP-1926 / #EXP-1948 (image-actions.json): a rendered genUI picture's
 * long press opens the HOST's image menu (`pressMenu`: never merged with an
 * enclosing context Menu; a picture not loaded falls through to it), and a
 * source the host's rule does not load paints a link (never fetched) or its
 * text.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ImageActionsTest {
    @get:Rule
    val compose = createComposeRule()

    private val density get() = RuntimeEnvironment.getApplication().resources.displayMetrics.density

    private class MenuHost(private val rule: (String, String) -> ImageSource? = { _, _ -> null }) : HostPlugin {
        val urls = ArrayList<String>()
        val opened = ArrayList<String>()
        override fun imageSource(src: String, alt: String): ImageSource? = rule(src, alt)
        override val hasImageMenu: Boolean get() = true
        override fun openUrl(url: String) {
            urls.add(url)
        }

        @Composable
        override fun ImageMenu(target: ImageTarget, expanded: Boolean, onDismiss: () -> Unit) {
            if (expanded) {
                opened.add(target.src)
                BasicText("image-menu", Modifier.testTag("image-menu"))
            }
        }
    }

    private fun render(json: String, host: HostPlugin): SurfaceModel {
        Fixtures.require()
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("images", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(json)
        compose.setContent { Box(Modifier.testTag("shot").fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        return m
    }

    private fun longPress(m: SurfaceModel, id: String) {
        val f = m.frame(m.indexOf(id)!!)
        val d = density
        compose.onNodeWithTag("shot").performTouchInput { longClick(Offset(f.center.x * d, f.center.y * d)) }
        compose.waitForIdle()
    }

    private fun image(src: String) = """{"id":"pic","component":"Image","props":{"src":"$src","alt":"Shot"},"style":{"width":200,"height":120}}"""

    private fun underMenu(child: String) = """{"id":"root","component":"Box","children":[{"id":"ctx","component":"Menu","props":{"openOn":"contextmenu","items":[{"label":"Retry","value":"retry"}]},
        "children":[{"id":"bubble","component":"Box","style":{"padding":8},"children":[$child]}]}]}"""

    @Test
    fun aRenderedPictureOpensTheHostImageMenu() {
        LeafImages.seed("https://example.com/a.png", ImageBitmap(400, 240))
        val host = MenuHost()
        val m = render("""{"id":"root","component":"Box","children":[${image("https://example.com/a.png")}]}""", host)
        longPress(m, "pic")
        assertEquals("pic", m.imageMenuFor)
        assertEquals("https://example.com/a.png", host.opened.lastOrNull())
        compose.onNodeWithTag("image-menu", useUnmergedTree = true).assertExists()
    }

    @Test
    fun onAPictureInsideAContextMenuOnlyTheImageMenuOpens() {
        LeafImages.seed("https://example.com/b.png", ImageBitmap(400, 240))
        val host = MenuHost()
        val m = render(underMenu(image("https://example.com/b.png")), host)
        longPress(m, "pic")
        assertEquals("pic", m.imageMenuFor)
        assertTrue("never merged with the bubble's menu", m.layers.isEmpty())
    }

    @Test
    fun aPictureNotLoadedFallsThroughToTheOwnerMenu() {
        val host = MenuHost()
        val m = render(underMenu(image("https://example.com/never-loaded.png")), host)
        longPress(m, "pic")
        assertNull(m.imageMenuFor)
        assertEquals(listOf("ctx"), m.layers.map { it.owner })
    }

    @Test
    fun anotherWebsitesPictureIsALinkAndNeverFetched() {
        val src = "https://other.example/cat.png"
        val host = MenuHost { s, _ -> if (s == src) ImageSource.Link(s, "Image: other.example") else null }
        val m = render("""{"id":"root","component":"Box","children":[${image(src)}]}""", host)
        compose.onNodeWithText("Image: other.example", useUnmergedTree = true).assertExists().performClick()
        compose.waitForIdle()
        assertEquals(listOf(src), host.urls)
        assertNull("never fetched", LeafImages.cached(src))
        assertTrue(!LeafImages.hasFailed(src))
        longPress(m, "pic")
        assertNull("a link has no image menu", m.imageMenuFor)
    }

    @Test
    fun aRefusedSourceIsItsAltText() {
        val host = MenuHost { _, alt -> ImageSource.Text(alt) }
        render("""{"id":"root","component":"Box","children":[${image("javascript:alert(1)")}]}""", host)
        compose.onNodeWithText("Shot", useUnmergedTree = true).assertExists()
    }

    @Test
    fun markdownImagesResolveThroughTheSourceRule() {
        val rule: (String, String) -> ImageSource = { src, alt ->
            when {
                src.startsWith("/api/") -> ImageSource.Load
                src.startsWith("https://") -> ImageSource.Link(src, alt.ifEmpty { "Image: example.com" })
                else -> ImageSource.Text(alt)
            }
        }
        val blocks = Markdown.prepare(
            "![](/api/attachments/x)\n\n![](https://example.com/c.png)\n\n![Alt](file:///etc/passwd)\n\n![](cat.png)",
            MarkdownHooks(imageSource = rule),
        )
        assertTrue(blocks[0].kind is MarkdownBlockKind.Image)
        assertEquals(MarkdownBlockKind.Paragraph, blocks[1].kind)
        assertEquals("https://example.com/c.png", blocks[1].inlines.single().link)
        assertEquals("Image: example.com", blocks[1].inlines.single().text)
        assertEquals("Alt", blocks[2].inlines.single().text)
        assertNull(blocks[2].inlines.single().link)
        assertEquals("an empty alt draws nothing", 3, blocks.size)
    }

    @Test
    fun theCopyFetchDropsTheThumbnailWidth() {
        assertEquals("/api/attachments/x", originalImageSrc("/api/attachments/x?w=480"))
        assertEquals("/api/attachments/x?v=2#f", originalImageSrc("/api/attachments/x?w=480&v=2#f"))
        assertEquals("data:image/png;base64,AA?w=1", originalImageSrc("data:image/png;base64,AA?w=1"))
    }
}
