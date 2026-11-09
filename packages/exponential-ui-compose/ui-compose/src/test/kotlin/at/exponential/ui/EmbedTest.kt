package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.host.SurfaceActionEvent
import at.exponential.ui.host.SurfaceFunctionCallEvent
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.shownText
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.model.EMBED_ERROR_ROOT
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.embed
import at.exponential.ui.model.embedCard
import at.exponential.ui.model.embedModel
import at.exponential.ui.model.embedMount
import at.exponential.ui.model.embedPayload
import at.exponential.ui.model.embedSurfaceId
import at.exponential.ui.model.press
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** A host that records the actions and the host-function calls it hears. */
private class EmbedHost : HostPlugin {
    val actions = ArrayList<SurfaceActionEvent>()
    val functions = ArrayList<SurfaceFunctionCallEvent>()

    override fun onAction(event: SurfaceActionEvent) {
        actions.add(event)
    }

    override fun onFunctionCall(event: SurfaceFunctionCallEvent) {
        functions.add(event)
    }

    override fun onFunctionCallThen(event: SurfaceFunctionCallEvent, then: () -> Unit) {
        functions.add(event)
        then()
    }
}

/**
 * The texts a surface paints, in pre-order, every Embed's nested surface
 * in its place (recursively): Text `text` and Button `label`, hidden
 * subtrees skipped.
 */
private fun painted(m: SurfaceModel): List<String> {
    val out = ArrayList<String>()
    val hidden = HashSet<Int>()
    for (n in m.nodes) {
        if (n.hidden || n.parent?.let { it in hidden } == true) {
            hidden.add(n.index)
            continue
        }
        when (n.component) {
            "Embed" -> m.embed(n.index)?.let { out.addAll(painted(it)) }
            "Text" -> n.props.shownText("text").takeIf { it.isNotEmpty() }?.let(out::add)
            "Button" -> n.props.shownText("label").takeIf { it.isNotEmpty() }?.let(out::add)
        }
    }
    return out
}

/** The painted Embeds of the host, in paint order. */
private fun SurfaceModel.paintedEmbeds(): List<Int> {
    val hidden = HashSet<Int>()
    val out = ArrayList<Int>()
    for (n in nodes) {
        if (n.hidden || n.parent?.let { it in hidden } == true) {
            hidden.add(n.index)
            continue
        }
        if (n.component == "Embed") out.add(n.index)
    }
    return out
}

/** Every `texts` entry painted in order, no `absent` one. */
private fun assertTexts(name: String, got: List<String>, case: JsonValue) {
    var at = 0
    for (t in case["texts"]?.array.orEmpty().mapNotNull { it.string }) {
        val i = got.subList(at, got.size).indexOf(t)
        assertTrue("$name: \"$t\" painted in order in $got", i >= 0)
        at += i + 1
    }
    for (t in case["absent"]?.array.orEmpty().mapNotNull { it.string }) {
        assertTrue("$name: \"$t\" not painted in $got", got.none { it.contains(t) })
    }
}

/** The fixture's host under the neutral theme, the core's fixed measure (no shaper), 390 wide. */
private fun mount(case: JsonValue, host: HostPlugin): SurfaceModel {
    val m = SurfaceModel("host", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined), noShaper)
    m.fixedMeasure = true
    case["data"]?.let { m.setData("", it) }
    m.setNested(case["root"]!!.json)
    m.setViewport(390f, 800f)
    return m
}

/**
 * #EDE-65 lane M: `fixtures/embed.json` on the Compose painter's model
 * (the core's fixed measure): `parse` through [embedCard], `render` (every
 * Embed's nested surface in its place) and `events` (an inner press reaches
 * the host as the Embed's `action`; a host function is denied; an inner
 * `set` stays inner).
 */
class EmbedTest {
    private val f by lazy { Fixtures.json("embed.json") }

    @Before
    fun setUp() = Fixtures.require()

    @Test
    fun theErrorRootIsTheFixtures() {
        assertEquals(f["errorRoot"], EMBED_ERROR_ROOT)
        val bad = embedMount(JsonValue.Str("hello"))
        assertFalse(bad.valid)
        assertEquals(EMBED_ERROR_ROOT, bad.card.root)
        assertTrue(bad.card.data.isEmpty())
    }

