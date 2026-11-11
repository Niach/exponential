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
}
