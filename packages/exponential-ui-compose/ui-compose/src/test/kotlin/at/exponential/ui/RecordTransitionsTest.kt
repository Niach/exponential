package at.exponential.ui

import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.host.RecordTransitions
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * eApps R6 (VAPP-222): `fixtures/record-transitions.json` through
 * [RecordTransitions] (the core over the FFI). The TS reference, the Rust
 * core and Swift's `RecordTransitionsTests` replay the same cases.
 */
class RecordTransitionsTest {
    private fun same(got: JsonValue, want: JsonValue?, name: String) {
        val w = want ?: JsonValue.Null
        assertTrue("$name: differs at ${jsonDiff(got.json, w.json)}: got ${got.json.take(400)}", jsonEqual(got.json, w.json))
    }

    private fun nonNull(v: JsonValue?): JsonValue? = v?.takeIf { it != JsonValue.Null }

    @Test
    fun fixture() {
        Fixtures.require()
        val f = Fixtures.json("record-transitions.json")
        val kindOf = { c: JsonValue -> c.obj?.get("transitions")?.let { JsonValue.Obj(mapOf("transitions" to it)) } ?: (f["kind"] ?: JsonValue.Null) }
        val validate = f["validateCases"]?.array.orEmpty()
        assertFalse(validate.isEmpty())
        for (c in validate) {
            val name = c["kindName"]?.string ?: ""
            same(JsonValue.Arr(RecordTransitions.validate(c["kind"] ?: JsonValue.Null, "/records/$name", name)), c["expected"], c["name"]?.string ?: "")
        }
        for (c in f["refusalCases"]?.array.orEmpty()) {
            val got = RecordTransitions.refusal(kindOf(c), nonNull(c["before"]), c["after"] ?: JsonValue.Null, c["viewer"] ?: JsonValue.Null, nonNull(c["row"]))
            same(got ?: JsonValue.Null, c["expected"], c["name"]?.string ?: "")
        }
        for (c in f["fromCases"]?.array.orEmpty()) {
            val got = RecordTransitions.from(kindOf(c), nonNull(c["record"]), nonNull(c["viewer"]), nonNull(c["row"]))
            same(JsonValue.Arr(got.map { JsonValue.Str(it) }), c["expected"], c["name"]?.string ?: "")
        }
    }
}
