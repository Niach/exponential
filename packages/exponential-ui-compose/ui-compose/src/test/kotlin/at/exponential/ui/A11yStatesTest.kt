package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * #EXP-1629 (fixtures/a11y-states.json): the author's `accessibility.selected`
 * / `.expanded` as TalkBack gets them: `selected`, the expand / collapse
 * actions; an unset key = no state.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-xhdpi")
class A11yStatesTest {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private fun model(case: JsonValue): SurfaceModel {
        val m = SurfaceModel("t", SurfaceOptions(), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
        m.fixedMeasure = true
        val outcome = m.setNested(case["tree"].toString())
        assertEquals(case["name"]?.string, emptyList<JsonValue>(), JsonValue.parse(outcome.issuesJson).array.orEmpty())
        case["data"]?.obj?.forEach { (k, v) -> m.setData("/$k", v) }
        m.setViewport(390f, 800f)
        return m
    }

    /** The node a label names, as TalkBack reads it (merged). */
    private fun config(label: String) = compose.onNode(hasContentDescription(label)).fetchSemanticsNode().config

    @Test
    fun theFixtureReplays() {
        for (case in Fixtures.json("a11y-states.json")["cases"]!!.array!!) {
            val name = case["name"]?.string
            val m = model(case)
            compose.setContent { Box(Modifier.fillMaxWidth()) { ExponentialSurface(m) } }
            compose.waitForIdle()
            // Each node's label: its literal, or its template row's `label`.
            fun labelOf(id: String): String {
                val rows = case["data"]?.get("rows")?.array.orEmpty()
                val key = id.substringAfter('.', "")
                return rows.firstOrNull { it["key"]?.string == key }?.get("label")?.string
                    ?: case["tree"]!!["children"]!!.array!!.first { it["id"]?.string == id }["accessibility"]!!["label"]!!.string!!
            }
            for ((id, states) in case["expect"]!!.obj!!) {
                val c = config(labelOf(id))
                states["selected"]?.bool?.let { assertEquals("$name $id selected", it, c.getOrNull(SemanticsProperties.Selected)) }
                states["expanded"]?.bool?.let {
                    assertEquals("$name $id collapse", it, c.getOrNull(SemanticsActions.Collapse) != null)
                    assertEquals("$name $id expand", !it, c.getOrNull(SemanticsActions.Expand) != null)
                }
            }
            for (id in case["unset"]!!.array!!.mapNotNull { it.string }) {
                val c = config(labelOf(id))
                assertEquals("$name $id", null, c.getOrNull(SemanticsProperties.Selected))
                assertEquals("$name $id", null, c.getOrNull(SemanticsActions.Expand) ?: c.getOrNull(SemanticsActions.Collapse))
            }
        }
    }
}
