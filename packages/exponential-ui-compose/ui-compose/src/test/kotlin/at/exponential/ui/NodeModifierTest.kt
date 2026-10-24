package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The host's paint-only node hook ([SurfaceModel.nodeModifier]): every node
 * view asks it once per index and wears what it returns, outermost — the
 * chat surface card staggers the root's first-level children through it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NodeModifierTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun theHostModifierReachesEveryFirstLevelChild() {
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("hook", SurfaceOptions(theme = ThemeHandle.builtin("exponential"), mode = Mode.Dark), RecordingHost(), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(
            """{"id": "root", "component": "Stack", "props": {"gap": "sm"}, "children": [
              {"id": "a", "component": "Text", "props": {"text": "One"}},
              {"id": "b", "component": "Text", "props": {"text": "Two"}},
              {"id": "c", "component": "Text", "props": {"text": "Three"}}
            ]}""",
        )
        val asked = sortedSetOf<Int>()
        m.nodeModifier = { index ->
            asked.add(index)
            val node = m.node(index)
            if (node?.parent == 0) Modifier.testTag("child-${node.id}").graphicsLayer { alpha = 0.5f } else null
        }
        compose.setContent { Box(Modifier.fillMaxWidth()) { ExponentialSurface(m) } }
        compose.waitForIdle()
        val root = m.indexOf("root")!!
        assertEquals(0, root)
        val children = m.node(root)!!.children
        assertEquals(listOf("a", "b", "c"), children.map { m.node(it)!!.id })
        // Asked for the root and each child; the children wear the hook.
        assertEquals((listOf(root) + children).toSortedSet(), asked.filter { it == root || it in children }.toSortedSet())
        for (id in listOf("a", "b", "c")) compose.onNodeWithTag("child-$id").assertExists()
    }
}
