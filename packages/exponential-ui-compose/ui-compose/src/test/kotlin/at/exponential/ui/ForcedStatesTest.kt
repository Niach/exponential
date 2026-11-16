package at.exponential.ui

import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.setInteraction
import at.exponential.ui.model.InteractionState
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.PaintStyle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlin.math.abs

/**
 * #EXP-1781 (fixtures/forced-states.json): the host's FORCED interaction
 * states reach the core through [SurfaceModel.setForcedStates] and join its
 * style resolution only: the visuals the model paints change, no layer
 * opens, no event reaches the host.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ForcedStatesTest {
    private fun model(host: RecordingHost, tree: String): SurfaceModel {
        val m = makeModel(id = "forced", theme = "exponential", mode = Mode.Light, width = 480f, fixed = true, host = host)
        m.setNested(tree)
        m.setViewport(480f, 900f)
        return m
    }

    private fun strings(v: JsonValue?): List<String> = v?.array.orEmpty().mapNotNull { it.string }

    private fun stateOf(states: List<String>) = InteractionState(
        hover = "hover" in states,
        pressed = "pressed" in states,
        focus = "focus" in states,
        focusVisible = "focus-visible" in states,
    )

    @Test
    fun everyForcedStatesCaseReplays() {
        Fixtures.require()
        val cases = Fixtures.json("forced-states.json")["cases"]?.array.orEmpty()
        assertTrue(cases.size >= 7)
        for (c in cases) {
            val name = c["name"]?.string ?: ""
            val host = RecordingHost()
            val m = model(host, c["tree"]!!.json)
            var dropped = emptyList<String>()
            for (step in c["steps"]?.array.orEmpty()) {
                step["forced"]?.obj?.let { f -> dropped = m.setForcedStates(f.mapValues { (_, v) -> strings(v) }) }
                step["host"]?.let { h -> m.setInteraction(h["id"]!!.string!!) { stateOf(strings(h["states"])) } }
            }
            m.pass()
            val e = c["expect"]!!
            // The order is unspecified across hosts (the FFI sorts map keys): a set.
            assertEquals("$name: dropped", strings(e["dropped"]).toSet(), dropped.toSet())
            for ((id, want) in e["states"]?.obj.orEmpty()) {
                assertEquals("$name: states of $id", strings(want), m.forcedStatesOf(id))
            }
            for ((id, want) in e["visuals"]?.obj.orEmpty()) {
                val i = m.indexOf(id) ?: error("$name: no node $id")
                val v = m.surface.visual(i.toUInt()) ?: error("$name: no visual of $id")
                for ((k, w) in want.obj.orEmpty()) {
                    when (k) {
                        "backgroundColor" -> assertEquals("$name: $id.$k", w.string, v.backgroundColor)
                        "borderColor" -> assertEquals("$name: $id.$k", w.string, v.borderColor)
                        "color" -> assertEquals("$name: $id.$k", w.string, v.color)
                        "opacity" -> assertTrue("$name: $id.$k = ${v.opacity}, want ${w.number}", abs((v.opacity ?: 1f) - (w.number ?: 1.0).toFloat()) < 1e-4)
                        else -> error("$name: unchecked visual key $k")
                    }
                }
                // What the Compose painter reads is the core's visual (no painter work).
                assertEquals("$name: $id paints the core's visual", PaintStyle(v), m.style(i))
            }
            assertEquals("$name: layers", e["layers"]?.number?.toInt() ?: 0, m.layers.size)
            assertTrue("$name: events ${host.actions}", host.actions.isEmpty() && host.inputs.isEmpty())
        }
    }

    @Test
    fun forcedStatesNeverReachTheHostStates() {
        val host = RecordingHost()
        val m = model(host, """{"id":"btn","component":"Button","props":{"label":"Save","variant":"ghost"}}""")
        val i = m.indexOf("btn")!!
        val rest = m.style(i)
        assertEquals(emptyList<String>(), m.setForcedStates(mapOf("btn" to listOf("hover"))))
        assertNotEquals("a forced hover restyles", rest, m.style(i))
        assertTrue("paint only: the host's own states stay empty", m.statesOf("btn").isEmpty())
        assertEquals(listOf("hover"), m.paintStatesOf("btn"))
        m.setForcedStates(emptyMap())
        assertEquals("an empty map restores the look", rest, m.style(i))
    }

    @Test
    fun aForcedHoverRestylesTheSegmentedItemItPaints() {
        // The Segmented item is drawn by the painter (`<id>.item.<k>`, gpui
        // `paint_segmented`): its look adds the forced states of that id.
        val m = model(
            RecordingHost(),
            """{"id":"seg","component":"Segmented","props":{"variant":"segmented","items":[{"label":"One","value":"one"},{"label":"Two","value":"two"}],"value":"one"}}""",
        )
        val props = m.ownerProps(m.indexOf("seg")!!)
        val rest = m.part("Segmented", "item", props, m.forcedStatesOf("seg.item.1"))
        m.setForcedStates(mapOf("seg.item.1" to listOf("hover")))
        assertEquals(listOf("hover"), m.forcedStatesOf("seg.item.1"))
        assertTrue("a sibling keeps its look", m.forcedStatesOf("seg.item.0").isEmpty())
        val hovered = m.part("Segmented", "item", props, m.forcedStatesOf("seg.item.1"))
        assertNotEquals("the forced hover changes the item's style", rest.style, hovered.style)
        // A key on the node covers its items.
        m.setForcedStates(mapOf("seg" to listOf("hover")))
        assertEquals(listOf("hover"), m.forcedStatesOf("seg.item.0"))
    }
}
