package at.exponential.ui

import at.exponential.ui.host.ClosureHost
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.InputKind
import at.exponential.ui.host.NoHost
import at.exponential.ui.host.SurfaceInputEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.checked
import at.exponential.ui.model.escape
import at.exponential.ui.model.fieldEdited
import at.exponential.ui.model.fieldFocused
import at.exponential.ui.model.fieldText
import at.exponential.ui.model.modalLayers
import at.exponential.ui.model.press
import at.exponential.ui.model.pressDown
import at.exponential.ui.model.pressUp
import at.exponential.ui.model.radioChecked
import at.exponential.ui.model.scroll
import at.exponential.ui.model.setOpen
import at.exponential.ui.model.sliderDrag
import at.exponential.ui.model.sliderRelease
import at.exponential.ui.model.sliderValue
import at.exponential.ui.model.toggleGroupSelect
import at.exponential.ui.model.toggleGroupValues
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test

/**
 * Presses, mirrors, host-owned fields (debounce, revisions, the echo
 * rule), overlays, links, the accessibility order and a windowed list,
 * all on the model (the core's fixed measure; the measurer's rules live in
 * [MeasurerRulesTest]). Debounces run on the test's virtual clock.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class InteractionTest {
    @Before
    fun setUp() = Fixtures.require()

    private fun sink(host: HostPlugin, scope: CoroutineScope = CoroutineScope(Dispatchers.Unconfined)): SurfaceModel {
        val m = SurfaceModel("kitchen-sink", host = host, scope = scope, shaper = noShaper)
        m.fixedMeasure = true
        m.setNested(Fixtures.text("kitchen-sink.json"))
        m.setData("/draft", JsonValue.Obj(mapOf("title" to JsonValue.Str(""))))
        m.setViewport(390f, 800f)
        return m
    }

    private fun SurfaceModel.idx(id: String): Int = indexOf(id) ?: error("no node $id")

    @Test
    fun pressFiresTheAction() {
        val host = RecordingHost()
        val m = sink(host)
        m.press("hdr-scan")
        assertEquals(listOf("scan"), host.actions.map { it.name })
        assertEquals("hdr-scan", host.actions[0].componentId)
        assertEquals("press", host.actions[0].event)
        assertEquals("kitchen-sink", host.actions[0].surfaceId)
    }

    @Test
    fun pressedStateRelayoutsTheOpacity() {
        val m = sink(RecordingHost())
        val i = m.idx("hdr-scan")
        assertNull(m.boxStyle(i).opacity)
        m.pressDown("hdr-scan")
        assertEquals(0.6f, m.boxStyle(i).opacity ?: 1f, 0.001f)
        m.pressUp("hdr-scan")
        assertNull(m.boxStyle(i).opacity)
    }

    @Test
    fun unboundControlsMirrorLocally() {
        val m = sink(RecordingHost())
        val box = m.idx("form-agree.box")
        assertTrue(m.checked(box))
        val before = m.style(box).background
        m.press(box)
        assertFalse(m.checked(box))
        // Round 1: the core keeps the unbound value and restyles the box itself.
        assertNotEquals("the core re-resolves the box's recipe", before, m.style(box).background)
        assertEquals(m.style(box).background, m.boxStyle(box).background)
        val track = m.idx("nav-toggle.track")
        assertTrue(m.checked(track))
        m.press(track)
        assertFalse(m.checked(track))
        // Radio: a dot press resolves the row by its index suffix.
        val dot = m.idx("form-plan.dot.1")
        assertFalse(m.radioChecked(dot))
        m.press(dot)
        assertTrue(m.radioChecked(dot))
        assertFalse(m.radioChecked(m.idx("form-plan.dot.0")))
        // ToggleGroup single + multiple.
        val seg = m.idx("segmented")
        m.toggleGroupSelect(seg, JsonValue.Str("board"))
        assertEquals(listOf("board"), m.toggleGroupValues(seg))
        val multi = m.idx("toggles")
        m.toggleGroupSelect(multi, JsonValue.Str("italic"))
        assertEquals(setOf("bold", "italic"), m.toggleGroupValues(multi).toSet())
        m.toggleGroupSelect(multi, JsonValue.Str("bold"))
        assertEquals(listOf("italic"), m.toggleGroupValues(multi))
        // Slider snaps to the step.
        val trackIdx = m.idx("form-volume.track")
        m.sliderDrag(trackIdx, SurfaceModel.snap(42.0, 0.0, 100.0, 5.0))
        assertEquals(40.0, m.sliderValue(trackIdx), 0.0)
        m.sliderRelease(trackIdx)
        assertEquals(40.0, m.sliderValue(trackIdx), 0.0)
    }

    @Test
    fun tabsAndAccordionSwitchThroughTheCore() {
        val m = sink(RecordingHost())
        val run = m.idx("tabs.tab.1")
        assertTrue(m.node(m.idx("tab-run"))!!.hidden)
        m.press(run)
        assertFalse(m.node(m.idx("tab-run"))!!.hidden)
        assertTrue(m.node(m.idx("tab-issue"))!!.hidden)
        assertTrue(m.node(run)!!.selected)
        val trigger = m.idx("accordion.trigger.1")
        assertTrue(m.node(m.idx("acc-history"))!!.hidden)
        m.press(trigger)
        assertFalse(m.node(m.idx("acc-history"))!!.hidden)
    }

    @Test
    fun fieldDebounceRevisionsAndCommit() = runTest {
        val host = RecordingHost()
        val m = sink(host, backgroundScope)
        val field = m.idx("echo-field.field")
        m.fieldFocused(field, true)
        for (ch in "Hello") m.fieldEdited(field, m.fieldText(field) + ch)
        runCurrent()
        assertEquals("nothing before the debounce", 0, host.inputs.size)
        advanceTimeBy(100)
        assertEquals("nothing before the debounce", 0, host.inputs.size)
        advanceTimeBy(160)
        assertEquals(1, host.inputs.size)
        assertEquals(JsonValue.Str("Hello"), host.inputs[0].value)
        assertEquals(5, host.inputs[0].revision)
        assertEquals(InputKind.Change, host.inputs[0].kind)
        assertEquals("/draft/title", host.inputs[0].path)
        assertEquals("a bound value writes through", JsonValue.Str("Hello"), m.data["draft"]?.get("title"))
        m.fieldEdited(field, "Hello!")
        m.fieldFocused(field, false)
        assertEquals(2, host.inputs.size)
        assertEquals(InputKind.Commit, host.inputs[1].kind)
        assertEquals(6, host.inputs[1].revision)
        assertEquals(JsonValue.Str("Hello!"), host.inputs[1].value)
        // The cancelled debounce never fires a late change.
        advanceTimeBy(500)
        assertEquals(2, host.inputs.size)
        // An echo of the host's own write never rewrites the field; a REAL
        // host change lands only while the field is idle and unfocused.
        m.setData("/draft/title", JsonValue.Str("Hello!"))
        assertEquals("Hello!", m.fieldText(field))
        val gen = m.fields["echo-field.field"]!!.writeGeneration
        m.setData("/draft/title", JsonValue.Str("From the host"))
        assertEquals("From the host", m.fieldText(field))
        assertTrue("the view reloads its value", m.fields["echo-field.field"]!!.writeGeneration > gen)
        m.fieldFocused(field, true)
        m.setData("/draft/title", JsonValue.Str("Ignored while typing"))
        assertEquals("From the host", m.fieldText(field))
    }

    @Test
    fun fortyKeystrokesInOrderWithAnEcho() = runTest {
        // The VAPP-4 typing test at the model level: 40 edits in a burst with
        // a 150 ms host echo; every character lands, in order, nothing is
        // overwritten by the echo.
        val echoed = ArrayList<SurfaceInputEvent>()
        val m = sink(NoHost, backgroundScope)
        val field = m.idx("echo-field.field")
        m.host = ClosureHost(inputs = { e ->
            echoed.add(e)
            backgroundScope.launch {
                delay(150)
                e.path?.let { m.setData(it, e.value) }
            }
        })
        m.fieldFocused(field, true)
        val text = "The quick brown fox jumps over the lazy dog".take(40)
        var typed = ""
        for (ch in text) {
            typed += ch
            m.fieldEdited(field, typed)
            advanceTimeBy(20) // a fast typist: well inside the debounce
            assertEquals(typed, m.fieldText(field))
        }
        advanceTimeBy(500)
        assertEquals(text, m.fieldText(field))
        assertEquals(40, echoed.last().revision)
        assertEquals(JsonValue.Str(text), echoed.last().value)
        assertEquals(JsonValue.Str(text), m.data["draft"]?.get("title"))
    }

    @Test
    fun overlaysOpenAndDismiss() {
        val host = RecordingHost()
        val m = sink(host)
        val trigger = m.nodes.firstOrNull { n -> n.triggerFor?.let { t -> m.indexOf(t)?.let { m.node(it)?.component } } == "Dialog" }
        assumeTrue("no dialog trigger in the kitchen sink", trigger != null)
        m.press(trigger!!.index)
        assertEquals(1, m.modalLayers.size)
        val owner = m.modalLayers[0].owner
        assertEquals(trigger.id, m.layerReturn[owner])
        assertFalse("a native dialog is not painted in the surface", m.paintsInSurface(m.modalLayers[0]))
        assertTrue(m.escape())
        assertTrue(m.layers.none { it.owner == owner })
        // The kitchen sink's open Toast stays: toasts paint in the surface and are never escaped.
        assertTrue(m.layers.all { it.isToast && m.paintsInSurface(it) })
        assertFalse("nothing left to escape", m.escape())
        // A tooltip paints in the surface in native mode.
        val tip = m.nodes.firstOrNull { it.component == "Tooltip" }
        assumeTrue("no tooltip in the kitchen sink", tip != null)
        m.setOpen(tip!!.id, true)
        val layer = m.layers.firstOrNull { it.kind == "Tooltip" }
        assertNotNull(layer)
        assertTrue(m.paintsInSurface(layer!!))
        m.setOpen(tip.id, false)
        assertTrue(m.layers.none { it.kind == "Tooltip" })
    }

    @Test
    fun linkOpensTheUrl() {
        val host = RecordingHost()
        val m = sink(host)
        m.press("form-link")
        assertEquals(listOf("https://example.com/rules"), host.urls)
    }

    @Test
    fun accessibilityOrderIsPreOrder() {
        val m = sink(RecordingHost())
        // The painter pins TalkBack to node order (`traversalIndex` = the
        // node index), so the node list must BE the pre-order walk: indices
        // are positions, parents precede children, and each node's parent
        // is the nearest earlier node one level up.
        val open = ArrayList<Int>()
        for ((pos, n) in m.nodes.withIndex()) {
            assertEquals(pos, n.index)
            while (open.isNotEmpty() && m.nodes[open.last()].depth >= n.depth) open.removeAt(open.size - 1)
            if (n.parent == null) {
                assertEquals(n.id, 0, n.depth)
            } else {
                assertTrue(n.id, n.parent!! < n.index)
                if (n.layer == m.nodes[n.parent!!].layer) assertEquals("${n.id} parent", open.lastOrNull(), n.parent)
            }
            open.add(n.index)
        }
        assertEquals(m.nodes.indices.toList(), m.nodes.sortedBy { it.index.toFloat() }.map { it.index })
        // Labels: every focusable leaf has an accessible name.
        val unnamed = m.nodes.filter {
            it.isFocusable && it.isLeaf && !it.isTextField && it.component != "ToggleGroup" &&
                it.part !in setOf("box", "dot", "track", "field", "indicator") && it.accessibilityLabel == null
        }
        assertEquals(emptyList<String>(), unnamed.map { it.id })
        // The header title reads before the button, the form before the footer link.
        val order = m.nodes.filter { it.isLeaf && !it.hidden }.map { it.id }
        assertTrue(order.indexOf("hdr-title") >= 0 && order.indexOf("hdr-title") < order.indexOf("hdr-scan"))
        assertTrue(order.indexOf("echo-field.field") >= 0 && order.indexOf("echo-field.field") < order.indexOf("form-link"))
    }

    @Test
    fun windowedListScrolls() {
        val m = makeModel("list", fixed = true)
        val rows = (0 until 200).map { i ->
            JsonValue.Obj(mapOf("id" to JsonValue.Str("r$i"), "component" to JsonValue.Str("Text"), "props" to JsonValue.Obj(mapOf("text" to JsonValue.Str("Row $i")))))
        }
        val tree = JsonValue.Obj(
            mapOf(
                "id" to JsonValue.Str("root"),
                "component" to JsonValue.Str("List"),
                "style" to JsonValue.Obj(mapOf("height" to JsonValue.Num(300.0), "overflow" to JsonValue.Str("scroll"))),
                "children" to JsonValue.Arr(rows),
            ),
        )
        m.setNested(tree.json)
        val list = m.lists["root"]
        assertNotNull("no list output", list)
        assertTrue(list!!.windowed)
        assertEquals(200, list.count)
        assertTrue(list.contentHeight > 1000f)
        assertTrue("only the visible range plus overscan is laid out", list.end - list.start < 200)
        assertEquals("the scroller's content is the whole list", list.contentHeight, m.contentHeight(m.idx("root")), 0.001f)
        assertNotNull(m.indexOf("r0"))
        val firstBefore = m.frame(m.idx("r0"))
        m.scroll("root", 2000f)
        val after = m.lists["root"]!!
        assertTrue(after.start > list.start)
        val rowInWindow = "r${after.start + 1}"
        assertNotNull("the new window's rows are laid out", m.indexOf(rowInWindow))
        assertTrue(m.frame(m.idx(rowInWindow)).top > firstBefore.top)
    }
}
