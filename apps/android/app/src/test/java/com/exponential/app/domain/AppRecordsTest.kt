package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// VAPP-122: replays `packages/domain-contract/fixtures/app-records.json`
// (web, desktop and iOS replay the same cases) and locks the constants.
class AppRecordsTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/domain-contract/fixtures/app-records.json")

    private fun cases(key: String): List<JsonValue> = fixture[key]!!.array!!

    private fun params(v: JsonValue?): Map<String, String> = v?.obj?.mapValues { it.value.string!! } ?: emptyMap()

    private fun strings(v: JsonValue?): List<String> = v!!.array!!.map { it.string!! }

    private fun queryJson(q: AppRecords.RecordsQuery): JsonValue = JsonValue.of(
        mapOf(
            "kind" to q.kind, "app" to q.app, "mine" to q.mine, "user" to q.user, "sort" to q.sort,
            "order" to q.order, "limit" to q.limit,
            "filters" to q.filters.map { mapOf("field" to it.field, "op" to it.op, "value" to it.value) },
            "sum" to q.sum, "by" to q.by, "month" to q.month,
        ),
    )

    @Test
    fun `the constants match the fixture`() {
        val query = fixture["query"]!!
        assertEquals(strings(query["reserved"]), AppRecords.RESERVED)
        assertEquals(query["sortDefault"]!!.string, AppRecords.SORT_DEFAULT)
        assertEquals(query["orderDefault"]!!.string, AppRecords.ORDER_DEFAULT)
        assertEquals(query["limitDefault"]!!.number!!.toInt(), AppRecords.LIMIT_DEFAULT)
        assertEquals(query["limitMax"]!!.number!!.toInt(), AppRecords.LIMIT_MAX)
        assertEquals(strings(query["operators"]), AppRecords.OPERATORS)
        assertEquals(strings(fixture["rowKeys"]), AppRecords.ROW_KEYS)
        assertEquals(strings(fixture["weekdays"]), AppRecords.WEEKDAYS)
        assertEquals(strings(fixture["months"]), AppRecords.MONTHS)
    }

    @Test
    fun `parseRecordsQuery replays queryCases`() {
        val all = cases("queryCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val got = queryJson(AppRecords.parseRecordsQuery(params(case["params"])))
            assertEquals(case["name"]!!.string, case["expected"]!!.json, got.json)
        }
    }

    @Test
    fun `recordsValue replays valueCases`() {
        val all = cases("valueCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val rows = case["rows"]!!.array!!.map { r ->
                AppRecords.Row(
                    id = r["id"]!!.string!!,
                    kind = r["kind"]!!.string!!,
                    data = r["data"]!!.obj!!,
                    createdBy = r["createdBy"]?.string,
                    createdAt = r["createdAt"]!!.string!!,
                    updatedAt = r["updatedAt"]!!.string!!,
                )
            }
            val got = AppRecords.recordsValue(
                rows,
                AppRecords.parseRecordsQuery(params(case["params"])),
                me = case["me"]?.string,
                now = case["now"]!!.string!!,
                names = params(case["names"]),
            )
            val expected = case["expected"]!!
            if (expected.isNull) assertNull(case["name"]!!.string, got) else assertEquals(case["name"]!!.string, expected.json, got!!.json)
        }
    }

    @Test
    fun `recordsCountLabel replays countLabelCases`() {
        for (case in cases("countLabelCases")) {
            val max = case["max"]?.takeIf { !it.isNull }?.number?.toLong()
            assertEquals(case["label"]!!.string, AppRecords.recordsCountLabel(case["count"]!!.number!!.toInt(), max))
        }
    }

    @Test
    fun `the month helpers`() {
        assertEquals("2025-12", AppRecords.shiftMonth("2026-01", -1))
        assertEquals("2027-01", AppRecords.shiftMonth("2026-12", 1))
        assertEquals("October 2026", AppRecords.monthLabel("2026-10"))
        assertEquals(29, AppRecords.monthDays("2028-02").size)
        assertEquals("Thu", AppRecords.monthDays("2026-10").first().weekday)
    }
}
