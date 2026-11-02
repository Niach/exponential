package at.exponential.ui

import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.host.ParsedSource
import at.exponential.ui.host.RecordRow
import at.exponential.ui.host.RecordsQuery
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * VAPP-189: `fixtures/records-query.json` through [RecordsQuery] (the core's
 * one evaluator over the FFI). The TS reference, the Rust core and Swift's
 * `RecordsQueryTests` replay the same cases.
 */
class RecordsQueryTest {
    private fun strings(v: JsonValue?): Map<String, String> =
        v?.obj.orEmpty().mapNotNull { (k, s) -> s.string?.let { k to it } }.toMap()

    private fun same(got: JsonValue, want: JsonValue?, name: String) {
        val w = want ?: JsonValue.Null
        assertTrue("$name: differs at ${jsonDiff(got.json, w.json)}: got ${got.json.take(400)}", jsonEqual(got.json, w.json))
    }

    private fun rows(v: JsonValue?): List<RecordRow> = v?.array.orEmpty().map { r ->
        RecordRow(
            id = r["id"]?.string ?: "",
            kind = r["kind"]?.string ?: "",
            data = r["data"] ?: JsonValue.Obj(emptyMap()),
            createdBy = r["createdBy"]?.string,
            authorId = r["authorId"]?.string,
            createdAt = r["createdAt"]?.string ?: "",
            updatedAt = r["updatedAt"]?.string ?: "",
        )
    }

    private fun cases(key: String): List<JsonValue> {
        Fixtures.require()
        return Fixtures.json("records-query.json")[key]?.array.orEmpty()
    }

    @Test
    fun parseCases() {
        val all = cases("parseCases")
        assertFalse(all.isEmpty())
        for (c in all) same(RecordsQuery.parse(strings(c["params"]), c["kind"]?.string).structured, c["expected"], c["name"]?.string ?: "")
    }

    @Test
    fun sourceCases() {
        for (c in cases("sourceCases")) {
            val source = c["source"]?.string ?: ""
            val name = c["name"]?.string ?: ""
            same(RecordsQuery.parse(source)?.structured ?: JsonValue.Null, c["expected"], name)
            val parsed = ParsedSource.parse(source)?.let { RecordsQuery.parse(it) }
            same(parsed?.structured ?: JsonValue.Null, c["expected"], "parsed $name")
        }
    }

    @Test
    fun valueCases() {
        val all = cases("valueCases")
        assertFalse(all.isEmpty())
        for (c in all) {
            val query = RecordsQuery.parse(strings(c["params"]), c["kind"]?.string)
            val value = query.value(rows(c["rows"]), c["me"]?.string, c["now"]?.string ?: "", strings(c["names"]))
            same(value, c["expected"], c["name"]?.string ?: "")
        }
    }

    @Test
    fun theQueryReadsItsKindAndApp() {
        val query = RecordsQuery.parse(mapOf("kind" to "entry", "app" to "working-hours"))
        assertEquals("entry", query.from)
        assertEquals("working-hours", query.app)
        assertNull(RecordsQuery.parse(emptyMap()).from)
        assertEquals(JsonValue.Null, RecordsQuery.parse(emptyMap()).value(emptyList(), null, "2026-10-24T09:00:00Z"))
    }
}
