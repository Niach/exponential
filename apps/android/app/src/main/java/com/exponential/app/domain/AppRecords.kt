package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import com.exponential.app.data.db.AppRecordEntity
import java.text.NumberFormat
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale

// VAPP-122: app RECORDS, the Android mirror of the pure reference
// `packages/domain-contract/src/app-records.ts` (fixtures/app-records.json is
// the spec; `AppRecordsTest` replays its ownership, on-behalf, count-label,
// purge and Data tab cases). VAPP-189: the `exp:records?…` query and its
// value are eUI's ONE evaluator (the Compose SDK's `RecordsQuery` over the
// core; packages/exponential-ui fixtures/records-query.json).
object AppRecords {
    // Fixture `rowKeys` (locked by AppRecordsTest).
    val ROW_KEYS: List<String> = listOf("id", "createdAt", "updatedAt", "createdBy", "authorId", "author")

    /** One synced `app_records` row as the host reads it ([data] = the parsed object). */
    data class Row(
        val id: String,
        val kind: String,
        val data: Map<String, JsonValue>,
        val createdBy: String?,
        val createdAt: String,
        val updatedAt: String,
        /** VAPP-187: whose record it is (null on a legacy row = [createdBy]). */
        val authorId: String? = null,
    )

    /** VAPP-187: a record's AUTHOR (`authorId ?? createdBy`): whose record it
     *  is, what `mine`/`user=` and the per-member views read. */
    fun recordAuthor(row: Row): String? = row.authorId ?: row.createdBy

    // VAPP-187 OWNERSHIP (fixture `ownership`, locked by AppRecordsTest): the
    // default per-kind policy (`author` | `member`) and the server's refusal.
    val OWNERSHIP_POLICY: Map<String, String> = mapOf("update" to "author", "delete" to "author")
    const val OWNERSHIP_REFUSAL = "Only the record's author or a team owner can change it"

    /** May [viewer] (with team [role]; null = not a member) [op] (`update` |
     *  `delete`) the record? Policy `author` = the author or a team owner
     *  (`canChangeCases`). */
    fun canChangeRecord(row: Row, viewer: String?, role: String?, op: String = "update"): Boolean {
        if (viewer == null || role == null) return false
        if (role == "owner") return true
        if (OWNERSHIP_POLICY[op] == "member") return true
        val author = recordAuthor(row)
        return author != null && author == viewer
    }

    // VAPP-188 ON-BEHALF (fixture `onBehalf`, locked by AppRecordsTest): who
    // may name another member as a write's author, and the server's refusals.
    const val ON_BEHALF_REFUSAL = "Only a team owner can write for another member"
    const val ON_BEHALF_NOT_MEMBER = "The author must be a member of the team"

    /** [writeAsVerdict]'s answer: `ok` | `forbidden` | `notMember`. */
    object WriteAs {
        const val OK = "ok"
        const val FORBIDDEN = "forbidden"
        const val NOT_MEMBER = "notMember"
    }

    /** May [viewer] (team [role]; null = not a member) write a record as
     *  [author] (null = as themself)? Another member = owners only, and the
     *  author must be one of [members] (`writeAsCases`). */
    fun writeAsVerdict(viewer: String?, role: String?, author: String?, members: List<String>): String {
        if (viewer == null || role == null) return WriteAs.FORBIDDEN
        if (author == null || author == viewer) return WriteAs.OK
        if (role != "owner") return WriteAs.FORBIDDEN
        return if (author in members) WriteAs.OK else WriteAs.NOT_MEMBER
    }

    /** An imported record's author (`importCases`): absent, the importer or a
     *  stranger to the team = the importer; another member = them when the
     *  importer is an owner, else null (the import is refused). */
    fun importAuthor(importer: String, role: String?, author: String?, members: List<String>): String? {
        if (author == null || author == importer || author !in members) return importer
        return if (role == "owner") author else null
    }

    /** JavaScript's `Number(text)` for a decimal literal (whitespace trimmed,
     *  "" = 0); null = NaN. Hex / binary / octal literals are not accepted. */
    private val DECIMAL = Regex("^[+-]?(\\d+\\.?\\d*|\\.\\d+)([eE][+-]?\\d+)?$")
    private fun jsNumber(raw: String?): Double? {
        if (raw == null) return null
        val t = raw.trim()
        if (t.isEmpty()) return 0.0
        return when (t) {
            "Infinity", "+Infinity" -> Double.POSITIVE_INFINITY
            "-Infinity" -> Double.NEGATIVE_INFINITY
            else -> if (DECIMAL.matches(t)) t.toDoubleOrNull() else null
        }
    }

