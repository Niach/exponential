package at.exponential.ui

import at.exponential.ui.measure.middleTruncateWidth
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** #EXP-1778: `fixtures/text-truncate.json` (TS, the Rust core and Swift replay the same cases). */
class TextTruncateTest {
    @Test
    fun fixture() {
        Fixtures.require()
        val f = Fixtures.json("text-truncate.json")
        val affix = f["affix"]?.string ?: "…"
        val cases = f["cases"]?.array.orEmpty()
        assertFalse(cases.isEmpty())
        for (c in cases) {
            val widths = c["widths"]!!
            val default = widths["default"]!!.number!!
            val shown = middleTruncateWidth(c["text"]!!.string!!, c["width"]!!.number!!, affix) { ch -> widths["chars"]?.get(ch)?.number ?: default }
            assertEquals(c["name"]?.string, c["shown"]?.string, shown)
        }
    }
}
