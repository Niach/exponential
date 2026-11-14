package at.exponential.ui

import at.exponential.ui.ffi.listDragStyle
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.isListDragRow
import at.exponential.ui.model.isReorderRow
import at.exponential.ui.model.listDrag
import at.exponential.ui.model.press
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * EXP-1832 (fixtures/list-reorder.json): a `reorderable` List's row drags
 * through the core ([listDrag] = what the long-press drag forwards): the
 * paint follows the pointer, the release fires `move {id, before | after |
 * into}` and its press is swallowed; a cancel fires nothing.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ListReorderTest {
    private class Host : HostPlugin {
        val actions = ArrayList<SurfaceActionEvent>()
        override fun onAction(event: SurfaceActionEvent) { actions.add(event) }
    }

    private val tree = """{"id":"root","component":"Box","children":[
        {"id":"l","component":"List","props":{"reorderable":true,"maxDepth":1},"template":{"component":"row","path":"/items","key":"id"},"on":{"move":{"event":{"name":"move"}}}},
        {"id":"row","component":"Box","style":{"height":30,"flexShrink":0},"on":{"press":{"event":{"name":"open"}}},"children":[{"id":"label","component":"Text","props":{"text":{"@path":"id"}}}]}
    ]}"""

    private fun model(host: Host): SurfaceModel {
        val m = SurfaceModel("lr", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
        m.fixedMeasure = true
        m.setNested(tree)
        m.setViewport(300f, 800f)
        m.setData("/items", JsonValue.Arr(listOf("a", "b", "c").map { JsonValue.Obj(mapOf("id" to JsonValue.Str(it))) }))
        return m
    }

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    @Test
    fun aRowDragsAndTheReleaseFiresMove() {
        val host = Host()
        val m = model(host)
        val a = m.idx("row.a")
        val c = m.idx("row.c")
        assertTrue(m.isReorderRow(a))
        assertFalse("a node inside a row is no row", m.isReorderRow(m.idx("label.a")))
        assertFalse(m.isReorderRow(m.idx("l")))

        val fa = m.frame(a)
        val fc = m.frame(c)
        m.listDrag(a, "start", fa.left + 10f, fa.center.y)
        m.listDrag(a, "move", fa.left + 40f, fc.bottom - 4f)
        val paint = m.listDragPaints.firstOrNull()
        assertNotNull("a live drag paints", paint)
        paint!!
        assertEquals("a", paint.key)
        assertEquals(a, paint.row.toInt())
        assertTrue("the dragged row dims", m.isListDragRow(a))
        assertFalse(m.isListDragRow(c))
        assertEquals("c", paint.dropKey)
        val position = paint.dropPosition!!
        if (position == "into") {
            assertNotNull(paint.into)
        } else {
            assertNotNull("before / after draw the line", paint.line)
            assertNotNull("and its ring", paint.ring)
        }
        assertEquals("the lifted copy sits right of the row", fa.left + listDragStyle().liftOffset, paint.lift.x, 0.01f)

        m.listDrag(a, "end", fa.left + 40f, fc.bottom - 4f)
        assertTrue("nothing painted after the drop", m.listDragPaints.isEmpty())
        val move = host.actions.last()
        assertEquals("move", move.name)
        val context = move.payload ?: move.context
        assertEquals("a", context["id"]?.string)
        assertEquals("c", context[position]?.string)

        // The release's press never opens the row.
        m.press(a)
        assertFalse(host.actions.any { it.name == "open" })
    }

    @Test
    fun aCancelFiresNothing() {
        val host = Host()
        val m = model(host)
        val a = m.idx("row.a")
        val fa = m.frame(a)
        val fc = m.frame(m.idx("row.c"))
        m.listDrag(a, "start", fa.left + 10f, fa.center.y)
        m.listDrag(a, "move", fa.left + 10f, fc.bottom - 4f)
        assertFalse(m.listDragPaints.isEmpty())
        m.listDrag(a, "cancel")
        assertTrue(m.listDragPaints.isEmpty())
        assertTrue("a cancel fires nothing", host.actions.isEmpty())
    }

    @Test
    fun thePaintNumbersMatchTheFixture() {
        val s = listDragStyle()
        assertEquals(0.4f, s.dim, 0.001f)
        assertEquals(2f, s.line, 0.001f)
        assertEquals(8f, s.ring, 0.001f)
        assertEquals(12f, s.liftOffset, 0.001f)
        assertEquals(0.95f, s.liftOpacity, 0.001f)
        assertEquals(0.14f, s.intoFill, 0.001f)
    }
}