    private val ISO_MILLIS: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'").withZone(ZoneOffset.UTC)

    /** A wire timestamp (Postgres text or ISO) as JavaScript's
     *  `toISOString()` (`2026-10-05T18:00:00.000Z`), the web host's form;
     *  unparseable text stays as is. */
    fun isoTimestamp(raw: String): String =
        WireTimestamps.parseEpochMs(raw)?.let { ISO_MILLIS.format(Instant.ofEpochMilli(it)) } ?: raw

    /** `now` the way the web host passes it. */
    fun isoNow(nowMs: Long = System.currentTimeMillis()): String = ISO_MILLIS.format(Instant.ofEpochMilli(nowMs))

    /** A synced row → the reference's row (`data` = the parsed object, {} otherwise). */
    fun row(entity: AppRecordEntity): Row = Row(
        id = entity.id,
        kind = entity.kind,
        data = entity.data?.let { JsonValue.parse(it).obj } ?: emptyMap(),
        createdBy = entity.createdBy?.takeIf { it.isNotEmpty() },
        createdAt = isoTimestamp(entity.createdAt),
        updatedAt = isoTimestamp(entity.updatedAt),
        authorId = entity.authorId?.takeIf { it.isNotEmpty() },
    )

    // VAPP-178 THE DATA TAB (fixture `dataTab`): the editor's records browser
    // and form, pure so the four clients replay the same cases
    // (`AppRecordsTest` replays every `dataTab.*Cases`).
    object DataCopy {
        const val KIND_OPTION = "{kind} · {n}"
        const val ADD_RECORD = "Add record"
        const val EDIT = "Edit"
        const val DELETE = "Delete"
        const val DELETE_TITLE = "Delete this record?"
        const val DELETE_BODY = "The app stops showing it. This cannot be undone."
        const val BY = "by"
        const val UPDATED = "updated"
        const val EMPTY = "No records yet"
        const val NO_KINDS = "This app keeps no records"
        const val EMPTY_TITLE = "—"
        const val NEW_TITLE = "New {kind}"
        const val EDIT_TITLE = "Edit {kind}"
        const val SAVE = "Save"
        const val CANCEL = "Cancel"
        const val YES = "Yes"
        const val NO = "No"
        const val NONE = "None"
        const val OPTIONAL = "optional"
        const val LIST_HINT = "Comma-separated"
        const val AUTHOR = "Author"
        const val BY_FOR = "{writer} for {author}"
    }

    /** Fixture `dataTab.columns` (the WIDE table's widths, px; phones do not use them). */
    object DataColumns {
        const val BY_WIDTH = 96
        const val UPDATED_WIDTH = 88
        const val MENU_WIDTH = 40
        const val MIN_WIDTH = 64
        const val GAP = 8
        val TYPE_WIDTHS: Map<String, Int> = mapOf(
            "date" to 104, "time" to 64, "date-time" to 152, "number" to 64, "integer" to 64, "boolean" to 56,
        )
    }

    /** Fixture `dataTab.placeholders`: a text field's hint by string format. */
    val DATA_PLACEHOLDERS: Map<String, String> = mapOf("date" to "YYYY-MM-DD", "time" to "HH:MM", "date-time" to "YYYY-MM-DDTHH:MM")

    object DataTestIds {
        const val KIND = "app-data-kind"
        const val ADD = "app-data-add"
        const val ROW = "app-data-row"
        const val MENU = "app-data-row-menu"
        const val FORM = "app-data-form"
        const val SAVE = "app-data-save"
        const val ERROR = "app-data-error"
    }

    data class DataKind(val kind: String, val count: Int)

    /** The manifest's `records` block (null = none). */
    private fun recordsBlock(manifest: JsonValue?): Map<String, JsonValue> = manifest?.get("records")?.obj ?: emptyMap()

    /** A kind's schema in [manifest] (null = undeclared). */
    fun dataSchema(manifest: JsonValue?, kind: String): JsonValue? = recordsBlock(manifest)[kind]?.get("schema")

