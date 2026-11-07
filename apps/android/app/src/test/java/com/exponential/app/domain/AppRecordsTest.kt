package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

// VAPP-122: replays `packages/domain-contract/fixtures/app-records.json`
// (web, desktop and iOS replay the same cases) and locks the constants.
// VAPP-189: the query and its value are eUI's (the Compose SDK's
// `RecordsQueryTest` replays packages/exponential-ui fixtures/records-query.json).
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

    @Test
    fun `the constants match the fixture`() {
        assertEquals(strings(fixture["rowKeys"]), AppRecords.ROW_KEYS)
    }

    // eApps R3: packages/exponential-ui fixtures/record-policy.json (eUI's
    // rule, hand-ported in AppRecords.Policy).
    @Test
    fun `record policy replays record-policy json`() {
        val f = read("packages/exponential-ui/fixtures/record-policy.json")
        assertEquals(strings(f["ops"]), AppRecords.Policy.OPS)
        assertEquals(strings(f["levels"]), AppRecords.Policy.LEVELS)
        assertEquals(strings(f["access"]), AppRecords.Policy.ACCESS)
        assertEquals(params(f["defaults"]), AppRecords.Policy.DEFAULTS)
        val copy = f["copy"]!!
        assertEquals(copy["none"]!!.string, AppRecords.Policy.COPY_NONE)
        assertEquals(copy["readOnly"]!!.string, AppRecords.Policy.COPY_READ_ONLY)
        assertEquals(params(copy["author"]), AppRecords.Policy.COPY_AUTHOR)
        assertEquals(params(copy["owners"]), AppRecords.Policy.COPY_OWNERS)
        for (c in f["validateCases"]!!.array!!) {
            val kind = JsonValue.Obj(mapOf("policy" to c["policy"]!!))
            assertEquals(c["name"]!!.string, params(c["effective"]), AppRecords.Policy.effective(kind))
        }
        for (c in f["allowCases"]!!.array!!) {
            val op = c["op"]!!.string!!
            val policy = AppRecords.Policy.effective(c["policy"]?.let { JsonValue.Obj(mapOf("policy" to it)) })
            val row = if (op == "create") null else AppRecords.Row("r", "k", emptyMap(), c["createdBy"]?.string, "", "", c["author"]?.string)
            val got = AppRecords.Policy.refusal(policy, op, c["viewer"]?.string, c["role"]?.string, c["access"]?.string, row)
            assertEquals(c["name"]!!.string, c["refusal"]?.string, got)
        }
    }

    // VAPP-187: record ownership (`ownership`).
    @Test
    fun `the ownership constants match the fixture`() {
        val ownership = fixture["ownership"]!!
        assertEquals(params(ownership["policy"]), AppRecords.OWNERSHIP_POLICY)
        assertEquals(ownership["refusal"]!!.string, AppRecords.OWNERSHIP_REFUSAL)
    }

    @Test
    fun `canChangeRecord replays canChangeCases`() {
        val all = fixture["ownership"]!!["canChangeCases"]!!.array!!
        assert(all.isNotEmpty())
        for (case in all) {
            val row = AppRecords.Row(
                id = "r", kind = "entry", data = emptyMap(),
                createdBy = case["createdBy"]?.string, createdAt = "", updatedAt = "",
                authorId = case["author"]?.string,
            )
            val viewer = case["viewer"]?.string
            val role = case["role"]?.string
            val expected = case["expected"]!!
            for (op in listOf("update", "delete")) {
                assertEquals("${case["name"]!!.string} ($op)", expected[op]!!.bool!!, AppRecords.canChangeRecord(row, viewer, role, op))
            }
        }
    }

    // VAPP-188: on-behalf writes (`onBehalf`).
    @Test
    fun `the on-behalf constants match the fixture`() {
        val onBehalf = fixture["onBehalf"]!!
        assertEquals(onBehalf["refusal"]!!.string, AppRecords.ON_BEHALF_REFUSAL)
        assertEquals(onBehalf["notMember"]!!.string, AppRecords.ON_BEHALF_NOT_MEMBER)
    }

    @Test
    fun `writeAsVerdict replays writeAsCases`() {
        val all = fixture["onBehalf"]!!["writeAsCases"]!!.array!!
        assert(all.isNotEmpty())
        for (case in all) {
            assertEquals(
                case["name"]!!.string,
                case["expected"]!!.string,
                AppRecords.writeAsVerdict(case["viewer"]?.string, case["role"]?.string, case["author"]?.string, strings(case["members"])),
            )
        }
    }

    @Test
    fun `importAuthor replays importCases`() {
        val all = fixture["onBehalf"]!!["importCases"]!!.array!!
        assert(all.isNotEmpty())
        for (case in all) {
            assertEquals(
                case["name"]!!.string,
                case["expected"]?.string,
                AppRecords.importAuthor(case["importer"]!!.string!!, case["role"]?.string, case["author"]?.string, strings(case["members"])),
            )
        }
    }

    @Test
    fun `recordsCountLabel replays countLabelCases`() {
        for (case in cases("countLabelCases")) {
            val max = case["max"]?.takeIf { !it.isNull }?.number?.toLong()
            assertEquals(case["label"]!!.string, AppRecords.recordsCountLabel(case["count"]!!.number!!.toInt(), max))
        }
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
            "author" to AppRecords.DataCopy.AUTHOR, "byFor" to AppRecords.DataCopy.BY_FOR,
            "file" to AppRecords.DataCopy.FILE, "files" to AppRecords.DataCopy.FILES,
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
            "history" to AppRecords.DataTestIds.HISTORY, "historyRow" to AppRecords.DataTestIds.HISTORY_ROW,
            "restore" to AppRecords.DataTestIds.RESTORE,
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
    fun `recordByline replays bylineCases`() {
        val all = dataCases("bylineCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val row = AppRecords.Row(
                id = "r", kind = "entry", data = emptyMap(),
                createdBy = case["createdBy"]?.string, createdAt = "", updatedAt = "",
                authorId = case["authorId"]?.string,
            )
            assertEquals(
                case["name"]!!.string,
                case["text"]!!.string,
                AppRecords.recordByline(row, params(case["names"]), case["fallback"]!!.string!!),
            )
        }
    }

    @Test
    fun `authorOptions replays authorOptionCases`() {
        val all = dataCases("authorOptionCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val users = case["users"]!!.array!!.map { AppRecords.Member(it["id"]!!.string!!, it["name"]!!.string!!) }
            assertEquals(case.json, strings(case["order"]), AppRecords.authorOptions(users).map { it.id })
        }
    }

    @Test
    fun `formAuthor replays formAuthorCases`() {
        val all = dataCases("formAuthorCases")
        assert(all.isNotEmpty())
        for (case in all) {
            val record = case["record"]?.takeIf { !it.isNull }?.let {
                AppRecords.Row(
                    id = "r", kind = "entry", data = emptyMap(),
                    createdBy = it["createdBy"]?.string, createdAt = "", updatedAt = "",
                    authorId = it["authorId"]?.string,
                )
            }
            val got = AppRecords.formAuthor(case["viewer"]?.string, record, case["picked"]?.string)
            val name = case["name"]!!.string
            assertEquals("$name (default)", case["default"]?.string, got.default)
            assertEquals("$name (send)", case["send"]?.string, got.send)
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

    // VAPP-215: record history + undo (`history`).
    private val history = fixture["history"]!!

    private fun historyCases(key: String): List<JsonValue> = history[key]!!.array!!.also { assert(it.isNotEmpty()) }

    private fun dataOf(v: JsonValue?): Map<String, JsonValue>? = v?.obj

    private fun event(v: JsonValue): AppRecords.RecordEvent = AppRecords.RecordEvent(
        id = v["id"]?.string ?: "",
        op = v["op"]!!.string!!,
        before = dataOf(v["before"]),
        patch = dataOf(v["patch"]),
        actor = v["actor"]?.string,
        createdAt = v["createdAt"]?.string ?: "",
        via = v["via"]?.string,
    )

    /** Numbers by value (2 == 2.0). */
    private fun assertData(name: String, expected: Map<String, JsonValue>?, actual: Map<String, JsonValue>?) {
        if (expected == null || actual == null) return assertEquals(name, expected, actual)
        assert(AppRecords.jsonEqual(JsonValue.Obj(expected), JsonValue.Obj(actual))) { "$name: $expected != $actual" }
    }

    @Test
    fun `the history constants match the fixture`() {
        assertEquals(history["undoSeconds"]!!.number!!.toInt(), AppRecords.History.UNDO_SECONDS)
        assertEquals(history["retentionDays"]!!.number!!.toInt(), AppRecords.History.RETENTION_DAYS)
        assertEquals(history["batch"]!!.number!!.toInt(), AppRecords.History.BATCH)
        assertEquals(history["historyLimit"]!!.number!!.toInt(), AppRecords.History.HISTORY_LIMIT)
        assertEquals(history["maxFields"]!!.number!!.toInt(), AppRecords.History.MAX_FIELDS)
        assertEquals(strings(history["ops"]), AppRecords.History.OPS)
        assertEquals(strings(history["vias"]), AppRecords.History.VIAS)
        val c = AppRecords.HistoryCopy
        val expected = mapOf(
            "history" to c.HISTORY, "restore" to c.RESTORE, "undo" to c.UNDO, "created" to c.CREATED,
            "deleted" to c.DELETED, "restored" to c.RESTORED, "changed" to c.CHANGED, "more" to c.MORE,
            "saved" to c.SAVED, "empty" to c.EMPTY, "undoCreate" to c.UNDO_CREATE, "undoUpdate" to c.UNDO_UPDATE,
            "undoDelete" to c.UNDO_DELETE, "undoBatch" to c.UNDO_BATCH, "undoVerb" to c.UNDO_VERB, "undoThing" to c.UNDO_THING,
        )
        assertEquals(params(history["copy"]), expected)
        assertEquals(c.UNDO_CREATE, AppRecords.undoTitle("create"))
        assertEquals(c.UNDO_UPDATE, AppRecords.undoTitle("update"))
        assertEquals(c.UNDO_DELETE, AppRecords.undoTitle("delete"))
        assertEquals(c.UNDO_BATCH, AppRecords.undoTitle("batch"))
    }

    @Test
    fun `eventState replays stateCases`() {
        for (c in historyCases("stateCases")) {
            assertData(c["name"]!!.string!!, dataOf(c["state"]), AppRecords.eventState(event(c["event"]!!)))
        }
    }

    @Test
    fun `inverseWrite replays inverseCases`() {
        for (c in historyCases("inverseCases")) {
            val name = c["name"]!!.string!!
            val got = AppRecords.inverseWrite(event(c["event"]!!))
            assertEquals(name, c["inverse"]!!["op"]!!.string, got.op)
            assertData(name, dataOf(c["inverse"]!!["data"]), got.data)
        }
    }

    @Test
    fun `eventLabel replays labelCases`() {
        for (c in historyCases("labelCases")) {
            assertEquals(c["name"]!!.string, c["label"]!!.string, AppRecords.eventLabel(event(c["event"]!!)))
        }
    }

    @Test
    fun `historyRows replays rowsCases`() {
        for (c in history["rowsCases"]!!.array!!) {
            val rows = AppRecords.historyRows(c["events"]!!.array!!.map(::event), params(c["names"]), c["fallback"]!!.string!!)
            val expected = c["rows"]!!.array!!.map {
                AppRecords.HistoryRow(it["id"]!!.string!!, it["label"]!!.string!!, it["who"]!!.string!!, it["at"]!!.string!!, it["restorable"]!!.bool!!)
            }
            assertEquals(c["name"]!!.string, expected, rows)
        }
    }

    // VAPP-235 (O1): the offline outbox (`outbox`).
    private val outbox = fixture["outbox"]!!

    private fun outboxCases(key: String): List<JsonValue> = outbox[key]!!.array!!.also { assert(it.isNotEmpty()) }

    private fun entry(v: JsonValue): AppRecords.OutboxEntry = AppRecords.OutboxEntry(
        writeId = v["writeId"]!!.string!!,
        appId = v["appId"]!!.string!!,
        call = v["call"]!!.string!!,
        writes = v["writes"]!!.array!!.map(AppRecords::outboxWrite),
        at = v["at"]!!.string!!,
        state = v["state"]!!.string!!,
        ackedAt = v["ackedAt"]?.string,
    )

    private fun recordRow(v: JsonValue): AppRecords.Row = AppRecords.Row(
        id = v["id"]!!.string!!,
        kind = v["kind"]!!.string!!,
        data = v["data"]!!.obj!!,
        createdBy = v["createdBy"]?.string,
        createdAt = v["createdAt"]!!.string!!,
        updatedAt = v["updatedAt"]!!.string!!,
        authorId = v["authorId"]?.string,
    )

    private fun rowJson(r: AppRecords.Row): JsonValue = JsonValue.Obj(
        mapOf(
            "id" to JsonValue.Str(r.id), "kind" to JsonValue.Str(r.kind), "data" to JsonValue.Obj(r.data),
            "createdBy" to (r.createdBy?.let { JsonValue.Str(it) } ?: JsonValue.Null),
            "authorId" to (r.authorId?.let { JsonValue.Str(it) } ?: JsonValue.Null),
            "createdAt" to JsonValue.Str(r.createdAt), "updatedAt" to JsonValue.Str(r.updatedAt),
        ),
    )

    private fun entryJson(e: AppRecords.OutboxEntry): JsonValue = JsonValue.Obj(
        buildMap {
            put("writeId", JsonValue.Str(e.writeId))
            put("appId", JsonValue.Str(e.appId))
            put("call", JsonValue.Str(e.call))
            put("writes", JsonValue.Arr(e.writes.map(AppRecords::outboxWriteJson)))
            put("at", JsonValue.Str(e.at))
            put("state", JsonValue.Str(e.state))
            e.ackedAt?.let { put("ackedAt", JsonValue.Str(it)) }
        },
    )

    private fun assertJson(name: String, expected: JsonValue, actual: JsonValue) {
        assert(AppRecords.jsonEqual(expected, actual)) { "$name: $expected != $actual" }
    }

    @Test
    fun `the outbox constants match the fixture`() {
        assertEquals(params(outbox["copy"]), mapOf("pendingOne" to AppRecords.Outbox.PENDING_ONE, "pending" to AppRecords.Outbox.PENDING))
    }

    @Test
    fun `a stored writes column reads back`() {
        for (c in outboxCases("writesCases")) {
            val writes = c["writes"]!!.array!!.map(AppRecords::outboxWrite)
            assertEquals(c["name"]!!.string, writes, AppRecords.outboxWritesOf(AppRecords.outboxWritesJson(writes)))
        }
    }

    @Test
    fun `outboxRows replays rowsCases`() {
        for (c in outboxCases("rowsCases")) {
            val name = c["name"]!!.string!!
            val got = AppRecords.outboxRows(c["rows"]!!.array!!.map(::recordRow), c["entries"]!!.array!!.map(::entry), c["me"]?.string)
            assertJson(name, c["expected"]!!, JsonValue.Arr(got.map(::rowJson)))
        }
    }

    /** VAPP-212: the overlay laid over the rows answers the same cases. */
    @Test
    fun `outboxOverlay replays rowsCases`() {
        for (c in outboxCases("rowsCases")) {
            val name = c["name"]!!.string!!
            val rows = c["rows"]!!.array!!.map(::recordRow)
            val overlay = AppRecords.outboxOverlay(rows, c["entries"]!!.array!!.map(::entry), c["me"]?.string)
            val laid = rows.filter { it.id !in overlay.deletes }.toMutableList()
            for (row in overlay.upserts) {
                val i = laid.indexOfFirst { it.id == row.id }
                if (i >= 0) laid[i] = row else laid += row
            }
            assertJson(name, c["expected"]!!, JsonValue.Arr(laid.map(::rowJson)))
        }
    }

    @Test
    fun `outboxPendingLabel replays pendingCases`() {
        for (c in outboxCases("pendingCases")) {
            val got = AppRecords.outboxPendingLabel(c["entries"]!!.array!!.map(::entry), c["appId"]!!.string!!)
            assertEquals(c["name"]!!.string, c["label"]?.string, got)
        }
    }

    // VAPP-228: the spreadsheet import (`csvImport`).
    private val csv = fixture["csvImport"]!!

    private fun csvCases(key: String): List<JsonValue> = csv[key]!!.array!!.also { assert(it.isNotEmpty()) }

    private fun csvRows(v: JsonValue?): List<AppRecords.CsvRow> =
        v!!.array!!.map { AppRecords.CsvRow(it["row"]!!.number!!.toInt(), strings(it["cells"])) }

    private fun csvMapping(v: JsonValue?): Map<String, Int?> =
        v!!.obj!!.mapValues { (_, x) -> x.number?.toInt() }

    @Test
    fun `the csv import constants match the fixture`() {
        assertEquals(csv["maxRows"]!!.number!!.toInt(), AppRecords.CsvImport.MAX_ROWS)
        assertEquals(csv["previewRows"]!!.number!!.toInt(), AppRecords.CsvImport.PREVIEW_ROWS)
        assertEquals(csv["issuesShown"]!!.number!!.toInt(), AppRecords.CsvImport.ISSUES_SHOWN)
        assertEquals(strings(csv["trueWords"]), AppRecords.CsvImport.TRUE_WORDS)
        assertEquals(strings(csv["falseWords"]), AppRecords.CsvImport.FALSE_WORDS)
        assertEquals(strings(csv["authorKeys"]), AppRecords.CsvImport.AUTHOR_KEYS)
        val c = AppRecords.CsvCopy
        assertEquals(params(csv["copy"]), mapOf(
            "importButton" to c.IMPORT_BUTTON, "title" to c.TITLE, "placeholder" to c.PLACEHOLDER,
            "chooseFile" to c.CHOOSE_FILE, "skip" to c.SKIP, "me" to c.ME, "column" to c.COLUMN,
            "issue" to c.ISSUE, "rowIssue" to c.ROW_ISSUE, "moreIssues" to c.MORE_ISSUES, "ready" to c.READY,
            "tooMany" to c.TOO_MANY, "noRows" to c.NO_ROWS, "importAction" to c.IMPORT_ACTION, "imported" to c.IMPORTED,
        ))
        val i = AppRecords.CsvIssues
        assertEquals(params(csv["issues"]), mapOf(
            "notNumber" to i.NOT_NUMBER, "notInteger" to i.NOT_INTEGER, "notBoolean" to i.NOT_BOOLEAN,
            "notDate" to i.NOT_DATE, "slashDate" to i.SLASH_DATE, "notTime" to i.NOT_TIME,
            "notDateTime" to i.NOT_DATE_TIME, "notEnum" to i.NOT_ENUM, "notMember" to i.NOT_MEMBER,
            "extraCells" to i.EXTRA_CELLS,
        ))
        val t = AppRecords.CsvTestIds
        assertEquals(params(csv["testIds"]), mapOf(
            "button" to t.BUTTON, "dialog" to t.DIALOG, "text" to t.TEXT, "file" to t.FILE, "map" to t.MAP,
            "author" to t.AUTHOR, "preview" to t.PREVIEW, "error" to t.ERROR, "confirm" to t.CONFIRM,
        ))
    }

    @Test
    fun `csvParse replays parseCases`() {
        for (c in csvCases("parseCases")) {
            val expected = AppRecords.CsvTable(strings(c["header"]), csvRows(c["rows"]))
            assertEquals(c["name"]!!.string, expected, AppRecords.csvParse(c["text"]!!.string!!))
        }
    }

    @Test
    fun `csvAutoMap replays mapCases`() {
        for (c in csvCases("mapCases")) {
            assertEquals(c["name"]!!.string, csvMapping(c["mapping"]), AppRecords.csvAutoMap(strings(c["fields"]), strings(c["header"])))
        }
    }

    @Test
    fun `csvAuthorColumn replays authorColumnCases`() {
        for (c in csvCases("authorColumnCases")) {
            assertEquals(c["header"]!!.json, c["column"]?.number?.toInt(), AppRecords.csvAuthorColumn(strings(c["header"])))
        }
    }

    @Test
    fun `csvCell replays cellCases`() {
        for (c in csvCases("cellCases")) {
            val name = c["name"]!!.string!!
            val got = AppRecords.csvCell(c["field"], c["cell"]!!.string!!)
            val issue = c["issue"]?.string
            if (issue != null) {
                assertEquals(name, AppRecords.CsvCell.Issue(issue), got)
            } else {
                val value = (got as? AppRecords.CsvCell.Value)?.value
                assert(AppRecords.jsonEqual(c["value"] ?: JsonValue.Null, value)) { "$name: expected ${c["value"]}, got $got" }
            }
        }
    }

    @Test
    fun `csvRecords replays rowsCases`() {
        for (c in csvCases("rowsCases")) {
            val name = c["name"]!!.string!!
            val author = c["author"]?.let { a ->
                AppRecords.CsvAuthor(
                    column = a["column"]?.number?.toInt(),
                    importer = a["importer"]!!.string!!,
                    members = a["members"]!!.array!!.map { AppRecords.CsvMember(it["id"]!!.string!!, it["name"]!!.string!!, it["email"]!!.string!!) },
                )
            }
            val got = AppRecords.csvRecords(c["schema"], strings(c["header"]), csvRows(c["rows"]), csvMapping(c["mapping"]), author)
            val expected = c["records"]!!.array!!
            assertEquals(name, expected.size, got.size)
            expected.zip(got).forEach { (e, r) ->
                assertEquals(name, e["row"]!!.number!!.toInt(), r.row)
                assertEquals(name, strings(e["issues"]), r.issues)
                assertEquals(name, e["author"]?.string, r.author)
                assertData(name, e["data"]!!.obj, r.data)
            }
        }
    }

    @Test
    fun `csvSummary counts the rows without issues`() {
        assertEquals("2 of 3 rows ready", AppRecords.csvSummaryOf(listOf(emptyList(), listOf("x"), emptyList())))
        val records = listOf(AppRecords.CsvRecord(2, emptyMap(), emptyList()), AppRecords.CsvRecord(3, emptyMap(), listOf("x")))
        assertEquals("1 of 2 rows ready", AppRecords.csvSummary(records))
    }
}
