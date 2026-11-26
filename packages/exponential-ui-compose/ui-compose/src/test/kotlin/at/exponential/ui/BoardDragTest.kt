package at.exponential.ui

import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.isListDragPath
import at.exponential.ui.model.isListDragRow
import at.exponential.ui.model.listDrag
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * genUI round 4 (fixtures/board-drop.json): a Board card drags between its
 * columns through the core ([listDrag]): the paint moves to the List under
 * the pointer (its path draws on top), the release fires the TARGET
 * column's `move` with the macro's `column`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class BoardDragTest {
    private class Host : HostPlugin {
        val actions = ArrayList<SurfaceActionEvent>()
        override fun onAction(event: SurfaceActionEvent) { actions.add(event) }
    }

    private val tree = """{"id":"root","component":"Box","children":[
        {"id":"b","component":"Board","props":{"columns":{"@path":"/cols"}},"on":{"move":{"event":{"name":"move"}}}}
    ]}"""

    private fun card(v: String) = JsonValue.Obj(mapOf("label" to JsonValue.Str("Card $v"), "value" to JsonValue.Str(v)))
    private fun col(v: String, cards: List<String>) = JsonValue.Obj(mapOf("value" to JsonValue.Str(v), "title" to JsonValue.Str(v), "cards" to JsonValue.Arr(cards.map(::card))))

    private fun model(host: Host): SurfaceModel {
        val m = SurfaceModel("bd", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
        m.fixedMeasure = true
        m.setNested(tree)
        m.setViewport(1000f, 700f)
        m.setData("/cols", JsonValue.Arr(listOf(col("todo", listOf("a", "b")), col("doing", listOf("d")), col("done", emptyList()))))
        return m
    }

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    @Test
    fun aCardDropsInAnotherColumn() {
        val host = Host()
        val m = model(host)
        val a = m.idx("b.column.cards.card.todo.a")
        val d = m.frame(m.idx("b.column.cards.card.doing.d"))
        val doing = m.idx("b.column.cards.doing")
        val fa = m.frame(a)
        m.listDrag(a, "start", fa.left + 10f, fa.center.y)
        m.listDrag(a, "move", d.left + 30f, d.top + 4f)
        val paint = m.listDragPaints.firstOrNull()
        assertNotNull(paint)
        assertEquals("the paint moves to the column under the pointer", doing, paint!!.listIndex.toInt())
        assertTrue("the dragged card dims", m.isListDragRow(a))
        assertTrue("the target column draws on top", m.isListDragPath(m.idx("b.column.doing")))
        assertEquals("d", paint.dropKey)
        m.listDrag(a, "end", d.left + 30f, d.top + 4f)
        val move = host.actions.last()
        assertEquals("move", move.name)
        assertEquals("doing", move.context["column"]?.string)
        assertEquals("a", move.context["id"]?.string)
        assertEquals("d", move.context["before"]?.string)
    }

    @Test
    fun anEmptyColumnTakesTheCard() {
        val host = Host()
        val m = model(host)
        val a = m.idx("b.column.cards.card.todo.a")
        val done = m.frame(m.idx("b.column.cards.done"))
        val fa = m.frame(a)
        m.listDrag(a, "start", fa.left + 10f, fa.center.y)
        m.listDrag(a, "move", done.left + 20f, done.top + 6f)
        assertNotNull("the into highlight", m.listDragPaints.first().into)
        m.listDrag(a, "end", done.left + 20f, done.top + 6f)
        val move = host.actions.last()
        assertEquals("done", move.context["column"]?.string)
        assertEquals(null, move.context["before"])
    }
}