    /** The head manifest's kinds, sorted by name, each with its live count
     *  (`kindCases`); [rowKinds] = the kind of every synced row of the app (an
     *  undeclared one is ignored). */
    fun dataKinds(manifest: JsonValue?, rowKinds: List<String>): List<DataKind> =
        recordsBlock(manifest).keys.sorted().map { kind -> DataKind(kind, rowKinds.count { it == kind }) }

    /** The columns (`columnCases`): the `required` names in their order, then
     *  every other property sorted by name. */
    fun dataColumns(schema: JsonValue?): List<String> {
        val names = schema?.get("properties")?.obj?.keys ?: emptySet()
        val required = (schema?.get("required")?.array ?: emptyList()).mapNotNull { it.string }.filter { it in names }.distinct()
        return required + names.filter { it !in required }.sorted()
    }

    /** A kind's rows, `updatedAt` desc, ties by id (`rowsCases`). */
    fun dataRows(rows: List<Row>, kind: String): List<Row> =
        rows.filter { it.kind == kind }.sortedWith { a, b ->
            if (a.updatedAt == b.updatedAt) a.id.compareTo(b.id) else b.updatedAt.compareTo(a.updatedAt)
        }

    /** One cell's text (`cellCases`): null/missing = "", a boolean Yes/No, a
     *  number as written (`3`, never `3.0`), a list joined `, `, an object its JSON. */
    fun cellText(value: JsonValue?): String = when (value) {
        null, JsonValue.Null -> ""
        is JsonValue.Bool -> if (value.v) DataCopy.YES else DataCopy.NO
        is JsonValue.Num -> value.displayText
        is JsonValue.Str -> value.v
        is JsonValue.Arr -> value.v.joinToString(", ") { cellText(it) }
        is JsonValue.Obj -> value.orderedJson
    }

    /** The phone row's title + subtitle: the first column's cell
     *  (`copy.emptyTitle` when empty), the other non-empty cells joined ` · `. */
    fun rowTitle(columns: List<String>, row: Row): String =
        columns.firstOrNull()?.let { cellText(row.data[it]) }?.takeIf { it.isNotEmpty() } ?: DataCopy.EMPTY_TITLE

    fun rowSubtitle(columns: List<String>, row: Row): String =
        columns.drop(1).map { cellText(row.data[it]) }.filter { it.isNotEmpty() }.joinToString(" · ")

    /** VAPP-188: the `by` cell (`bylineCases`): the author's name (else
     *  [fallback], apps.json `copy.byAgent`), `{writer} for {author}` when
     *  someone else with a name wrote it. */
    fun recordByline(row: Row, names: Map<String, String>, fallback: String): String {
        val authorId = recordAuthor(row)
        val author = authorId?.let { names[it] }?.takeIf { it.isNotEmpty() } ?: fallback
        val writer = row.createdBy?.takeIf { it != authorId }?.let { names[it] }?.takeIf { it.isNotEmpty() }
        return if (writer != null) DataCopy.BY_FOR.replace("{writer}", writer).replace("{author}", author) else author
    }

    /** VAPP-188: one team member in the owner's Author picker. */
    data class Member(val id: String, val name: String)

    /** VAPP-188: the owner's Author picker, members by name (case-insensitive,
     *  ties by id: `authorOptionCases`). */
    fun authorOptions(users: List<Member>): List<Member> =
        users.sortedWith(compareBy<Member> { it.name.lowercase(Locale.ROOT) }.thenBy { it.id })

    /** VAPP-188: [formAuthor]'s answer: the picker's [default] and what Save
     *  sends ([send]; null = omit `author`). */
    data class FormAuthor(val default: String?, val send: String?)

    /** VAPP-188: the picker's default (the record's author on edit, the viewer
     *  on create) and what Save sends (the pick when it differs from the
     *  record's author / the viewer, else null; `formAuthorCases`). */
    fun formAuthor(viewer: String?, record: Row?, picked: String?): FormAuthor {
        val base = if (record != null) recordAuthor(record) else viewer
        return FormAuthor(default = base ?: viewer, send = if (picked != null && picked != base) picked else null)
    }

    object Controls {
        const val TEXT = "text"
        const val NUMBER = "number"
        const val SWITCH = "switch"
        const val SELECT = "select"
        const val LIST = "list"
    }

