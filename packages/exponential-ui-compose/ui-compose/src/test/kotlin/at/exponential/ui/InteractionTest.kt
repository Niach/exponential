package at.exponential.ui

import android.view.KeyEvent as AndroidKeyEvent
import at.exponential.ui.compose.handleKey
import at.exponential.ui.compose.keyRecorderKeyName
import at.exponential.ui.compose.keyRecorderKeyPayload
import at.exponential.ui.host.ClosureHost
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.InputKind
import at.exponential.ui.host.NoHost
import at.exponential.ui.host.PickedFile
import at.exponential.ui.host.SurfaceInputEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.boxHover
import at.exponential.ui.model.keyRecorderBlur
import at.exponential.ui.model.keyRecorderMouse
import at.exponential.ui.model.keyRecorderPayload
import at.exponential.ui.model.keyRecorderPress
import at.exponential.ui.model.keyRecorderRecord
import at.exponential.ui.model.keyRecorderRecording
import at.exponential.ui.model.checked
import at.exponential.ui.model.composerKey
import at.exponential.ui.model.composerMenuKey
import at.exponential.ui.model.composerPaste
import at.exponential.ui.model.composerSubmit
import at.exponential.ui.model.escape
import at.exponential.ui.model.INPUT_DEBOUNCE_MS
import at.exponential.ui.model.field
import at.exponential.ui.model.fieldEdited
import at.exponential.ui.model.fieldFocused
import at.exponential.ui.model.fieldText
import at.exponential.ui.model.focus
import at.exponential.ui.model.hover
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
import at.exponential.ui.model.segmentedSelect
import at.exponential.ui.model.selectPick
import at.exponential.ui.model.selectSearchClearable
import at.exponential.ui.model.selectSearchClear
import at.exponential.ui.model.segmentedValues
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
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
        m.press("hdr_scan")
        assertEquals(listOf("scan"), host.actions.map { it.name })
        assertEquals("hdr_scan", host.actions[0].componentId)
        assertEquals("press", host.actions[0].event)
        assertEquals("kitchen-sink", host.actions[0].surfaceId)
    }

    @Test
    fun pressedStateRelayoutsTheOpacity() {
        val m = sink(RecordingHost())
        val i = m.idx("hdr_scan")
        assertNull(m.boxStyle(i).opacity)
        m.pressDown("hdr_scan")
        assertEquals(0.6f, m.boxStyle(i).opacity ?: 1f, 0.001f)
        m.pressUp("hdr_scan")
        assertNull(m.boxStyle(i).opacity)
    }

    @Test
    fun unboundControlsMirrorLocally() {
        val m = sink(RecordingHost())
        val box = m.idx("form_agree.box")
        assertTrue(m.checked(box))
        val before = m.style(box).background
        m.press(box)
        assertFalse(m.checked(box))
        // Round 1: the core keeps the unbound value and restyles the box itself.
        assertNotEquals("the core re-resolves the box's recipe", before, m.style(box).background)
        assertEquals(m.style(box).background, m.boxStyle(box).background)
        val track = m.idx("nav_toggle.track")
        assertTrue(m.checked(track))
        m.press(track)
        assertFalse(m.checked(track))
        // Radio: a dot press resolves the row by its index suffix.
        val dot = m.idx("form_plan.dot.1")
        assertFalse(m.radioChecked(dot))
        m.press(dot)
        assertTrue(m.radioChecked(dot))
        assertFalse(m.radioChecked(m.idx("form_plan.dot.0")))
        // Segmented single + multiple.
        val seg = m.idx("segmented")
        m.segmentedSelect(seg, JsonValue.Str("board"))
        assertEquals(listOf("board"), m.segmentedValues(seg))
        val multi = m.idx("toggles")
        m.segmentedSelect(multi, JsonValue.Str("italic"))
        assertEquals(setOf("bold", "italic"), m.segmentedValues(multi).toSet())
        m.segmentedSelect(multi, JsonValue.Str("bold"))
        assertEquals(listOf("italic"), m.segmentedValues(multi))
        // Slider snaps to the step.
        val trackIdx = m.idx("form_volume.track")
        m.sliderDrag(trackIdx, SurfaceModel.snap(42.0, 0.0, 100.0, 5.0))
        assertEquals(40.0, m.sliderValue(trackIdx), 0.0)
        m.sliderRelease(trackIdx)
        assertEquals(40.0, m.sliderValue(trackIdx), 0.0)
    }

    @Test
    fun tabsAndAccordionSwitchThroughTheCore() {
        val m = sink(RecordingHost())
        val run = m.idx("tabs.tab.1")
        assertTrue(m.node(m.idx("tab_run"))!!.hidden)
        m.press(run)
        assertFalse(m.node(m.idx("tab_run"))!!.hidden)
        assertTrue(m.node(m.idx("tab_issue"))!!.hidden)
        assertTrue(m.node(run)!!.selected)
        val trigger = m.idx("accordion.trigger.1")
        assertTrue(m.node(m.idx("acc_history"))!!.hidden)
        m.press(trigger)
        assertFalse(m.node(m.idx("acc_history"))!!.hidden)
    }

    @Test
    fun fieldDebounceRevisionsAndCommit() = runTest {
        val host = RecordingHost()
        val m = sink(host, backgroundScope)
        val field = m.idx("echo_field.field")
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
        val gen = m.fields["echo_field.field"]!!.writeGeneration
        m.setData("/draft/title", JsonValue.Str("From the host"))
        assertEquals("From the host", m.fieldText(field))
        assertTrue("the view reloads its value", m.fields["echo_field.field"]!!.writeGeneration > gen)
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
        val field = m.idx("echo_field.field")
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
        m.press("form_link")
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
            it.isFocusable && it.isLeaf && !it.isTextField && it.component != "Segmented" &&
                it.part !in setOf("box", "dot", "track", "field", "indicator") && it.accessibilityLabel == null
        }
        assertEquals(emptyList<String>(), unnamed.map { it.id })
        // The header title reads before the button, the form before the footer link.
        val order = m.nodes.filter { it.isLeaf && !it.hidden }.map { it.id }
        assertTrue(order.indexOf("hdr_title") >= 0 && order.indexOf("hdr_title") < order.indexOf("hdr_scan"))
        assertTrue(order.indexOf("echo_field.field") >= 0 && order.indexOf("echo_field.field") < order.indexOf("form_link"))
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

    /**
     * fixtures/interactions-round1.json `select` (VAPP-146): "" is an
     * ordinary option value (the trigger shows the option holding it;
     * picking the option that holds the value fires nothing), through the
     * core's popup (painted overlays) and through the native menu's
     * [selectPick].
     */
    @Test
    fun selectCasesTakeAnEmptyOptionValue() {
        val cases = Fixtures.json("interactions-round1.json")["select"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        val values = listOf("", "open", "done")
        for (native in listOf(false, true)) for (c in cases) {
            val name = (c["name"]?.string ?: "?") + if (native) " (selectPick)" else ""
            val host = RecordingHost()
            // Painted overlays: the core's popup; native ones: the M3 menu picks through selectPick.
            val overlays = if (native) OverlayPresentation.Native else OverlayPresentation.Painted
            val m = SurfaceModel("select", SurfaceOptions(overlays = overlays), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(
                """{"id":"root","component":"Box","children":[{"id":"s","component":"Select","props":{"label":"Status","name":"s","options":[{"label":"Any","value":""},{"label":"Open","value":"open"},{"label":"Done","value":"done"}],"value":{"@path":"/v"}},"on":{"change":{"event":{"name":"change"}}}}]}""",
            )
            m.setViewport(390f, 800f)
            c["value"]?.takeUnless { it.isNull }?.let { m.setData("/v", it) }
            for (i in c["presses"]?.array.orEmpty().map { it.number!!.toInt() }) {
                if (native) {
                    m.selectPick(m.idx("s.trigger"), JsonValue.Str(values[i]))
                } else {
                    m.press("s.trigger")
                    m.press(m.idx("s.item.$i"))
                }
            }
            val expect = c["expect"]!!
            // The open/close `change {open}` is not a pick.
            val changes = host.actions.filter { it.name == "change" }.mapNotNull { (it.payload?.get("value") ?: it.context["value"])?.string }
            assertEquals(name, expect["changes"]?.array.orEmpty().map { it.string }, changes)
            assertEquals(name, expect["value"] ?: JsonValue.Null, m.data["v"] ?: JsonValue.Null)
            assertEquals(name, expect["shows"]?.string, m.node(m.idx("s.trigger"))?.props?.get("text")?.displayText)
        }
    }

    /**
     * #EXP-1629: an open Select popup (the core's layer, painted overlays)
     * starts at the TOP with its ONE `hover` (the cursor) on the first
     * enabled option, the bottom fade while options sit below; the pointer
     * moves the cursor (leaving keeps it), `selected` stays the chosen
     * option's, a reopen starts over.
     */
    @Test
    fun selectPopupCursorStartsOnTheFirstOption() {
        val options = (0 until 30).joinToString(",") { """{"label":"Option $it","value":"o$it"}""" }
        val m = SurfaceModel("select-cursor", SurfaceOptions(overlays = OverlayPresentation.Painted), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
        m.fixedMeasure = true
        m.setNested("""{"id":"root","component":"Box","children":[{"id":"s","component":"Select","props":{"label":"Pick","name":"s","options":[$options],"value":"o20"}}]}""")
        m.setViewport(390f, 800f)
        val hovered = { m.nodes.filter { it.ownerComponent == "Select" && it.part == "item" && !it.hidden && "hover" in m.statesOf(it.id) }.map { it.id } }
        m.press("s.trigger")
        assertEquals(listOf("s.item.0"), hovered())
        assertTrue(m.node(m.idx("s.item.20"))!!.selected)
        val list = m.nodes.first { it.owner == "s" && it.part == "list" }
        assertEquals("the list starts at the top", 0f, m.scrolls[list.index]?.offsetY ?: 0f)
        assertTrue("more options below: the fade", m.nodes.any { it.owner == "s" && it.part == "fade" && !it.hidden })
        m.hover("s.item.3", true)
        assertEquals(listOf("s.item.3"), hovered())
        m.hover("s.item.3", false)
        assertEquals(listOf("s.item.3"), hovered())
        assertFalse(m.node(m.idx("s.item.3"))!!.selected)
        assertTrue(m.escape())
        assertTrue(m.layers.none { it.owner == "s" })
        m.press("s.trigger")
        assertEquals(listOf("s.item.0"), hovered())
    }

    /**
     * #EXP-1629: an option's `trailing` reads on after its label in the popup
     * row (the core's item, the native menu's option), never in the trigger;
     * `open` fires once as the popup opens, through the core's layer and
     * through the native menu alike.
     */
    @Test
    fun selectTrailingAndOpen() {
        for (native in listOf(false, true)) {
            val host = RecordingHost()
            val overlays = if (native) OverlayPresentation.Native else OverlayPresentation.Painted
            val m = SurfaceModel("select-trailing", SurfaceOptions(overlays = overlays), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested("""{"id":"root","component":"Box","children":[{"id":"s","component":"Select","props":{"label":"Branch","name":"s","triggerVariant":"row","options":[{"label":"main","value":"main","trailing":"· default"},{"label":"develop","value":"develop"}],"value":"main"},"on":{"open":{"event":{"name":"loadBranches"}}}}]}""")
            m.setViewport(390f, 800f)
            assertEquals(0, host.actions.count { it.name == "loadBranches" })
            m.press("s.trigger")
            assertEquals("native=$native", 1, host.actions.count { it.name == "loadBranches" })
            if (native) {
                assertEquals(listOf("main · default", "develop"), at.exponential.ui.compose.selectOptions(m.ownerProps(m.idx("s.trigger")), "").map { it.text })
                assertEquals(listOf("main"), at.exponential.ui.compose.selectOptions(m.ownerProps(m.idx("s.trigger")), "mai").map { it.value.displayText })
            } else {
                assertEquals("main · default", m.node(m.idx("s.item.0"))?.props?.get("text")?.displayText)
                assertEquals("develop", m.node(m.idx("s.item.1"))?.props?.get("text")?.displayText)
            }
        }
    }

    /** #EXP-1629: the popup's cursor rules, alignment, spacing and width (a row: under its value, `start`, layout `selectRowPopupWidth`). */
    @Test
    fun selectPopupRules() {
        val opts = listOf("a", "b", "c", "d").mapIndexed { i, v -> at.exponential.ui.compose.SelectOption(JsonValue.Str(v), v.uppercase(), null, i == 0 || i == 2) }
        assertEquals(1, at.exponential.ui.compose.selectInitialCursor(opts))
        // A `multiple` one: its first enabled CHOSEN option, none while nothing is (the IDE's multi picker).
        assertEquals(3, at.exponential.ui.compose.selectInitialCursor(opts, listOf("c", "d")))
        assertEquals(-1, at.exponential.ui.compose.selectInitialCursor(opts, emptyList()))
        assertEquals(3, at.exponential.ui.compose.selectStep(opts, 1, 1))
        assertEquals(3, at.exponential.ui.compose.selectStep(opts, 3, 1))
        assertEquals(1, at.exponential.ui.compose.selectStep(opts, 3, -1))
        val row = mapOf("triggerVariant" to JsonValue.Str("row"))
        assertTrue(at.exponential.ui.compose.selectRowVariant(row))
        assertFalse(at.exponential.ui.compose.selectRowVariant(emptyMap()))
        assertEquals("start", at.exponential.ui.compose.selectPopupAlign(row))
        assertEquals("start", at.exponential.ui.compose.selectPopupAlign(emptyMap()))
        assertEquals("center", at.exponential.ui.compose.selectPopupAlign(mapOf("align" to JsonValue.Str("center"))))
        assertEquals(1f to 7f, at.exponential.ui.compose.selectPopupSpacing(row = true))
        assertEquals(4f to 8f, at.exponential.ui.compose.selectPopupSpacing(row = false))
        assertTrue(at.exponential.ui.compose.selectMoreBelow(0, 200))
        assertFalse(at.exponential.ui.compose.selectMoreBelow(199, 200))
        assertFalse(at.exponential.ui.compose.selectMoreBelow(0, 0))
        assertEquals(262f, at.exponential.ui.compose.selectPopupWidth(row, row = true))
        assertNull(at.exponential.ui.compose.selectPopupWidth(emptyMap(), row = false))
        assertEquals(300f, at.exponential.ui.compose.selectPopupWidth(mapOf("popupWidth" to JsonValue.Num(300.0)), row = false))
    }

    /**
     * #EXP-1765: a `pill` Select opened by the POINTER (the core's layer)
     * shows no cursor until the pointer moves over an option or a key goes
     * (↓ lands on the first enabled option); opened from the keyboard it
     * keeps today's cursor. `loading` lays the Skeleton rows (`<id>.skeleton`)
     * in place of the options and `empty`; a mono option keeps its
     * `trailing` its own run (the core's `trailing` prop, the label alone
     * as the text), a plain one reads it on.
     */
    @Test
    fun selectPill() {
        val options = """[{"label":"Option A","value":"a","disabled":true},{"label":"main","value":"main","mono":true,"trailing":"default"},{"label":"dev","value":"dev","trailing":"· old"}]"""
        fun surface(extra: String = ""): SurfaceModel {
            val m = SurfaceModel("select-pill", SurfaceOptions(overlays = OverlayPresentation.Painted), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested("""{"id":"root","component":"Box","children":[{"id":"s","component":"Select","props":{"label":"Branch","name":"s","triggerVariant":"pill","options":$options,"value":"main"$extra}}]}""")
            m.setViewport(390f, 800f)
            return m
        }
        val hovered = { m: SurfaceModel -> m.nodes.filter { it.ownerComponent == "Select" && it.part == "item" && !it.hidden && "hover" in m.statesOf(it.id) }.map { it.id } }
        // The pointer: no cursor, then the pointer (or ↓) brings it.
        var m = surface()
        m.focus("s.trigger", true, fromKeyboard = false)
        m.press("s.trigger")
        assertTrue(m.layers.any { it.owner == "s" })
        assertEquals(emptyList<String>(), hovered(m))
        m.hover("s.item.2", true)
        assertEquals(listOf("s.item.2"), hovered(m))
        m = surface()
        m.focus("s.trigger", true, fromKeyboard = false)
        m.press("s.trigger")
        assertTrue(m.layers.any { it.owner == "s" })
        assertEquals(emptyList<String>(), hovered(m))
        assertTrue(handleKey(m, "down"))
        assertEquals(listOf("s.item.1"), hovered(m))
        // The keyboard: today's cursor at once.
        m = surface()
        m.focus("s.trigger", true, fromKeyboard = true)
        m.press("s.trigger")
        assertEquals(listOf("s.item.1"), hovered(m))
        // A mono option's trailing stays its own run; a plain one's reads on.
        assertEquals("main", m.node(m.idx("s.item.1"))?.props?.get("text")?.displayText)
        assertEquals("default", m.node(m.idx("s.item.1"))?.props?.get("trailing")?.displayText)
        assertEquals("dev · old", m.node(m.idx("s.item.2"))?.props?.get("text")?.displayText)
        assertNull(m.node(m.idx("s.item.2"))?.props?.get("trailing"))
        val opts = at.exponential.ui.compose.selectOptions(m.ownerProps(m.idx("s.trigger")), "")
        assertEquals(listOf("Option A", "main", "dev · old"), opts.map { it.text })
        assertEquals(listOf(null, "default", null), opts.map { it.trailing })
        assertEquals(listOf(false, true, false), opts.map { it.mono })
        assertTrue("the search matches the label alone", at.exponential.ui.compose.selectOptions(m.ownerProps(m.idx("s.trigger")), "default").isEmpty())
        // loading: the Skeleton rows in place of the options (and `empty`); the search stays.
        m = surface(""","loading":true,"searchable":true,"searchPlaceholder":"Search branches"""")
        m.press("s.trigger")
        assertNotNull(m.indexOf("s.skeleton"))
        assertEquals(emptyList<String>(), m.nodes.filter { it.owner == "s" && (it.part == "item" || it.part == "empty") && !it.hidden }.map { "${it.id}/${it.component}" })
        val search = m.nodes.first { it.owner == "s" && it.part == "search" }
        assertEquals("Search branches", search.props["placeholder"]?.displayText)
    }

    /**
     * #EXP-1765: a `pill` Select's search ends in the clear button only while
     * it holds text; pressing it empties the field, the core hears the empty
     * query (the list unfiltered) and the focus stays in the field. A
     * `field` Select's search never has one.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    @Test
    fun selectPillSearchClear() = runTest {
        for (variant in listOf("pill", "field")) {
            val m = SurfaceModel("select-clear", SurfaceOptions(overlays = OverlayPresentation.Painted), RecordingHost(), backgroundScope, noShaper)
            m.fixedMeasure = true
            m.setNested("""{"id":"root","component":"Box","children":[{"id":"s","component":"Select","props":{"label":"Branch","name":"s","triggerVariant":"$variant","searchable":true,"options":[{"label":"main","value":"main"},{"label":"develop","value":"develop"}]}}]}""")
            m.setViewport(390f, 800f)
            m.press("s.trigger")
            val search = { m.nodes.first { it.owner == "s" && it.part == "search" }.index }
            val items = { m.nodes.filter { it.owner == "s" && it.part == "item" && !it.hidden }.map { it.id } }
            assertFalse("$variant: no clear while empty", m.selectSearchClearable(search()))
            m.fieldEdited(search(), "dev")
            advanceTimeBy(INPUT_DEBOUNCE_MS + 10)
            runCurrent()
            println("DBG $variant text=${m.fieldText(search())} field=${m.field(search())} pending=${m.field(search())?.pendingChange} props=${m.node(search())?.props} items=${m.nodes.filter { it.owner == "s" && it.part == "item" }.map { it.id + ":" + it.hidden }}")
            assertEquals("$variant: filtered", listOf("s.item.1"), items())
            assertEquals("$variant: clear only on a pill", variant == "pill", m.selectSearchClearable(search()))
            if (variant != "pill") continue
            m.selectSearchClear(search())
            assertEquals("", m.fieldText(search()))
            assertEquals(listOf("s.item.0", "s.item.1"), items())
            assertFalse(m.selectSearchClearable(search()))
            assertEquals("s.search", m.focusRequest)
            assertTrue(m.layers.any { it.owner == "s" })
        }
    }

    /** #EXP-1765: a `loading` Select is not inert: it opens (the core's layer with its skeleton, and our own popup). */
    @Test
    fun selectLoadingOpens() {
        for (native in listOf(false, true)) {
            val overlays = if (native) OverlayPresentation.Native else OverlayPresentation.Painted
            val m = SurfaceModel("select-loading", SurfaceOptions(overlays = overlays), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested("""{"id":"root","component":"Box","children":[{"id":"s","component":"Select","props":{"label":"Branch","name":"s","triggerVariant":"pill","loading":true,"options":[{"label":"main","value":"main"}]}}]}""")
            m.setViewport(390f, 800f)
            assertFalse(m.isDisabled(m.idx("s.trigger")))
            m.press("s.trigger")
            if (native) assertEquals("s.trigger", m.popup)
            else {
                assertTrue(m.layers.any { it.owner == "s" })
                assertNotNull(m.indexOf("s.skeleton"))
            }
        }
    }

    /** #EXP-1765: a pill's popup geometry (our own popup): `selectRowPopupWidth` wide, 3 under the pill, its aligned edge 1 outside the pill's. */
    @Test
    fun selectPillPopupRules() {
        val pill = mapOf("triggerVariant" to JsonValue.Str("pill"))
        assertTrue(at.exponential.ui.compose.selectPillVariant(pill))
        assertTrue(at.exponential.ui.compose.selectPillVariant(emptyMap(), pill))
        assertFalse(at.exponential.ui.compose.selectPillVariant(mapOf("triggerVariant" to JsonValue.Str("row"))))
        assertEquals(3f to 7f, at.exponential.ui.compose.selectPopupSpacing(row = false, pill = true))
        assertEquals(1f, at.exponential.ui.compose.selectPopupInset(true))
        assertEquals(0f, at.exponential.ui.compose.selectPopupInset(false))
        assertEquals(262f, at.exponential.ui.compose.selectPopupWidth(pill, row = true))
        // LTR start: one before the pill's left edge; end: one past its right edge; rtl start = the right edge.
        assertEquals(99, at.exponential.ui.compose.selectPopupX("start", false, 100, 160, 262, 1))
        assertEquals(161 - 262, at.exponential.ui.compose.selectPopupX("end", false, 100, 160, 262, 1))
        assertEquals(161 - 262, at.exponential.ui.compose.selectPopupX("start", true, 100, 160, 262, 1))
        assertEquals(100, at.exponential.ui.compose.selectPopupX("start", false, 100, 160, 262, 0))
        assertEquals(7f, at.exponential.ui.compose.selectTrailingGap(7f))
        assertEquals(4f, at.exponential.ui.compose.selectTrailingGap(0f))
    }

    /**
     * fixtures/interactions-round1.json `foldRow` (#VAPP-284): a foldable
     * Row (children); a press on the head or its fold button, or
     * ArrowRight / ArrowLeft through [handleKey] on the focused one
     * (swapped in rtl); the events the host sees and the head's `expanded`.
     */
    @Test
    fun foldRowCasesPressFoldAndTakeTheArrows() {
        val fold = Fixtures.json("interactions-round1.json")["foldRow"]!!
        val cases = fold["cases"]?.array.orEmpty()
        assertEquals(12, cases.size)
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            var row = fold["surface"]!!.obj!!
            if (c["pressable"]?.bool == false) row = row + ("on" to JsonValue.Obj(row["on"]!!.obj!! - "press"))
            val tree = buildMap<String, JsonValue> {
                put("id", JsonValue.Str("root"))
                put("component", JsonValue.Str("Box"))
                put("children", JsonValue.Arr(listOf(JsonValue.Obj(row))))
                if (c["rtl"]?.bool == true) put("style", JsonValue.Obj(mapOf("direction" to JsonValue.Str("rtl"))))
            }
            val host = RecordingHost()
            val m = SurfaceModel("fold", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(JsonValue.Obj(tree).json)
            m.setViewport(390f, 800f)
            c["open"]?.takeUnless { it.isNull }?.let { m.setData("/open", it) }
            val step = c["step"]!!
            val part = (step["press"] ?: step["on"])?.string
            val target = if (part == "head") "issues.head" else "issues.head.fold"
            assertNotNull("$name: no $target", m.indexOf(target))
            if (step["press"] != null) {
                m.press(target)
            } else {
                m.focus(target, true, fromKeyboard = true)
                val key = if (step["key"]?.string == "ArrowLeft") "left" else "right"
                assertTrue("$name: the arrow is the fold's", handleKey(m, key))
            }
            assertEquals("$name: events", c["events"]?.array.orEmpty().map { it.string }, host.actions.map { it.name })
            for (a in host.actions.filter { it.name == "fold" }) assertEquals("$name: fold context", c["after"]?.bool, a.context["open"]?.bool)
            assertEquals("$name: expanded after", c["after"]?.bool, m.node(m.idx("issues.head"))?.accessibility?.get("expanded")?.bool)
        }
    }

    /**
     * fixtures/interactions-round1.json `overlayAnchor` (#EXP-1739): an open
     * anchored Popover closes once its anchor scrolls, moves or leaves the
     * tree. The core raises the close during the pass; the model drains it
     * (like iOS and the desktop) and the next pass drops the layer.
     */
    @Test
    fun overlayAnchorCasesCloseOnScrollMoveAndRemoval() {
        val fx = Fixtures.json("interactions-round1.json")["overlayAnchor"]!!
        val base = fx["surface"]!!.obj!!
        val cases = fx["cases"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val m = SurfaceModel("anchor", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(JsonValue.Obj(base).json)
            m.setViewport(390f, 800f)
            assertTrue("$name: opens against its anchor", m.layers.any { it.owner == "pop" })
            val step = c["step"]!!
            var children = base["children"]!!.array!!
            if (step["insertAbove"]?.bool == true) children = listOf(fx["above"]!!) + children
            if (step["removeAnchor"]?.bool == true) children = children.filter { it["id"]?.string != "target" }
            if (children != base["children"]!!.array!!) m.setNested(JsonValue.Obj(base + ("children" to JsonValue.Arr(children))).json)
            step["scroll"]?.string?.let { m.scroll(it, (step["y"]?.number ?: 0.0).toFloat()) }
            if (step["relayout"]?.bool == true) m.setViewport(390f, 800f)
            assertEquals(name, c["after"]?.bool, m.layers.any { it.owner == "pop" })
        }
    }

    /**
     * fixtures/interactions-round1.json `boxHover` (#EDE-65): the pointer
     * entering a Box fires its `on.hover` (no payload), leaving fires
     * nothing, a pressable Box still presses; [boxHover] is what the
     * painter's pointer Enter / Exit calls.
     */
    @Test
    fun boxHoverCasesFireOnEntering() {
        // #FEED-201: `tilePress` (StatTile and Card presses) has the same shape.
        for (key in listOf("boxHover", "tilePress")) {
        val f = Fixtures.json("interactions-round1.json")[key]!!
        val cases = f["cases"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val host = RecordingHost()
            val m = SurfaceModel("box-hover", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(f["surface"]!!.json)
            m.setViewport(390f, 800f)
            for (step in c["steps"]?.array.orEmpty()) {
                val enter = step["enter"]?.string
                val leave = step["leave"]?.string
                val press = step["press"]?.string
                when {
                    enter != null -> m.boxHover(enter, true)
                    leave != null -> m.boxHover(leave, false)
                    press != null -> m.press(press)
                    else -> error("$name: unknown step ${step.json}")
                }
            }
            val want = c["events"]?.array.orEmpty()
            assertEquals("$name: names", want.map { it["name"]?.string }, host.actions.map { it.name })
            assertEquals("$name: contexts", want.map { it["context"] ?: JsonValue.Obj(emptyMap()) }, host.actions.map { it.context })
        }
        }
    }

    /**
     * fixtures/interactions-round1.json `keyRecorder` (#EXP-1629): what the
     * painter sends for each step: a press = [keyRecorderPress], a key
     * down / up = the Android key code of the canonical name with the held
     * mods' meta state through [keyRecorderKeyPayload] (the capture half),
     * a modifier change = one `modifiers` payload, a mouse button =
     * [keyRecorderMouse] (3-5 only), blur = [keyRecorderBlur], hostWrite =
     * the host's data write; then the fired actions {name, context} and
     * /recording (null = unset), and the field's live `recording`.
     */
    @Test
    fun keyRecorderCasesStartRecordAndStop() {
        val f = Fixtures.json("interactions-round1.json")["keyRecorder"]!!
        val cases = f["cases"]?.array.orEmpty()
        assertTrue(cases.size >= 10)
        val codes = (0..400).filter { it != AndroidKeyEvent.KEYCODE_BACK && it != AndroidKeyEvent.KEYCODE_NUMPAD_ENTER }
            .mapNotNull { c -> keyRecorderKeyName(c)?.let { it to c } }.toMap()
        fun meta(mods: List<JsonValue>): Int = mods.mapNotNull { it.string }.fold(0) { acc, m ->
            acc or when (m) {
                "ctrl" -> AndroidKeyEvent.META_CTRL_ON or AndroidKeyEvent.META_CTRL_LEFT_ON
                "alt" -> AndroidKeyEvent.META_ALT_ON or AndroidKeyEvent.META_ALT_LEFT_ON
                "shift" -> AndroidKeyEvent.META_SHIFT_ON or AndroidKeyEvent.META_SHIFT_LEFT_ON
                "meta" -> AndroidKeyEvent.META_META_ON or AndroidKeyEvent.META_META_LEFT_ON
                else -> error("mod $m")
            }
        }
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val host = RecordingHost()
            val m = SurfaceModel("key_recorder", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(f["surface"]!!.json)
            m.setViewport(390f, 800f)
            for (step in c["steps"]?.array.orEmpty()) {
                val field = m.idx("ptt.field")
                val held = step["mods"]?.array.orEmpty()
                when {
                    step["press"] != null -> m.keyRecorderPress(field)
                    step["blur"] != null -> m.keyRecorderBlur(field)
                    step["down"] != null || step["up"] != null -> {
                        val down = step["down"] != null
                        val key = (step["down"] ?: step["up"])!!.string!!
                        val code = codes[key] ?: error("$name: no key code for $key")
                        keyRecorderKeyPayload(code, down, meta(held))?.let { m.keyRecorderRecord(field, it) }
                    }
                    step["modifiers"] != null -> {
                        val set = step["modifiers"]!!.array.orEmpty().mapNotNull { it.string }.toSet()
                        m.keyRecorderRecord(field, keyRecorderPayload("modifiers", "ctrl" in set, "alt" in set, "shift" in set, "meta" in set))
                    }
                    step["mouse"] != null -> m.keyRecorderMouse(field, step["mouse"]!!.number!!.toInt())
                    step["hostWrite"] != null -> m.setData(step["hostWrite"]!!["path"]!!.string!!, step["hostWrite"]!!["value"])
                    else -> error("$name: unknown step ${step.json}")
                }
            }
            val want = c["events"]?.array.orEmpty()
            assertEquals("$name: names", want.map { it["name"]?.string }, host.actions.map { it.name })
            assertEquals("$name: contexts", want.map { it["context"] ?: JsonValue.Obj(emptyMap()) }, host.actions.map { it.context })
            val recording = JsonValue.parse(m.surface.dataJson())["recording"] ?: JsonValue.Null
            assertEquals("$name: /recording", c["recording"] ?: JsonValue.Null, recording)
            assertEquals("$name: the field records", recording.bool == true, m.keyRecorderRecording(m.idx("ptt.field")))
        }
    }

    /**
     * fixtures/interactions-round1.json `composerKeys` (#EDE-65): a
     * Composer `c` (value /draft, menuKeys /menu). `type` = the field's own
     * edit ([fieldEdited]), a key = [composerKey] with the held mods and
     * the IME composition; a key it leaves is the field's own, and the
     * field's own unmodified, uncomposed Enter is [composerSubmit] (the
     * view's Enter); `handled` = the Composer took it. `hostWrite` = the
     * host's data write; then, the debounce run out, the fired actions
     * {name, context} and /draft (null = unset).
     */
    @Test
    fun composerKeyCasesFireKeyOrSubmit() = runTest {
        val f = Fixtures.json("interactions-round1.json")["composerKeys"]!!
        val cases = f["cases"]?.array.orEmpty()
        assertTrue(cases.size >= 10)
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val host = RecordingHost()
            val m = SurfaceModel("composer_keys", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, backgroundScope, noShaper)
            m.fixedMeasure = true
            m.setNested(f["surface"]!!.json)
            for ((k, v) in c["data"]?.obj.orEmpty()) m.setData("/$k", v)
            m.setViewport(390f, 800f)
            for (step in c["steps"]?.array.orEmpty()) {
                val field = m.idx("c")
                when {
                    step["type"] != null -> m.fieldEdited(field, step["type"]!!.string!!)
                    step["key"] != null -> {
                        val key = step["key"]!!.string!!
                        val modified = step["mods"]?.array.orEmpty().isNotEmpty()
                        val composing = step["composing"]?.bool == true
                        var handled = m.composerKey(field, key, modified, composing)
                        if (!handled && key == "Enter" && !modified && !composing) {
                            m.composerSubmit(field)
                            handled = true
                        }
                        assertEquals("$name: $key ${step.json} handled", step["handled"]?.bool == true, handled)
                    }
                    step["hostWrite"] != null -> m.setData(step["hostWrite"]!!["path"]!!.string!!, step["hostWrite"]!!["value"])
                    else -> error("$name: unknown step ${step.json}")
                }
            }
            advanceTimeBy(INPUT_DEBOUNCE_MS + 10)
            runCurrent()
            val want = c["events"]?.array.orEmpty()
            assertEquals("$name: names", want.map { it["name"]?.string }, host.actions.map { it.name })
            assertEquals("$name: contexts", want.map { it["context"] ?: JsonValue.Obj(emptyMap()) }, host.actions.map { it.context })
            assertEquals("$name: /draft", c["draft"] ?: JsonValue.Null, m.data["draft"] ?: JsonValue.Null)
        }
    }

    /**
     * fixtures/interactions-round1.json `composerSlots` (#EDE-65 R): through
     * the MODEL (the core's layout), the slot roots sit in the controls row
     * in `order` (start → end, mirrored in RTL), centred on the send disc's
     * row (`c.sendSpace`), inside the composer; `trailing` ends before the
     * disc; a press on a slot child fires its own action. The attach
     * control's place is the leaf's (`ComposerLeaf`: before `trailing`).
     */
    @Test
    fun composerSlotCasesSitInTheControlsRow() = runTest {
        val f = Fixtures.json("interactions-round1.json")["composerSlots"]!!
        val cases = f["cases"]?.array.orEmpty()
        assertEquals(7, cases.size)
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val rtl = c["direction"]?.string == "rtl"
            val surface = f["surface"]!!.obj!!
            val slots = c["slots"]?.obj.orEmpty()
            val composer = surface + ("props" to JsonValue.Obj(surface["props"]!!.obj!! + c["props"]?.obj.orEmpty())) + (if (slots.isEmpty()) emptyMap<String, JsonValue>() else mapOf<String, JsonValue>("slots" to JsonValue.Obj(slots)))
            val tree = JsonValue.Obj(mapOf("id" to JsonValue.Str("root"), "component" to JsonValue.Str("Box"), "style" to JsonValue.Obj(mapOf("display" to JsonValue.Str("flex"), "flexDirection" to JsonValue.Str("column"), "direction" to JsonValue.Str(if (rtl) "rtl" else "ltr"))), "children" to JsonValue.Arr(listOf(JsonValue.Obj(composer)))))
            val host = RecordingHost()
            val m = SurfaceModel("composer_slots", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, backgroundScope, noShaper)
            m.fixedMeasure = true
            m.setNested(tree.json)
            m.setViewport(390f, 800f)
            val cf = m.frame(m.idx("c"))
            if (slots.isEmpty()) {
                assertNull("$name: no row without slots", m.indexOf("c.bar"))
                continue
            }
            val send = m.frame(m.idx("c.sendSpace"))
            val along = ArrayList<Pair<Float, String>>()
            for ((slot, id) in listOf("leading" to "plus", "trailing" to "ring")) {
                val i = m.indexOf(id)
                if (i == null) {
                    assertNull("$name: $slot laid out", slots[slot])
                    continue
                }
                val b = m.frame(i)
                assertEquals("$name: $slot centred on the disc", send.center.y, b.center.y, 0.01f)
                assertTrue("$name: $slot inside the composer", b.left >= cf.left - 0.01f && b.right <= cf.right + 0.01f && b.top >= cf.top - 0.01f && b.bottom <= cf.bottom + 0.01f)
                along.add((if (rtl) -b.right else b.left) to slot)
                if (slot == "trailing") assertTrue("$name: trailing before the disc", if (rtl) b.left >= send.right else b.right <= send.left)
            }
            val order = c["order"]?.array.orEmpty().mapNotNull { it.string }.filter { it == "leading" || it == "trailing" }
            assertEquals("$name: order", order, along.sortedBy { it.first }.map { it.second })
            for (step in c["steps"]?.array.orEmpty()) m.press(step["press"]!!.string!!)
            runCurrent()
            assertEquals("$name: events", c["events"]?.array.orEmpty().map { it["name"]?.string }, host.actions.map { it.name })
        }
    }

    /**
     * fixtures/interactions-round1.json `composerPaste` (#EDE-65 R): through
     * the MODEL, [composerPaste] with the clipboard's files (what the field
     * reads off the system clipboard, `composerClipboard`); not taken +
     * text = the field's own edit. The fired actions, the uploads (named by
     * the Composer) and /draft.
     */
    @Test
    fun composerPasteCasesAttachImages() = runTest {
        val f = Fixtures.json("interactions-round1.json")["composerPaste"]!!
        val cases = f["cases"]?.array.orEmpty()
        assertEquals(5, cases.size)
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val host = RecordingHost()
            val m = SurfaceModel("composer_paste", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, backgroundScope, noShaper)
            m.fixedMeasure = true
            m.setNested(f["surface"]!!.json)
            for ((k, v) in c["data"]?.obj.orEmpty()) m.setData("/$k", v)
            m.setViewport(390f, 800f)
            val field = m.idx("c")
            val files = c["clipboard"]?.get("files")?.array.orEmpty().map { PickedFile(it["name"]!!.string!!, it["size"]!!.number!!.toLong(), it["type"]!!.string!!) }
            val taken = m.composerPaste(field, files, files.map { "content://clip/${it.name}" })
            val text = c["clipboard"]?.get("text")?.string
            if (!taken && text != null) m.fieldEdited(field, (m.data["draft"]?.string ?: "") + text)
            advanceTimeBy(INPUT_DEBOUNCE_MS + 10)
            runCurrent()
            val want = c["events"]?.array.orEmpty()
            assertEquals("$name: names", want.map { it["name"]?.string }, host.actions.map { it.name })
            assertEquals("$name: contexts", want.map { it["context"] ?: JsonValue.Obj(emptyMap()) }, host.actions.map { it.context })
            assertEquals("$name: uploads", c["uploads"]?.array.orEmpty().map { it.string }, host.uploads.flatMap { u -> u.files.map { it.name } })
            assertTrue("$name: the upload names the Composer", host.uploads.all { it.componentId == "c" && it.name == "c" && it.uris.size == it.files.size })
            assertEquals("$name: /draft", c["draft"] ?: JsonValue.Null, m.data["draft"] ?: JsonValue.Null)
        }
    }

    /** #EDE-65: the five menu keys pass unmodified; a held modifier or another key is the field's. */
    @Test
    fun composerMenuKeyNames() {
        for (k in listOf("ArrowUp", "ArrowDown", "Enter", "Tab", "Escape")) {
            assertEquals(k, composerMenuKey(k, modified = false))
            assertNull(composerMenuKey(k, modified = true))
        }
        assertNull(composerMenuKey("ArrowLeft", modified = false))
        assertNull(composerMenuKey("a", modified = false))
    }

    /** #EXP-1629: Android key codes → the canonical KeyRecorder names (the physical key). */
    @Test
    fun keyRecorderKeyNamesAreCanonical() {
        val want = mapOf(
            AndroidKeyEvent.KEYCODE_A to "a", AndroidKeyEvent.KEYCODE_K to "k", AndroidKeyEvent.KEYCODE_Z to "z",
            AndroidKeyEvent.KEYCODE_0 to "0", AndroidKeyEvent.KEYCODE_1 to "1", AndroidKeyEvent.KEYCODE_9 to "9",
            AndroidKeyEvent.KEYCODE_F1 to "f1", AndroidKeyEvent.KEYCODE_F12 to "f12", AndroidKeyEvent.KEYCODE_F13 to "f13", AndroidKeyEvent.KEYCODE_F24 to "f24",
            AndroidKeyEvent.KEYCODE_SPACE to "space", AndroidKeyEvent.KEYCODE_ENTER to "enter", AndroidKeyEvent.KEYCODE_NUMPAD_ENTER to "enter",
            AndroidKeyEvent.KEYCODE_TAB to "tab", AndroidKeyEvent.KEYCODE_ESCAPE to "escape", AndroidKeyEvent.KEYCODE_BACK to "escape",
            AndroidKeyEvent.KEYCODE_DEL to "backspace", AndroidKeyEvent.KEYCODE_FORWARD_DEL to "delete", AndroidKeyEvent.KEYCODE_INSERT to "insert",
            AndroidKeyEvent.KEYCODE_MOVE_HOME to "home", AndroidKeyEvent.KEYCODE_MOVE_END to "end",
            AndroidKeyEvent.KEYCODE_PAGE_UP to "pageup", AndroidKeyEvent.KEYCODE_PAGE_DOWN to "pagedown",
            AndroidKeyEvent.KEYCODE_DPAD_UP to "up", AndroidKeyEvent.KEYCODE_DPAD_DOWN to "down",
            AndroidKeyEvent.KEYCODE_DPAD_LEFT to "left", AndroidKeyEvent.KEYCODE_DPAD_RIGHT to "right",
            AndroidKeyEvent.KEYCODE_MINUS to "-", AndroidKeyEvent.KEYCODE_EQUALS to "=",
            AndroidKeyEvent.KEYCODE_LEFT_BRACKET to "[", AndroidKeyEvent.KEYCODE_RIGHT_BRACKET to "]",
            AndroidKeyEvent.KEYCODE_BACKSLASH to "\\", AndroidKeyEvent.KEYCODE_SEMICOLON to ";",
            AndroidKeyEvent.KEYCODE_APOSTROPHE to "'", AndroidKeyEvent.KEYCODE_COMMA to ",",
            AndroidKeyEvent.KEYCODE_PERIOD to ".", AndroidKeyEvent.KEYCODE_SLASH to "/", AndroidKeyEvent.KEYCODE_GRAVE to "`",
        )
        for ((code, name) in want) assertEquals("key code $code", name, keyRecorderKeyName(code))
        for (code in listOf(AndroidKeyEvent.KEYCODE_NUMPAD_1, AndroidKeyEvent.KEYCODE_VOLUME_UP, AndroidKeyEvent.KEYCODE_CTRL_LEFT, AndroidKeyEvent.KEYCODE_SHIFT_RIGHT, AndroidKeyEvent.KEYCODE_AT, AndroidKeyEvent.KEYCODE_UNKNOWN)) {
            assertNull("key code $code", keyRecorderKeyName(code))
        }
        // Every canonical name is reached: 86 names (a-z, 0-9, f1-f24, 15 named, 11 punctuation).
        assertEquals(26 + 10 + 24 + 15 + 11, (0..400).mapNotNull(::keyRecorderKeyName).toSet().size)
    }

    /** #EXP-1629: the shifted digit reads its key; modifiers alone; repeats; unknown keys. */
    @Test
    fun keyRecorderKeyPayloads() {
        val shift = AndroidKeyEvent.META_SHIFT_ON or AndroidKeyEvent.META_SHIFT_LEFT_ON
        val ctrl = AndroidKeyEvent.META_CTRL_ON or AndroidKeyEvent.META_CTRL_LEFT_ON
        assertEquals(keyRecorderPayload("down", false, false, true, false, key = "1"), keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_1, true, shift))
        assertEquals(keyRecorderPayload("up", true, false, false, false, key = "k"), keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_K, false, ctrl))
        // A modifier alone: the flags after the event, whatever the event's meta state says about itself.
        assertEquals(keyRecorderPayload("modifiers", true, false, false, false), keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_CTRL_LEFT, true, 0))
        assertEquals(keyRecorderPayload("modifiers", true, false, true, false), keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_SHIFT_LEFT, true, ctrl))
        assertEquals(keyRecorderPayload("modifiers", false, false, true, false), keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_CTRL_LEFT, false, ctrl or shift))
        // The other side still held keeps the flag.
        assertEquals(
            keyRecorderPayload("modifiers", true, false, false, false),
            keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_CTRL_LEFT, false, AndroidKeyEvent.META_CTRL_ON or AndroidKeyEvent.META_CTRL_LEFT_ON or AndroidKeyEvent.META_CTRL_RIGHT_ON),
        )
        assertEquals(keyRecorderPayload("modifiers", false, false, false, true), keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_META_RIGHT, true, 0))
        assertEquals(keyRecorderPayload("modifiers", false, true, false, false), keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_ALT_RIGHT, true, 0))
        assertNull(keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_K, true, ctrl, repeatCount = 1))
        assertNull(keyRecorderKeyPayload(AndroidKeyEvent.KEYCODE_VOLUME_UP, true, 0))
    }

    /**
     * fixtures/interactions-round1.json `tree` (#VAPP-285): a press on a row
     * (leaf = select, branch = fold, lazy = load), or a key through
     * [handleKey] on the focused treeitem (the core's tree keys move the
     * focus — its `focus` event lands in `focusRequest` — or fold, rtl-aware);
     * the events the host sees, where the focus lands, /open and /picked.
     */
    @Test
    fun treeCasesSelectFoldFetchAndMoveTheFocus() {
        val t = Fixtures.json("interactions-round1.json")["tree"]!!
        val rowIds = t["rowIds"]!!.obj!!.mapValues { it.value.string!! }
        val cases = t["cases"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        val keys = mapOf("ArrowUp" to "up", "ArrowDown" to "down", "ArrowLeft" to "left", "ArrowRight" to "right", "Home" to "home", "End" to "end")
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val tree = buildMap<String, JsonValue> {
                put("id", JsonValue.Str("root"))
                put("component", JsonValue.Str("Box"))
                put("children", JsonValue.Arr(listOf(t["surface"]!!)))
                if (c["rtl"]?.bool == true) put("style", JsonValue.Obj(mapOf("direction" to JsonValue.Str("rtl"))))
            }
            val host = RecordingHost()
            val m = SurfaceModel("tree", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
            m.fixedMeasure = true
            m.setNested(JsonValue.Obj(tree).json)
            m.setViewport(390f, 800f)
            m.setData("/open", c["open"] ?: JsonValue.Arr(emptyList()))
            val step = c["step"]!!
            val item = (step["press"] ?: step["on"])!!.string!!
            val target = rowIds[item]!!
            assertNotNull("$name: no $target", m.indexOf(target))
            var moved: String? = null
            if (step["press"] != null) {
                m.press(target)
            } else {
                m.focus(target, true, fromKeyboard = true)
                val key = keys[step["key"]?.string]!!
                assertTrue("$name: $key is the tree's", handleKey(m, key))
                val now = m.focusRequest
                if (now != null && now != target) moved = rowIds.entries.firstOrNull { it.value == now }?.key ?: now
            }
            val after = c["after"]!!
            assertEquals("$name: events", c["events"]?.array.orEmpty().map { it.string }, host.actions.map { it.name })
            assertEquals("$name: focus", after["focus"]?.string, moved)
            assertEquals("$name: /open", after["open"] ?: JsonValue.Null, m.data["open"] ?: JsonValue.Null)
            assertEquals("$name: /picked", after["picked"] ?: JsonValue.Null, m.data["picked"] ?: JsonValue.Null)
            after["visible"]?.array?.let { visible ->
                val shown = rowIds.filter { m.indexOf(it.value) != null }.keys.sorted()
                assertEquals("$name: visible rows", visible.map { it.string!! }.sorted(), shown)
            }
        }
    }

    /**
     * fixtures/interactions-round1.json `fieldWrites` (#VAPP-288): an Input
     * `draft` bound to /draft and a Button `save` whose press is
     * `functionCall set`. A host write (setData) is an echo a focused field
     * ignores; an action's `set` (a programmatic write: the write epoch
     * moves) lands even while focused, the outstanding edit dropped.
     */
    @Test
    fun fieldWritesCasesAdoptProgrammaticSetsWhileFocused() = runTest {
        val cases = Fixtures.json("interactions-round1.json")["fieldWrites"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val children = ArrayList<JsonValue>()
            children.add(JsonValue.parse("""{"id":"draft","component":"Input","props":{"label":"Draft","value":{"@path":"/draft"}}}"""))
            c["pressPath"]?.string?.let { path ->
                val set = JsonValue.Obj(mapOf("@call" to JsonValue.Str("set"), "args" to JsonValue.Obj(mapOf("path" to JsonValue.Str(path), "value" to c["pressValue"]!!))))
                children.add(
                    JsonValue.Obj(
                        mapOf(
                            "id" to JsonValue.Str("save"),
                            "component" to JsonValue.Str("Button"),
                            "props" to JsonValue.Obj(mapOf("label" to JsonValue.Str("Save"))),
                            "on" to JsonValue.Obj(mapOf("press" to JsonValue.Obj(mapOf("functionCall" to set)))),
                        ),
                    ),
                )
            }
            val tree = JsonValue.Obj(mapOf("id" to JsonValue.Str("root"), "component" to JsonValue.Str("Box"), "children" to JsonValue.Arr(children)))
            val m = SurfaceModel("fw", SurfaceOptions(), RecordingHost(), backgroundScope, noShaper)
            m.fixedMeasure = true
            m.setNested(tree.json)
            m.setData("/draft", c["seed"]!!)
            m.setViewport(390f, 800f)
            val field = m.idx("draft.field")
            assertNotNull("$name: field state", m.field(field))
            for (step in c["steps"]?.array.orEmpty()) {
                when (step["op"]?.string) {
                    "focus" -> m.fieldFocused(field, true)
                    "blur" -> m.fieldFocused(field, false)
                    "type" -> {
                        m.fieldEdited(field, step["text"]!!.string!!)
                        advanceTimeBy(INPUT_DEBOUNCE_MS + 10)
                        runCurrent()
                    }
                    "hostWrite" -> m.setData(step["path"]!!.string!!, step["value"])
                    "press" -> m.press("save")
                    else -> error("$name: unknown step $step")
                }
            }
            val expect = c["expect"]!!
            assertEquals("$name: text", expect["text"]?.string, m.fieldText(field))
            expect["data"]?.let { assertEquals("$name: /draft", it, m.data["draft"] ?: JsonValue.Null) }
        }
    }

    /**
     * #VAPP-288: an action's `set` lands while the edit is still inside the
     * debounce: the press sends the edit first, then the set (back to the
     * pre-edit value) wins, the late `change` never fires.
     */
    @Test
    fun aSetBackToThePreEditValueLandsMidDebounce() = runTest {
        val host = RecordingHost()
        val m = SurfaceModel("fw", SurfaceOptions(), host, backgroundScope, noShaper)
        m.fixedMeasure = true
        m.setNested(
            """{"id":"root","component":"Box","children":[{"id":"draft","component":"Input","props":{"label":"Draft","value":{"@path":"/draft"}}},{"id":"save","component":"Button","props":{"label":"Save"},"on":{"press":{"functionCall":{"@call":"set","args":{"path":"/draft","value":"x"}}}}}]}""",
        )
        m.setData("/draft", JsonValue.Str("x"))
        m.setViewport(390f, 800f)
        val field = m.idx("draft.field")
        m.fieldFocused(field, true)
        m.fieldEdited(field, "xyz")
        val gen = m.field(field)!!.writeGeneration
        m.press("save")
        assertEquals("x", m.fieldText(field))
        assertEquals(JsonValue.Str("x"), m.data["draft"])
        assertTrue("the view reloads its value", m.field(field)!!.writeGeneration > gen)
        assertFalse(m.field(field)!!.pendingChange)
        val sent = host.inputs.size
        advanceTimeBy(500)
        assertEquals("no late change", sent, host.inputs.size)
        assertEquals(JsonValue.Str("x"), m.data["draft"])
    }
}
