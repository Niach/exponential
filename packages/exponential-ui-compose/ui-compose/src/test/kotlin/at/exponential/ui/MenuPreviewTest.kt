package at.exponential.ui

import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.shownText
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.embed
import at.exponential.ui.model.escape
import at.exponential.ui.model.fieldEdited
import at.exponential.ui.model.fieldFocused
import at.exponential.ui.model.focus
import at.exponential.ui.model.hover
import at.exponential.ui.model.dismissLayer
import at.exponential.ui.model.isMenuPreviewLayer
import at.exponential.ui.model.press
import at.exponential.ui.model.setOpen
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** The texts a surface paints (Text `text`, Button `label`), every Embed's nested surface in its place. */
private fun paintedTexts(m: SurfaceModel): List<String> {
    val out = ArrayList<String>()
    for (n in m.nodes) {
        if (n.hidden || n.removed) continue
        when (n.component) {
            "Embed" -> m.embed(n.index)?.let { out.addAll(paintedTexts(it)) }
            "Text" -> n.props.shownText("text").takeIf { it.isNotEmpty() }?.let(out::add)
            "Button" -> n.props.shownText("label").takeIf { it.isNotEmpty() }?.let(out::add)
        }
    }
    return out
}

/**
 * #EXP-1629: `fixtures/interactions-round1.json` `menuRows` + `menuPreview`
 * on the Compose painter's model (the core's fixed measure): the core lays
 * out an item's `identifier` (between its icon and its label) and `detail`
 * (after the label); a row's `preview` shows as the menu's own layer while
 * the row is hovered ([hover], after 150 ms on the model's clock: the core's
 * `hoverTimer`) or keyboard-highlighted ([focus] from the keyboard, at
 * once), hides 80 ms after both left, never takes Escape, and its nested
 * surface's events fire the Menu's `action`.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class MenuPreviewTest {
    private val f by lazy { Fixtures.json("interactions-round1.json") }

    @Before
    fun setUp() = Fixtures.require()

    private fun mount(tree: JsonValue, host: RecordingHost, scope: kotlinx.coroutines.CoroutineScope): SurfaceModel {
        val m = SurfaceModel("menu", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, scope, noShaper)
        m.fixedMeasure = true
        m.setNested(tree.json)
        m.setViewport(390f, 800f)
        m.setOpen("m", true)
        return m
    }

    @Test
    fun menuRowCases() = runTest {
        val cases = f["menuRows"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val tree = JsonValue.Obj(mapOf("id" to JsonValue.Str("m"), "component" to JsonValue.Str("Menu"), "props" to JsonValue.Obj(mapOf("label" to JsonValue.Str("More"), "items" to c["items"]!!))))
            val m = mount(tree, RecordingHost(), backgroundScope)
            c["submenu"]?.number?.toInt()?.let { sub ->
                m.press("m.item.$sub")
                // A searchable submenu: type into its field.
                c["query"]?.string?.let { q ->
                    val field = m.indexOf("m.search.$sub")!!
                    m.fieldFocused(field, true)
                    m.fieldEdited(field, q)
                    advanceTimeBy(200)
                    runCurrent()
                }
                c["shown"]?.array?.let { shown ->
                    val list = m.nodes.first { it.id == "m.list.$sub" }
                    val got = list.children.map { child -> m.nodes.first { it.index == child } }.filter { it.part == "item" }.map { it.id.removePrefix("m.item.") }
                    assertEquals("$name: shown rows", shown.map { it.string }, got)
                }
            }
            for (r in c["rows"]?.array.orEmpty()) {
                val key = r["key"]!!.string!!
                val row = m.nodes.firstOrNull { it.id == "m.item.$key" }
                assertNotNull("$name: row $key", row)
                val parts = ArrayList<String>()
                for (child in row!!.children) {
                    val n = m.nodes.first { it.index == child }
                    when (n.part) {
                        "itemLabel" -> {
                            if (n.props["icon"]?.string != null) {
                                parts.add("icon")
                                r["iconColor"]?.string?.let { assertEquals("$name: $key tint", it, n.props["iconColor"]?.string) }
                            }
                            parts.add("label")
                            assertEquals("$name: $key label", r["label"]?.string, n.props.shownText("text"))
                        }
                        "itemIcon" -> parts.add("icon")
                        "check" -> parts.add("check")
                        "identifier" -> {
                            parts.add("identifier")
                            assertEquals("$name: $key identifier", r["identifier"]?.string, n.props.shownText("text"))
                        }
                        "detail" -> {
                            parts.add("detail")
                            assertEquals("$name: $key detail", r["detail"]?.string, n.props.shownText("text"))
                        }
                        "shortcut" -> parts.add("shortcut")
                        "submenuIndicator" -> parts.add("submenuIndicator")
                    }
                }
                assertEquals("$name: $key parts", r["parts"]?.array.orEmpty().map { it.string }, parts)
            }
        }
    }

    private fun shown(m: SurfaceModel): String? {
        val previews = m.layers.filter { m.isMenuPreviewLayer(it) }
        assertTrue("one preview at most", previews.size <= 1)
        return previews.firstOrNull()?.let { l -> m.nodes.first { it.index == l.root }.id.removePrefix("m.preview.") }
    }

    @Test
    fun menuPreviewCases() = runTest {
        val fx = f["menuPreview"]!!
        val cases = fx["cases"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) replay(fx, c)
    }

    private fun TestScope.replay(fx: JsonValue, c: JsonValue) {
        val name = c["name"]?.string ?: "?"
        val host = RecordingHost()
        val m = mount(fx["menu"]!!, host, backgroundScope)
        c["submenu"]?.number?.toInt()?.let { m.press("m.item.$it") }
        runCurrent()
        var hovered: String? = null
        var highlighted: String? = null
        for ((i, step) in c["steps"]?.array.orEmpty().withIndex()) {
            val before = host.actions.size
            if (step.obj!!.containsKey("hover")) {
                hovered?.let { m.hover("m.item.$it", false) }
                hovered = step["hover"]?.string
                hovered?.let { m.hover("m.item.$it", true) }
            }
            if (step.obj!!.containsKey("highlight")) {
                highlighted?.let { m.focus("m.item.$it", false) }
                highlighted = step["highlight"]?.string
                highlighted?.let { m.focus("m.item.$it", true, fromKeyboard = true) }
            }
            step["overPreview"]?.bool?.let { over ->
                val key = shown(m)
                assertNotNull("$name: step $i: a preview to hover", key)
                m.hover("m.preview.$key", over)
            }
            runCurrent()
            step["wait"]?.number?.let {
                advanceTimeBy(it.toLong())
                runCurrent()
            }
            step["press"]?.string?.let { inner ->
                val key = shown(m)!!
                val nested = m.embed(m.indexOf("m.preview.$key.surface")!!)!!
                nested.press(nested.indexOf(inner)!!)
                runCurrent()
            }
            val key = shown(m)
            assertEquals("$name: step $i: shown", step["shown"]?.string, key)
            if (key != null) {
                val texts = paintedTexts(m)
                for (t in step["texts"]?.array.orEmpty().mapNotNull { it.string }) assertTrue("$name: step $i: \"$t\" in $texts", texts.any { it.contains(t) })
            }
            val want = step["events"]?.array.orEmpty()
            val got = host.actions.drop(before).filter { it.name != "picked" }
            assertEquals("$name: step $i: names", want.map { it["name"]?.string }, got.map { it.name })
            assertEquals("$name: step $i: contexts", want.map { it["context"] ?: JsonValue.Obj(emptyMap()) }, got.map { it.context })
            assertTrue("$name: step $i: the menu stays open", m.layers.any { it.owner == "m" && !m.isMenuPreviewLayer(it) })
        }
        // A shown preview never takes Escape: the menu closes, its preview with it.
        if (shown(m) != null) {
            assertTrue(m.escape())
            runCurrent()
            assertFalse("$name: Escape closed the menu", m.layers.any { it.owner == "m" })
        }
    }

    /**
     * #EDE-253 `menuOpen`: a Menu's bindable `open` (Popover's rule) through
     * the core: a host write opens a press Menu and a context Menu; Escape,
     * an outside press (the layer's dismiss), a pick or the trigger's press
     * writes /open and fires `change {open}`; a host write fires nothing.
     */
    @Test
    fun menuOpenCases() = runTest {
        val fx = f["menuOpen"]!!
        val cases = fx["cases"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val host = RecordingHost()
            val m = SurfaceModel("menu", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, backgroundScope, noShaper)
            m.fixedMeasure = true
            m.setNested(fx["surfaces"]!![c["surface"]!!.string!!]!!.json)
            m.setViewport(390f, 800f)
            for ((k, v) in c["data"]?.obj.orEmpty()) m.setData("/$k", v)
            runCurrent()
            for ((i, step) in c["steps"]?.array.orEmpty().withIndex()) {
                val at = "$name: step $i (${step["do"]?.string})"
                val before = host.actions.size
                when (step["do"]?.string) {
                    "open" -> m.setData("/open", JsonValue.Bool(true))
                    "close" -> m.setData("/open", JsonValue.Bool(false))
                    "escape" -> assertTrue("$at: Escape handled", m.escape())
                    "outside" -> m.dismissLayer("m")
                    "pick" -> m.press("m.item.0")
                    "pressTrigger" -> m.press("t")
                }
                runCurrent()
                assertEquals("$at: shown", step["shown"]?.bool, m.layers.any { it.owner == "m" && !m.isMenuPreviewLayer(it) })
                step["data"]?.bool?.let { assertEquals("$at: /open", it, m.data["open"]?.bool) }
                val fired = host.actions.drop(before)
                assertEquals("$at: changes", step["changes"]?.array.orEmpty().map { it.bool }, fired.filter { it.name == "change" }.map { it.context["open"]?.bool })
                assertEquals("$at: selects", step["selects"]?.array.orEmpty().map { it.string }, fired.filter { it.name == "select" }.map { it.context["value"]?.string })
            }
        }
    }

    /**
     * #EDE-264 / #EDE-270 `eventArgs` through the core: an action's relative
     * paths read the event's payload first (a Menu `select`'s {value} in a
     * host function's args, a `set`'s value, the event's context), then the
     * scope; a Toggle with only `on.change` is a press target (`pressable`,
     * what the tap gesture keys on) and each press runs its function.
     */
    @Test
    fun eventArgsCases() = runTest {
        val fx = f["eventArgs"]!!
        val cases = fx["cases"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val host = RecordingHost()
            val m = SurfaceModel("ea", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, backgroundScope, noShaper)
            m.fixedMeasure = true
            val surface = fx["surfaces"]!![c["surface"]!!.string!!] as JsonValue.Obj
            val kids = (surface.v["children"] as JsonValue.Arr).v
            val first = JsonValue.Obj((kids[0] as JsonValue.Obj).v + ("on" to c["on"]!!))
            m.setNested(JsonValue.Obj(surface.v + ("children" to JsonValue.Arr(listOf(first) + kids.drop(1)))).json)
            m.setViewport(390f, 800f)
            for ((k, v) in c["data"]?.obj.orEmpty()) m.setData("/$k", v)
            runCurrent()
            c["pressable"]?.bool?.let { want -> assertEquals("$name: pressable", want, m.indexOf("t")?.let(m::node)?.pressable) }
            for (step in c["steps"]?.array.orEmpty()) {
                m.press(if (step.string == "pick") "m.item.0" else "t")
                runCurrent()
            }
            assertEquals("$name: calls", c["calls"], JsonValue.Arr(host.calls.map { JsonValue.Obj(mapOf("name" to JsonValue.Str(it.name), "args" to JsonValue.Obj(it.args))) }))
            assertEquals("$name: events", c["events"], JsonValue.Arr(host.actions.map { JsonValue.Obj(mapOf("name" to JsonValue.Str(it.name), "context" to it.context)) }))
            for ((k, v) in c["after"]?.obj.orEmpty()) assertEquals("$name: /$k", v, m.data[k])
        }
    }
}