    data class FormField(
        val name: String,
        /** [Controls]. */
        val control: String,
        val required: Boolean,
        val placeholder: String? = null,
        val options: List<String>? = null,
        val itemType: String? = null,
    )

    /** The form's fields, in column order (`formCases`). */
    fun formFields(schema: JsonValue?): List<FormField> {
        val required = (schema?.get("required")?.array ?: emptyList()).mapNotNull { it.string }
        val properties = schema?.get("properties")?.obj ?: emptyMap()
        return dataColumns(schema).map { name ->
            val field = properties[name]
            val isRequired = name in required
            val type = field?.get("type")?.string
            val enum = field?.get("enum")?.array
            when {
                type == "boolean" -> FormField(name, Controls.SWITCH, isRequired)
                type == "number" || type == "integer" -> FormField(name, Controls.NUMBER, isRequired)
                type == "array" -> FormField(name, Controls.LIST, isRequired, itemType = field?.get("items")?.get("type")?.string ?: "string")
                enum != null -> FormField(name, Controls.SELECT, isRequired, options = enum.mapNotNull { it.string })
                else -> FormField(name, Controls.TEXT, isRequired, placeholder = field?.get("format")?.string?.let { DATA_PLACEHOLDERS[it] })
            }
        }
    }

    /** A stored value → the control's initial input (`inputCases`): a switch
     *  takes a Boolean, every other control a String. */
    fun formInput(control: String, value: JsonValue?): Any {
        if (control == Controls.SWITCH) return value == JsonValue.Bool(true)
        if (value == null || value == JsonValue.Null) return ""
        if (control == Controls.LIST && value is JsonValue.Arr) return value.v.joinToString(", ") { cellText(it) }
        return if (value is JsonValue.Str) value.v else value.displayText
    }

    private fun parseNumber(text: String): JsonValue {
        val n = if (text.trim().isEmpty()) null else jsNumber(text)
        return if (n != null && n.isFinite()) JsonValue.Num(n) else JsonValue.Str(text)
    }

    private fun parseItem(text: String, type: String?): JsonValue = when (type) {
        "number", "integer" -> parseNumber(text)
        "boolean" -> if (text == "true") JsonValue.Bool(true) else if (text == "false") JsonValue.Bool(false) else JsonValue.Str(text)
        else -> JsonValue.Str(text)
    }

    /** The inputs (String or Boolean by control) → the record's data
     *  (`parseCases`): an empty field = null, numbers and list items parse
     *  (unparsable stays the raw text, so the server names it); CREATE omits
     *  the nulls, EDIT keeps them (null clears). */
    fun formData(schema: JsonValue?, inputs: Map<String, Any?>, editing: Boolean): Map<String, JsonValue> {
        val data = LinkedHashMap<String, JsonValue>()
        for (field in formFields(schema)) {
            val input = inputs[field.name]
            val value: JsonValue = if (field.control == Controls.SWITCH) {
                JsonValue.Bool(input == true)
            } else {
                val text = input as? String ?: ""
                when {
                    field.control == Controls.LIST -> {
                        val items = text.split(",").map { it.trim() }.filter { it.isNotEmpty() }
                        if (items.isEmpty()) JsonValue.Null else JsonValue.Arr(items.map { parseItem(it, field.itemType) })
                    }
                    text.isEmpty() -> JsonValue.Null
                    field.control == Controls.NUMBER -> parseNumber(text)
                    else -> JsonValue.Str(text)
                }
            }
            if (value != JsonValue.Null || editing) data[field.name] = value
        }
        return data
    }

    /** Settings → Apps → the app: "1,240 records", "1,240 of 25,000 records"
     *  ([max] = the team's quota, null = unlimited), "No records". */
    fun recordsCountLabel(count: Int, max: Long?): String {
        if (count == 0) return Apps.Copy.RECORDS_NONE
        val format = NumberFormat.getIntegerInstance(Locale.US)
        val n = format.format(count)
        if (max == null) return if (count == 1) Apps.Copy.RECORDS_ONE else Apps.Copy.RECORDS.replace("{n}", n)
        return Apps.Copy.RECORDS_OF.replace("{n}", n).replace("{max}", format.format(max))
    }
}
