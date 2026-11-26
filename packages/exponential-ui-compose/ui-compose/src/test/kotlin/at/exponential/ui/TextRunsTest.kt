package at.exponential.ui

import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.TextRuns
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** Round 4 group B: `fixtures/text-runs.json` (TS, the Rust core and Swift replay the same cases). */
class TextRunsTest {
    private val fixture by lazy { Fixtures.json("text-runs.json") }

    private fun runs(v: List<at.exponential.ui.measure.TextRun>) = JsonValue.Arr(v.map { it.toJson() })

    @Test
    fun numbers() {
        Fixtures.require()
        val d = fixture["dropCap"]!!
        assertEquals(TextRuns.MIN, d["min"]!!.number!!.toInt())
        assertEquals(TextRuns.MAX, d["max"]!!.number!!.toInt())
        assertEquals(TextRuns.LINES, d["lines"]!!.number!!.toInt())
        assertEquals(TextRuns.RATIO.toDouble(), d["ratio"]!!.number!!, 1e-9)
        assertEquals("\$spacing.${TextRuns.GAP_TOKEN}", d["gap"]!!.string)
    }

    @Test
    fun normalCases() {
        Fixtures.require()
        val cases = fixture["normalCases"]!!.array!!
        assertFalse(cases.isEmpty())
        for (c in cases) {
            val name = c["name"]!!.string
            val out = TextRuns.normalise(c["runs"])
            assertEquals(name, c["out"]!!.json, out?.let { runs(it).json } ?: JsonValue.Null.json)
            assertEquals(name, c["text"]!!.string, out?.let { TextRuns.joined(it) })
        }
    }

    @Test
    fun linesCases() {
        Fixtures.require()
        for (c in fixture["linesCases"]!!.array!!) assertEquals(c["name"]!!.string, c["lines"]!!.number!!.toInt(), TextRuns.dropCapLines(c["props"]!!.obj!!))
    }

    @Test
    fun splitCases() {
        Fixtures.require()
        for (c in fixture["splitCases"]!!.array!!) {
            val split = TextRuns.dropCapSplit(TextRuns.normalise(c["runs"])!!)
            val got = split?.let { (cap, rest) -> JsonValue.Obj(mapOf("cap" to cap.toJson(), "rest" to runs(rest))).json } ?: JsonValue.Null.json
            assertEquals(c["name"]!!.string, c["split"]!!.json, got)
        }
    }

    @Test
    fun sizeCases() {
        Fixtures.require()
        for (c in fixture["sizeCases"]!!.array!!) {
            assertEquals(c["size"]!!.number!!.toFloat(), TextRuns.dropCapSize(c["fontSize"]!!.number!!.toFloat(), c["lineHeight"]!!.number!!.toFloat(), c["lines"]!!.number!!.toInt()), 0f)
        }
    }
}