    @Test
    fun parseCases() {
        val cases = f["parse"]?.array.orEmpty()
        assertTrue(cases.size >= 20)
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val card = embedCard(c["surface"])
            val valid = c["valid"]?.bool == true
            assertEquals("$name: valid", valid, card != null)
            assertEquals("$name: mount", valid, embedMount(c["surface"]).valid)
            if (card != null) {
                assertEquals("$name: data", c["data"], JsonValue.Obj(card.data))
                assertEquals("$name: root", c["surface"]!!["root"], card.root)
            }
        }
        // A missing prop (an unbound `{"@path"}`) is not a card either.
        assertNull(embedCard(null))
    }

    @Test
    fun payloadAndSurfaceId() {
        val ctx = JsonValue.Obj(mapOf("text" to JsonValue.Str("Yes")))
        assertEquals(JsonValue.Obj(mapOf("name" to JsonValue.Str("reply"), "context" to ctx)), embedPayload("reply", ctx))
        assertEquals(JsonValue.Obj(mapOf("name" to JsonValue.Str("go"), "context" to JsonValue.Obj(emptyMap()))), embedPayload("go", null))
        assertEquals("host/card", embedSurfaceId("host", "card"))
    }

    @Test
    fun renderCases() {
        val cases = f["render"]?.array.orEmpty()
        assertTrue(cases.size >= 9)
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val m = mount(c, EmbedHost())
            assertTexts(name, painted(m), c)
            for (i in m.paintedEmbeds()) {
                val nested = m.embed(i)!!
                assertEquals("$name: nested surface id", embedSurfaceId(m.id, m.node(i)!!.id), nested.id)
                assertNotSame("$name: its own surface", m.surface, nested.surface)
            }
        }
    }

    @Test
    fun eventCases() {
        val cases = f["events"]?.array.orEmpty()
        assertTrue(cases.size >= 7)
        for (c in cases) {
            val name = c["name"]?.string ?: "?"
            val host = EmbedHost()
            val m = mount(c, host)
            val press = c["press"]!!
            val embeds = m.paintedEmbeds()
            val nested = m.embed(embeds[press["embed"]!!.number!!.toInt()])!!
            val target = nested.indexOf(press["node"]!!.string!!)
            assertNotNull("$name: inner node", target)
            nested.press(target!!)
            val want = c["actions"]?.array.orEmpty()
            assertEquals("$name: names", want.map { it["name"]?.string }, host.actions.map { it.name })
            assertEquals("$name: contexts", want.map { it["context"] ?: JsonValue.Obj(emptyMap()) }, host.actions.map { it.context })
            assertTrue("$name: every action is the Embed's", host.actions.all { it.event == "action" && it.surfaceId == m.id })
            assertEquals("$name: functions", c["functions"]?.array.orEmpty().map { it["name"]?.string }, host.functions.map { it.name })
            assertTexts(name, painted(m), c)
        }
    }

    @Test
    fun aChangedSurfaceRemountsFresh() {
        val c = f["events"]!!.array!!.first { it["name"]?.string == "set-stays-inner" }
        val m = mount(c, EmbedHost())
        val card = m.paintedEmbeds().single()
        val first = m.embed(card)!!
        first.press(first.indexOf("bump")!!)
        assertTrue(painted(m).contains("Inner after"))
        // The same value: the same mount (its inner state kept).
        assertSame(first, m.embed(card))
        // Another value: a fresh mount, the card's own data again.
        val root = c["root"]!!.obj!!
        val (hostCount, embedNode) = root.getValue("children").array!!
        val surface = embedNode["props"]!!["surface"]!!.obj!!
        val changed = JsonValue.Obj(surface + ("version" to JsonValue.Str("1.0")))
        val node = JsonValue.Obj(embedNode.obj!! + ("props" to JsonValue.Obj(mapOf("surface" to changed))))
        m.setNested(JsonValue.Obj(root + ("children" to JsonValue.Arr(listOf(hostCount, node)))).json)
        val again = m.embed(m.paintedEmbeds().single())!!
        assertNotSame(first, again)
        assertTrue(painted(m).contains("Inner before"))
        assertFalse(painted(m).contains("Inner after"))
        assertSame(again, m.embedModel(m.node(m.paintedEmbeds().single())!!.id))
    }
}

/**
 * #EDE-65 lane M: `fixtures/embed.json` `render` PAINTED (the real Compose
 * shaper, the measurer sizing each Embed by its nested surface): every
 * `texts` entry shows, no `absent` one; an inner Button tapped reaches the
 * host as the Embed's `action`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class EmbedPaintTest {
    @get:Rule
    val compose = createComposeRule()

    private val f by lazy { Fixtures.json("embed.json") }
    private val current = mutableStateOf<SurfaceModel?>(null)

    @Before
    fun setUp() = Fixtures.require()

    private fun show(c: JsonValue, host: HostPlugin): SurfaceModel {
        val density = RuntimeEnvironment.getApplication().resources.displayMetrics.density
        val shaper = robolectricShaper(FontResolver { null }, density)
        val m = SurfaceModel("host", SurfaceOptions(theme = ThemeHandle.builtin("neutral"), mode = Mode.Light), host, CoroutineScope(Dispatchers.Unconfined)) { shaper }
        c["data"]?.let { m.setData("", it) }
        m.setNested(c["root"]!!.json)
        if (current.value == null) {
            current.value = m
            compose.setContent { current.value?.let { model -> Box(Modifier.fillMaxWidth()) { ExponentialSurface(model) } } }
        } else {
            compose.runOnIdle { current.value = m }
        }
        compose.waitForIdle()
        return m
    }

    @Test
    fun renderCasesPaint() {
        for (c in f["render"]?.array.orEmpty()) {
            val name = c["name"]?.string ?: "?"
            val m = show(c, EmbedHost())
            for (t in c["texts"]?.array.orEmpty().mapNotNull { it.string }) {
                assertTrue("$name: \"$t\" painted", compose.onAllNodesWithText(t, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty())
            }
            for (t in c["absent"]?.array.orEmpty().mapNotNull { it.string }) {
                compose.onAllNodesWithText(t, substring = true, useUnmergedTree = true).assertCountEquals(0)
            }
            // The measurer sized each Embed by its card.
            for (i in m.paintedEmbeds()) {
                val nested = m.embedModel(m.node(i)!!.id)
                assertNotNull("$name: mounted by the measurer", nested)
                assertEquals("$name: as tall as its card", nested!!.surfaceSize.height, m.frame(i).height, 1f)
                assertTrue("$name: a card has height", m.frame(i).height > 0f)
            }
        }
    }

    @Test
    fun aTappedInnerButtonFiresTheEmbedsAction() {
        val c = f["events"]!!.array!!.first { it["name"]?.string == "reply-bubbles" }
        val host = EmbedHost()
        show(c, host)
        compose.onNode(hasContentDescription("Yes"), useUnmergedTree = true).performClick()
        compose.waitForIdle()
        val want = c["actions"]!!.array!!
        assertEquals(want.map { it["name"]?.string }, host.actions.map { it.name })
        assertEquals(want.map { it["context"] }, host.actions.map { it.context })
    }
}
