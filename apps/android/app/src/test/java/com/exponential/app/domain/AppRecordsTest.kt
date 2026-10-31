package com.exponential.app.domain

import at.exponential.ui.host.ParsedSource
import at.exponential.ui.json.JsonValue
import java.io.File
import java.net.URLDecoder
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

    /** The core's source rule for the fixture's plain URIs (`scheme:name?k=v&…`,
     *  keys sorted); the app's JVM tests do not load the facade dylib. */
    private fun parseSource(uri: String): ParsedSource {
        val colon = uri.indexOf(':')
        val rest = uri.substring(colon + 1)
        val q = rest.indexOf('?')
        val params = if (q < 0) emptyMap() else rest.substring(q + 1).split('&').filter { it.isNotEmpty() }.associate {
            val eq = it.indexOf('=')
            val key = if (eq < 0) it else it.substring(0, eq)
            URLDecoder.decode(key, "UTF-8") to (if (eq < 0) "" else URLDecoder.decode(it.substring(eq + 1), "UTF-8"))
        }.toSortedMap()
        return ParsedSource(uri, uri.substring(0, colon), if (q < 0) rest else rest.substring(0, q), params)
    }

    @Test
    fun `recordsSourceParams replays sourceCases`() {
        val all = cases("sourceCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val params = AppRecords.recordsSourceParams(parseSource(case["source"]!!.string!!))
            val expected = case["expected"]!!
            if (expected.isNull) {
                assertNull(case["name"]!!.string, params)
            } else {
                assertEquals(case["name"]!!.string, expected.json, queryJson(AppRecords.parseRecordsQuery(params!!)).json)
            }
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

    // VAPP-178: the Data tab (`dataTab`).
    private val dataTab = fixture["dataTab"]!!

    private fun dataCases(key: String): List<JsonValue> = dataTab[key]!!.array!!

    @Test
    fun `the data tab constants match the fixture`() {
        val copy = dataTab["copy"]!!
        val expected = mapOf(
            "kindOption" to AppRecords.DataCopy.KIND_OPTION, "addRecord" to AppRecords.DataCopy.ADD_RECORD,
            "edit" to AppRecords.DataCopy.EDIT, "delete" to AppRecords.DataCopy.DELETE,
            "deleteTitle" to AppRecords.DataCopy.DELETE_TITLE, "deleteBody" to AppRecords.DataCopy.DELETE_BODY,
            "by" to AppRecords.DataCopy.BY, "updated" to AppRecords.DataCopy.UPDATED,
            "empty" to AppRecords.DataCopy.EMPTY, "noKinds" to AppRecords.DataCopy.NO_KINDS,
            "emptyTitle" to AppRecords.DataCopy.EMPTY_TITLE, "newTitle" to AppRecords.DataCopy.NEW_TITLE,
            "editTitle" to AppRecords.DataCopy.EDIT_TITLE, "save" to AppRecords.DataCopy.SAVE,
            "cancel" to AppRecords.DataCopy.CANCEL, "yes" to AppRecords.DataCopy.YES, "no" to AppRecords.DataCopy.NO,
            "none" to AppRecords.DataCopy.NONE, "optional" to AppRecords.DataCopy.OPTIONAL,
            "listHint" to AppRecords.DataCopy.LIST_HINT,
        )
        assertEquals(copy.obj!!.keys, expected.keys)
        for ((key, value) in expected) assertEquals(key, copy[key]!!.string, value)
        val columns = dataTab["columns"]!!
        assertEquals(columns["byWidth"]!!.number!!.toInt(), AppRecords.DataColumns.BY_WIDTH)
        assertEquals(columns["updatedWidth"]!!.number!!.toInt(), AppRecords.DataColumns.UPDATED_WIDTH)
        assertEquals(columns["menuWidth"]!!.number!!.toInt(), AppRecords.DataColumns.MENU_WIDTH)
        assertEquals(columns["minWidth"]!!.number!!.toInt(), AppRecords.DataColumns.MIN_WIDTH)
        assertEquals(columns["typeWidths"]!!.obj!!.mapValues { it.value.number!!.toInt() }, AppRecords.DataColumns.TYPE_WIDTHS)
        assertEquals(params(dataTab["placeholders"]), AppRecords.DATA_PLACEHOLDERS)
        val ids = dataTab["testIds"]!!
        val expectedIds = mapOf(
            "kind" to AppRecords.DataTestIds.KIND, "add" to AppRecords.DataTestIds.ADD, "row" to AppRecords.DataTestIds.ROW,
            "menu" to AppRecords.DataTestIds.MENU, "form" to AppRecords.DataTestIds.FORM,
            "save" to AppRecords.DataTestIds.SAVE, "error" to AppRecords.DataTestIds.ERROR,
        )
        assertEquals(ids.obj!!.keys, expectedIds.keys)
        for ((key, value) in expectedIds) assertEquals(key, ids[key]!!.string, value)
    }

    @Test
    fun `dataKinds replays kindCases`() {
        val all = dataCases("kindCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val got = AppRecords.dataKinds(case["manifest"], case["rows"]!!.array!!.map { it["kind"]!!.string!! })
            val want = case["kinds"]!!.array!!.map { AppRecords.DataKind(it["kind"]!!.string!!, it["count"]!!.number!!.toInt()) }
            assertEquals(case.json, want, got)
        }
    }

    @Test
    fun `dataColumns replays columnCases`() {
        val all = dataCases("columnCases")
        assert(all.isNotEmpty())
        for (case in all) assertEquals(case.json, strings(case["columns"]), AppRecords.dataColumns(case["schema"]))
    }

    @Test
    fun `dataRows replays rowsCases`() {
        val all = dataCases("rowsCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val rows = case["rows"]!!.array!!.map {
                AppRecords.Row(it["id"]!!.string!!, it["kind"]!!.string!!, emptyMap(), null, "", it["updatedAt"]!!.string!!)
            }
            assertEquals(case.json, strings(case["order"]), AppRecords.dataRows(rows, case["kind"]!!.string!!).map { it.id })
        }
    }

    @Test
    fun `cellText replays cellCases`() {
        val all = dataCases("cellCases")
        assert(all.isNotEmpty())
        for (case in all) assertEquals(case.json, case["text"]!!.string, AppRecords.cellText(case["value"]))
    }

    private fun fieldJson(f: AppRecords.FormField): String {
        val m = linkedMapOf<String, Any?>("name" to f.name, "control" to f.control, "required" to f.required)
        f.placeholder?.let { m["placeholder"] = it }
        f.options?.let { m["options"] = it }
        f.itemType?.let { m["itemType"] = it }
        return JsonValue.of(m).json
    }

    @Test
    fun `formFields replays formCases`() {
        val all = dataCases("formCases")
        assert(all.isNotEmpty())
        for (case in all) {
            assertEquals(
                case.json,
                case["fields"]!!.array!!.map { it.json },
                AppRecords.formFields(case["schema"]).map(::fieldJson),
            )
        }
    }

    @Test
    fun `formInput replays inputCases`() {
        val all = dataCases("inputCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val want: Any = when (val input = case["input"]!!) {
                is JsonValue.Bool -> input.v
                else -> input.string!!
            }
            assertEquals(case.json, want, AppRecords.formInput(case["control"]!!.string!!, case["value"]))
        }
    }

    @Test
    fun `formData replays parseCases`() {
        val all = dataCases("parseCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val inputs = case["inputs"]!!.obj!!.mapValues { (_, v) -> if (v is JsonValue.Bool) v.v else v.string }
            val got = AppRecords.formData(case["schema"], inputs, case["editing"]!!.bool!!)
            assertEquals(case["\$why"]?.string, case["data"]!!.json, JsonValue.Obj(got).json)
        }
    }

    @Test
    fun `the phone row reads title and subtitle`() {
        val row = AppRecords.Row(
            "r", "entry",
            mapOf("date" to JsonValue.Str("2026-10-23"), "hours" to JsonValue.Num(6.0), "tags" to JsonValue.Arr(listOf(JsonValue.Str("a"), JsonValue.Str("b")))),
            null, "", "",
        )
        val columns = listOf("date", "hours", "note", "tags")
        assertEquals("2026-10-23", AppRecords.rowTitle(columns, row))
        assertEquals("6 · a, b", AppRecords.rowSubtitle(columns, row))
        assertEquals(AppRecords.DataCopy.EMPTY_TITLE, AppRecords.rowTitle(listOf("note"), row))
    }
}
