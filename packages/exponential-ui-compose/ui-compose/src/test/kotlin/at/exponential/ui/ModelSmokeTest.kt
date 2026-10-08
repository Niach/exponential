package at.exponential.ui

import androidx.compose.ui.graphics.Color
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.checked
import at.exponential.ui.model.press
import at.exponential.ui.model.sliderDrag
import at.exponential.ui.model.sliderRelease
import at.exponential.ui.model.sliderValue
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownBlockKind
import at.exponential.ui.primitives.parseHexColor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** The model half headless: the kitchen sink under the core's fixed measure. */
class ModelSmokeTest {
    private class RecordingHost : HostPlugin {
        val actions = ArrayList<SurfaceActionEvent>()
        override fun onAction(event: SurfaceActionEvent) {
            actions.add(event)
        }
    }

    private fun sink(host: HostPlugin): SurfaceModel {
        val m = SurfaceModel("kitchen-sink", host = host, scope = CoroutineScope(Dispatchers.Unconfined)) {
            error("the shaper is never called under fixed measure")
        }
        m.fixedMeasure = true
        val dir = System.getProperty("exponential.ui.fixtures")
        m.setNested(File(dir, "kitchen-sink.json").readText())
        m.setData("/draft", JsonValue.Obj(mapOf("title" to JsonValue.Str(""))))
        m.setViewport(390f, 800f)
        return m
    }

    @Test
    fun kitchenSinkLaysOutAndInteracts() {
        val host = RecordingHost()
        val m = sink(host)
        assertTrue("nodes ${m.nodes.size}", m.nodes.size > 150)
        assertEquals(m.nodes.size, m.frames.size)
        assertEquals(m.nodes.size, m.styles.size)
        assertTrue("height ${m.surfaceSize.height}", m.surfaceSize.height > 1000f)
        assertTrue("issues ${m.issues}", m.issues.isEmpty())

        m.press("hdr-scan")
        assertEquals(listOf("scan"), host.actions.map { it.name })
        assertEquals("hdr-scan", host.actions[0].componentId)
        assertEquals("press", host.actions[0].event)

        val box = m.indexOf("form-agree.box")!!
        assertTrue(m.checked(box))
        val before = m.style(box).background
        m.press(box)
        assertFalse(m.checked(box))
        // Round 1: the core keeps the unbound value and restyles the box itself.
        assertNotEquals(before, m.style(box).background)

        val run = m.indexOf("tabs.tab.1")!!
        assertTrue(m.node(m.indexOf("tab-run")!!)!!.hidden)
        m.press(run)
        assertFalse(m.node(m.indexOf("tab-run")!!)!!.hidden)
        assertTrue(m.node(m.indexOf("tab-issue")!!)!!.hidden)
        assertTrue(m.node(run)!!.selected)

        val track = m.indexOf("form-volume.track")!!
        m.sliderDrag(track, SurfaceModel.snap(42.0, 0.0, 100.0, 5.0))
        assertEquals(40.0, m.sliderValue(track), 0.0)
        m.sliderRelease(track)
        assertEquals(40.0, m.sliderValue(track), 0.0)
        assertEquals(0.3, SurfaceModel.snap(0.30000000004, 0.0, 1.0, 0.1), 0.0)
    }

    @Test
    fun jsonRoundTripsWithSortedKeys() {
        val v = JsonValue.parse("""{"b":[1,2.5,true,null,"x\"y\n"],"a":{"z":false,"c":-3e2}}""")
        assertEquals("""{"a":{"c":-300,"z":false},"b":[1,2.5,true,null,"x\"y\n"]}""", v.json)
        assertEquals(v, JsonValue.parse(v.json))
        assertEquals(JsonValue.Bool(true), v["b"]!!.array!![2])
        assertEquals("3", JsonValue.Num(3.0).displayText)
        assertEquals(JsonValue.Null, JsonValue.parse("{oops"))
        assertEquals("é😀", JsonValue.parse("\"\\u00e9\\ud83d\\ude00\"").string)
    }

    @Test
    fun hexColoursAndMarkdown() {
        assertEquals(Color(1f, 0f, 0f, 1f), parseHexColor("#f00"))
        assertEquals(Color(0f, 1f, 0f, 1f), parseHexColor("#00ff00"))
        assertEquals(0x80 / 255f, parseHexColor("#0000ff80")!!.alpha, 0.001f)
        assertNull(parseHexColor("red"))
        assertNull(parseHexColor("#12"))
        val blocks = Markdown.parse("# Title\n\nSome **bold** and `code` [link](https://x.y).\n\n- a\n- [x] b\n\n| h1 | h2 |\n| --- | :-: |\n| 1 | 2 |")
        assertEquals(listOf("Heading", "Paragraph", "ListItem", "ListItem", "Table"), blocks.map { it.kind::class.simpleName })
        assertEquals(MarkdownBlockKind.ListItem("•", true), blocks[3].kind)
        val spans = blocks[1].inlines
        assertTrue(spans.any { it.bold && it.text == "bold" })
        assertTrue(spans.any { it.code && it.text == "code" })
        assertTrue(spans.any { it.link == "https://x.y" && it.text == "link" })
    }
}
