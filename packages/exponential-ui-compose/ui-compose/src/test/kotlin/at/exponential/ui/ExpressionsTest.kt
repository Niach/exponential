package at.exponential.ui

import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.host.Expressions
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import java.util.TimeZone

/**
 * VAPP-191: `fixtures/expressions.json` through [Expressions] (the core's one
 * evaluator over the FFI). The TS reference, the Rust core and Swift's
 * `ExpressionsTests` replay the same cases.
 */
class ExpressionsTest {
    private val f: JsonValue by lazy {
        Fixtures.require()
        Fixtures.json("expressions.json")
    }

    private fun clock(c: JsonValue): Pair<Double, Int> {
        val clock = c["clock"] ?: f["clock"] ?: JsonValue.Null
        return Instant.parse(clock["now"]?.string ?: "").toEpochMilli().toDouble() to (clock["offsetMinutes"]?.number ?: 0.0).toInt()
    }

    private fun same(got: JsonValue, want: JsonValue?, name: String) {
        val w = want ?: JsonValue.Null
        assertTrue("$name: differs at ${jsonDiff(got.json, w.json)}: got ${got.json.take(400)}", jsonEqual(got.json, w.json))
    }

    @Test
    fun evalAndCostCases() {
        var n = 0
        for (key in listOf("evalCases", "costCases")) for (c in f[key]?.array.orEmpty()) {
            n++
            val (now, offset) = clock(c)
            val got = when (val o = Expressions.eval(c["source"]?.string ?: "", c["vars"] ?: f["vars"] ?: JsonValue.Obj(emptyMap()), now, offset)) {
                is Expressions.Outcome.Value -> JsonValue.Obj(mapOf("value" to o.value))
                is Expressions.Outcome.Error -> JsonValue.Obj(mapOf("error" to JsonValue.Str(o.message)))
            }
            same(got, c["expected"], "$key ${c["name"]?.string}")
        }
        assertTrue(n > 200)
    }

    @Test
    fun checkCases() {
        for (c in f["checkCases"]?.array.orEmpty()) {
            val types = c["types"]?.obj.orEmpty().mapNotNull { (k, v) -> v.string?.let { k to it } }.toMap()
            val got = Expressions.check(c["source"]?.string ?: "", types, f["kinds"] ?: JsonValue.Obj(emptyMap()))
            same(JsonValue.Obj(mapOf("type" to JsonValue.Str(got.type), "issues" to JsonValue.Arr(got.issues.map { JsonValue.Str(it) }))), c["expected"], c["name"]?.string ?: "")
        }
    }

    @Test
    fun callCases() {
        for (c in f["callCases"]?.array.orEmpty()) {
            val (now, offset) = clock(c)
            val got = Expressions.call(c["call"]?.string ?: "", c["args"] ?: JsonValue.Obj(emptyMap()), now, offset)
            same(got?.let { JsonValue.Obj(mapOf("value" to it)) } ?: JsonValue.Obj(emptyMap()), c["expected"], c["name"]?.string ?: "")
        }
    }

    @Test
    fun theClockAndZone() {
        val now = Instant.parse("2026-11-02T23:30:00Z").toEpochMilli()
        assertEquals(Expressions.Outcome.Value(JsonValue.Str("2026-11-03")), Expressions.eval("today()", nowMs = now, timeZone = TimeZone.getTimeZone("Europe/Vienna")))
        assertEquals(Expressions.Outcome.Value(JsonValue.Str("2026-11-02")), Expressions.eval("today()", nowMs = now))
    }
}
