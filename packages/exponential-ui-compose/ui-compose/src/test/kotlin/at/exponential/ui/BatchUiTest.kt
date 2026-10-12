package at.exponential.ui

import at.exponential.ui.catalog.CatalogConstants
import at.exponential.ui.compose.handleKey
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.fieldEdited
import at.exponential.ui.model.fieldFocused
import at.exponential.ui.model.fieldSubmitted
import at.exponential.ui.model.press
import at.exponential.ui.model.searchKey
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * EXP-1259 (the UI batch) on the Compose model: the SearchList through its
 * field (↑/↓ move the core's highlighted row, Enter / Tab pick), a
 * searchable submenu's ← at the caret's start, Skeleton rows, the Tabs close
 * glyph, and the feedback timing + motion scale the painter reads.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class BatchUiTest {
    @Before
    fun setUp() = Fixtures.require()

    private fun model(json: String, host: RecordingHost = RecordingHost(), scope: CoroutineScope = CoroutineScope(Dispatchers.Unconfined)): SurfaceModel {
        val m = SurfaceModel("batch", host = host, scope = scope, shaper = noShaper)
        m.fixedMeasure = true
        m.setNested(json)
        m.setViewport(400f, 800f)
        return m
    }

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")
    /** A node the last pass placed (slots outlive their nodes). */
    private fun SurfaceModel.live(id: String): Boolean = indexOf(id)?.let(::node)?.removed == false
    private fun SurfaceModel.selected(id: String): Boolean = indexOf(id)?.let(::node)?.selected == true

    @Test
    fun theSearchListFiltersMovesAndPicksFromItsField() = runTest {
        val host = RecordingHost()
        val m = model(
            """{"id": "root", "component": "Box", "children": [{"id": "sl", "component": "SearchList", "props": {"items": [{"label": "Ship the search list", "value": "a", "identifier": "EXP-12"}, {"label": "Spinner sizes", "value": "b"}, {"label": "Spin the mark", "value": "c"}]}, "on": {"select": {"event": {"name": "pick"}}}}]}""",
            host,
            backgroundScope,
        )
        val field = m.idx("sl.search")
        assertTrue(m.node(field)!!.isTextField)
        m.fieldFocused(field, true)
        m.keyboardFocus = "sl.search"
        m.fieldEdited(field, "spin")
        advanceTimeBy(200)
        assertTrue("filtered", !m.live("sl.item.0"))
        assertTrue("the top row is highlighted", m.selected("sl.item.1"))
        assertTrue(handleKey(m, "down"))
        assertTrue(m.selected("sl.item.2"))
        assertTrue(handleKey(m, "up"))
        assertTrue(handleKey(m, "down"))
        m.fieldSubmitted(m.idx("sl.search"))
        assertEquals("pick", host.actions.last().name)
        assertEquals(JsonValue.Str("c"), host.actions.last().context["value"])
        assertTrue("the picked row wears the check", m.live("sl.check.2"))
    }

    @Test
    fun tabPicksAndLeftAtTheStartLeavesASearchableSubmenu() {
        val host = RecordingHost()
        val m = model("""{"id": "root", "component": "Box", "children": [{"id": "m", "component": "Menu", "props": {"label": "Add", "items": [{"label": "Implement issue", "kind": "submenu", "searchable": true, "items": [{"label": "One", "value": "1"}, {"label": "Two", "value": "2"}]}]}, "on": {"select": {"event": {"name": "pick"}}}}]}""", host)
        m.press(m.idx("m.trigger"))
        m.press(m.idx("m.item.0"))
        assertEquals(2, m.layers.size)
        m.searchKey(m.idx("m.search.0"), "ArrowLeft")
        assertEquals("back to the parent menu", 1, m.layers.size)
        m.press(m.idx("m.item.0"))
        m.searchKey(m.idx("m.search.0"), "Tab")
        assertEquals(JsonValue.Str("1"), host.actions.last().context["value"])
    }

    @Test
    fun skeletonRowsAndTheHoverCloseGlyph() {
        val m = model("""{"id": "root", "component": "Box", "children": [{"id": "sk", "component": "Skeleton", "props": {"rows": 2, "rowHeight": 44}}, {"id": "t", "component": "Tabs", "props": {"tabs": [{"label": "One", "value": "1"}], "closable": "hover", "addable": true}, "children": [{"id": "p", "component": "Text", "props": {"text": "1"}}]}]}""")
        assertEquals(44f, m.frame(m.idx("sk.row.1")).height, 0.01f)
        assertNotNull(m.indexOf("sk.bar.1.1"))
        assertNotNull(m.indexOf("t.new"))
        assertEquals("hidden until its tab is hovered", 0f, m.style(m.idx("t.close.0")).opacity ?: 1f, 0.001f)
    }

    @Test
    fun theFeedbackTimingAndTheMotionScale() {
        assertEquals(150L, CatalogConstants.PENDING_DELAY_MS)
        assertEquals(1600, CatalogConstants.SKELETON_PULSE_MS)
        assertEquals(0.45f, CatalogConstants.SKELETON_OPACITY_LOW, 0f)
        assertEquals(0.9f, CatalogConstants.SKELETON_OPACITY_HIGH, 0f)
        assertEquals(0.6f, CatalogConstants.SKELETON_REDUCED_OPACITY, 0f)
        val exponential = ThemeHandle.builtin("exponential")!!
        assertEquals(0.97f, exponential.scale("press")!!, 1e-6f)
        assertEquals(0.96f, exponential.scale("popoverFrom")!!, 1e-6f)
    }
}
