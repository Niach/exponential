package at.exponential.ui

import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.host.ParsedSource
import at.exponential.ui.host.RecordRow
import at.exponential.ui.host.RecordsOverlay
import at.exponential.ui.host.RecordsQuery
import at.exponential.ui.host.RecordsStore
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * VAPP-189: `fixtures/records-query.json` through [RecordsQuery] (the core's
 * one evaluator over the FFI; VAPP-190: jsonCases, bucketCases, JSON valueCases). The TS reference, the Rust core and Swift's
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
            val json = c["query"]
            val query = if (json != null) RecordsQuery.parseJson(json.json, c["kind"]?.string).query else RecordsQuery.parse(strings(c["params"]), c["kind"]?.string)
            val value = query.value(rows(c["rows"]), c["me"]?.string, c["now"]?.string ?: "", strings(c["names"]), strings(c["computed"]), c["today"]?.string, c["refs"] ?: JsonValue.Null, c["issues"]?.obj.orEmpty(), c["transitions"] ?: JsonValue.Null, c["role"]?.string)
            same(value, c["expected"], c["name"]?.string ?: "")
        }
    }

    /** VAPP-212: the same cases through the handle: seeded, then with a row that came and went. */
    @Test
    fun valueCasesThroughTheStore() {
        for (c in cases("valueCases")) {
            val name = c["name"]?.string ?: ""
            val json = c["query"]
            val query = if (json != null) RecordsQuery.parseJson(json.json, c["kind"]?.string).query else RecordsQuery.parse(strings(c["params"]), c["kind"]?.string)
            val given = rows(c["rows"])
            val store = RecordsStore<RecordRow>()
            val ghost = RecordRow(id = "ghost", kind = "ghost", data = JsonValue.Obj(emptyMap()), createdBy = null, createdAt = "2020-01-01T00:00:00.000Z", updatedAt = "2020-01-01T00:00:00.000Z")
            store.sync(given + ghost, { it.id }, { it })
            assertEquals(name, given.size + 1, store.count)
            store.sync(given, { it.id }, { it })
            assertEquals(name, given.size, store.count)
            val value = query.value(store, c["me"]?.string, c["now"]?.string ?: "", strings(c["names"]), strings(c["computed"]), c["today"]?.string, c["refs"] ?: JsonValue.Null, c["issues"]?.obj.orEmpty(), c["transitions"] ?: JsonValue.Null, c["role"]?.string)
            same(value, c["expected"], name)
        }
    }

    /** VAPP-212: only what changed crosses, a removed row leaves, an overlay touches one query. */
    @Test
    fun aStorePushesDeltas() {
        fun row(id: String, title: String) = RecordRow(id = id, kind = "note", data = JsonValue.Obj(mapOf("title" to JsonValue.Str(title))), createdBy = null, createdAt = "2026-01-01T00:00:00.000Z", updatedAt = "2026-01-01T00:00:00.000Z")
        val query = RecordsQuery.parse(mapOf("sort" to "id", "order" to "asc"), "note")
        val titles = { store: RecordsStore<RecordRow>, overlay: RecordsOverlay? ->
            query.value(store, null, "2026-11-05T00:00:00.000Z", overlay = overlay)["rows"]?.array.orEmpty().map { "${it["id"]?.string}:${it["title"]?.string}" }
        }
        val store = RecordsStore<RecordRow>()
        var mapped = 0
        val sync = { rows: List<RecordRow> -> store.sync(rows, { it.id }, { mapped += 1; it }) }
        sync(listOf(row("a", "1"), row("b", "1")))
        assertEquals(2, mapped)
        sync(listOf(row("a", "1"), row("b", "2")))
        assertEquals(3, mapped)
        assertEquals(listOf("a:1", "b:2"), titles(store, null))
        sync(listOf(row("b", "2")))
        assertEquals(3, mapped)
        assertEquals(listOf("b:2"), titles(store, null))
        assertEquals(listOf("b:3", "c:1"), titles(store, RecordsOverlay(listOf(row("b", "3"), row("c", "1")))))
        assertEquals(emptyList<String>(), titles(store, RecordsOverlay(deletes = listOf("b"))))
        assertEquals(listOf("b:2"), titles(store, null))
    }

    @Test
    fun jsonCases() {
        val all = cases("jsonCases")
        assertFalse(all.isEmpty())
        for (c in all) {
            val parsed = RecordsQuery.parseJson(c["query"]?.json ?: "null", c["kind"]?.string)
            val got = JsonValue.Obj(mapOf("query" to parsed.query.structured, "issues" to JsonValue.Arr(parsed.issues)))
            same(got, c["expected"], c["name"]?.string ?: "")
        }
    }

    @Test
    fun bucketCases() {
        val all = cases("bucketCases")
        assertFalse(all.isEmpty())
        for (c in all) assertEquals("${c["date"]} ${c["bucket"]}", c["expected"]?.string, RecordsQuery.dateBucket(c["date"]?.string ?: "", c["bucket"]?.string ?: ""))
    }

    @Test
    fun theQueryReadsItsKindAndApp() {
        val query = RecordsQuery.parse(mapOf("app" to "working-hours"), "entry")
        assertEquals("entry", query.from)
        assertEquals("working-hours", query.app)
        assertEquals(query, RecordsQuery.parse("records:entry?app=working-hours"))
        // `kind` is no reserved param: only the source's name is the kind.
        assertNull(RecordsQuery.parse(mapOf("kind" to "entry")).from)
        assertNull(RecordsQuery.parse(emptyMap()).from)
        assertEquals(JsonValue.Null, RecordsQuery.parse(emptyMap()).value(emptyList(), null, "2026-10-24T09:00:00Z"))
    }
}
