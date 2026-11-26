package at.exponential.ui.primitives

import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** Group D: fixtures/data-viz.json replays through [DataViz]. */
class DataVizGeometryTest {
    private val sdk: File = File("../../exponential-ui")

    @Suppress("UNCHECKED_CAST")
    private fun Any?.obj(): Map<String, Any?> = this as Map<String, Any?>

    @Suppress("UNCHECKED_CAST")
    private fun Any?.arr(): List<Any?> = this as List<Any?>

    private fun Any?.num(): Double? = (this as? Number)?.toDouble()

    @Test
    fun fixtureReplays() {
        val file = File(sdk, "fixtures/data-viz.json")
        assumeTrue("$file not in this checkout", file.isFile)
        val root = MiniJson(file.readText()).parse().obj()
        val w = root["waveform"].obj()
        assertEquals(listOf(DataViz.BAR_WIDTH, DataViz.GAP, DataViz.MIN_HEIGHT, DataViz.HEIGHT), listOf(w["barWidth"].num(), w["gap"].num(), w["minHeight"].num(), w["height"].num()))
        for (c in root["rings"].arr()) {
            val i = c.obj()["input"].obj()
            val rings = (i["rings"] as? List<*>)?.map { r -> val o = r.obj(); DataViz.RingSpec(o["value"].num(), o["max"].num(), o["tone"] as? String) }
            val got = DataViz.ringArcs(i["size"].num()!!, i["stroke"].num()!!, i["value"].num(), i["tone"] as? String, i["sweep"].num(), rings)
            val e = c.obj()["expected"].obj()
            val expected = DataViz.RingArcs(e["start"].num()!!, e["sweep"].num()!!, e["rings"].arr().map { r -> val o = r.obj(); DataViz.RingArc(o["r"].num()!!, o["fraction"].num()!!, o["tone"] as String) })
            assertEquals(c.obj()["name"] as String, expected, got)
        }
        for (c in root["bars"].arr()) {
            val i = c.obj()["input"].obj()
            val got = DataViz.waveformBars(i["width"].num()!!, i["peaks"].arr().map { it.num() }, i["progress"].num())
            val expected = c.obj()["expected"].arr().map { b -> val o = b.obj(); DataViz.WaveBar(o["x"].num()!!, o["y"].num()!!, o["width"].num()!!, o["height"].num()!!, o["played"] as Boolean) }
            assertEquals(c.obj()["name"] as String, expected, got)
        }
        for (c in root["seek"].arr()) {
            val o = c.obj()
            assertEquals(o["expected"].num()!!, DataViz.seekFraction(o["x"].num()!!, o["width"].num()!!), 0.0)
        }
    }
}
