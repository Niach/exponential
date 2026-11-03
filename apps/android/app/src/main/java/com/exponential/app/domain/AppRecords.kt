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
// purge and Data tab cases). VAPP-189: the `records:<kind>?…` query and its
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

    /** eApps R3: who may read, create, update and delete a kind's records
     *  under the member's app access: a hand port of eUI's
     *  `src/host/record-policy.ts` (packages/exponential-ui
     *  fixtures/record-policy.json is the spec; AppRecordsTest replays it). */
    object Policy {
        val OPS = listOf("read", "create", "update", "delete")
        val LEVELS = listOf("member", "author", "owners")
        val ACCESS = listOf("none", "read", "write")
        val DEFAULTS: Map<String, String> = mapOf("read" to "member", "create" to "member", "update" to "author", "delete" to "author")
        const val COPY_NONE = "You have no access to this app"
        const val COPY_READ_ONLY = "You have read-only access to this app"
        val COPY_AUTHOR: Map<String, String> = mapOf(
            "read" to "Only the record's author or a team owner can read it",
            "update" to "Only the record's author or a team owner can change it",
            "delete" to "Only the record's author or a team owner can change it",
        )
        val COPY_OWNERS: Map<String, String> = mapOf(
            "read" to "Only team owners can read these records",
            "create" to "Only team owners can add these records",
            "update" to "Only team owners can change these records",
            "delete" to "Only team owners can change these records",
        )

        /** A kind's effective policy (`kind` = the package's `records.<kind>`;
         *  an invalid entry keeps the default). */
        fun effective(kind: JsonValue?): Map<String, String> {
            val out = DEFAULTS.toMutableMap()
            val policy = kind?.obj?.get("policy")?.obj ?: return out
            for (op in OPS) {
                val level = policy[op]?.string ?: continue
                if (level in LEVELS && !(op == "create" && level == "author")) out[op] = level
            }
            return out
        }

        /** The policy of [kind] in a package JSON. */
        fun of(pkg: JsonValue?, kind: String): Map<String, String> = effective(pkg?.obj?.get("records")?.obj?.get(kind))

        /** Why [viewer] (team [role], null = not a member; app [access], null =
         *  write) may NOT [op] a record ([row] for read/update/delete), or null. */
        fun refusal(policy: Map<String, String>, op: String, viewer: String?, role: String?, access: String? = null, row: Row? = null): String? {
            val level0 = access ?: "write"
            if (viewer == null || role == null || level0 == "none") return COPY_NONE
            if (level0 == "read" && op != "read") return COPY_READ_ONLY
            if (role == "owner") return null
            val level = policy[op] ?: DEFAULTS[op] ?: "member"
            if (level == "member") return null
            if (level == "owners" || op == "create") return COPY_OWNERS[op]
            val author = row?.let(::recordAuthor)
            return if (author != null && author == viewer) null else COPY_AUTHOR[op]
        }

        fun allowed(policy: Map<String, String>, op: String, viewer: String?, role: String?, access: String? = null, row: Row? = null): Boolean =
            refusal(policy, op, viewer, role, access, row) == null
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
     *  importer is an owner, else the importer too (VAPP-192: a member
     *  importing a teammate's export keeps the rows as their own). */
    fun importAuthor(importer: String, role: String?, author: String?, members: List<String>): String? {
        if (author == null || author == importer || author !in members) return importer
        return if (role == "owner") author else importer
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
        const val DELETE_BODY = "The app stops showing it."
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
        const val HISTORY = "app-data-history"
        const val HISTORY_ROW = "app-data-history-row"
        const val RESTORE = "app-data-restore"
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

    // VAPP-215 (R4) RECORD HISTORY + UNDO (fixture `history`, locked by
    // AppRecordsTest): what one `app_record_events` row means, its undo and
    // its History row (TS `eventState`, `inverseWrite`, `eventLabel`,
    // `historyRows`, `undoTitle`).
    object History {
        const val UNDO_SECONDS = 10
        const val RETENTION_DAYS = 90
        const val BATCH = 500
        const val HISTORY_LIMIT = 50
        const val MAX_FIELDS = 3
        val OPS = listOf("create", "update", "delete", "restore")
        val VIAS = listOf("editor", "app", "mcp", "action", "import")
    }

    object HistoryCopy {
        const val HISTORY = "History"
        const val RESTORE = "Restore"
        const val UNDO = "Undo"
        const val CREATED = "Created"
        const val DELETED = "Deleted"
        const val RESTORED = "Restored"
        const val CHANGED = "Changed {fields}"
        const val MORE = "+{n}"
        const val SAVED = "Saved"
        const val EMPTY = "No history yet"
        const val UNDO_CREATE = "Record added"
        const val UNDO_UPDATE = "Record saved"
        const val UNDO_DELETE = "Record deleted"
        const val UNDO_BATCH = "Records saved"
        const val UNDO_VERB = "undo"
        const val UNDO_THING = "the change"
    }

    /** One event as `apps.records.history` returns it (newest first);
     *  the pure rules read [op], [before] and [patch]. */
    data class RecordEvent(
        val id: String = "",
        val op: String,
        val before: Map<String, JsonValue>?,
        val patch: Map<String, JsonValue>?,
        val actor: String? = null,
        val createdAt: String = "",
    )

    /** The write that undoes an event: a delete ([data] null) or a restore of [data]. */
    data class InverseWrite(val op: String, val data: Map<String, JsonValue>? = null)

    /** The record an event left (`stateCases`): null = gone. */
    fun eventState(event: RecordEvent): Map<String, JsonValue>? = when (event.op) {
        "delete" -> null
        "update" -> (event.before ?: emptyMap()) + (event.patch ?: emptyMap())
        else -> event.patch ?: emptyMap()
    }

    /** Undo = a new write of the event's `before` (`inverseCases`). */
    fun inverseWrite(event: RecordEvent): InverseWrite =
        if (event.before == null) InverseWrite("delete") else InverseWrite("restore", event.before)

    /** Deep JSON equality (objects by key, numbers by value). */
    fun jsonEqual(a: JsonValue?, b: JsonValue?): Boolean {
        if (a == null || b == null) return a == null && b == null
        return when {
            a is JsonValue.Num && b is JsonValue.Num -> a.v == b.v
            a is JsonValue.Arr && b is JsonValue.Arr -> a.v.size == b.v.size && a.v.indices.all { jsonEqual(a.v[it], b.v[it]) }
            a is JsonValue.Obj && b is JsonValue.Obj -> a.v.size == b.v.size && a.v.all { (k, v) -> b.v.containsKey(k) && jsonEqual(v, b.v[k]) }
            else -> a == b
        }
    }

    /** A History row's title (`labelCases`). */
    fun eventLabel(event: RecordEvent): String {
        val c = HistoryCopy
        when (event.op) {
            "create" -> return c.CREATED
            "delete" -> return c.DELETED
            "restore" -> return c.RESTORED
        }
        val before = event.before ?: emptyMap()
        val patch = event.patch ?: emptyMap()
        val changed = patch.keys.filter { !jsonEqual(before[it], patch[it]) }.sorted()
        if (changed.isEmpty()) return c.SAVED
        val max = History.MAX_FIELDS
        val shown = changed.take(max).joinToString(", ")
        val rest = changed.size - max
        return c.CHANGED.replace("{fields}", if (rest > 0) "$shown ${c.MORE.replace("{n}", rest.toString())}" else shown)
    }

    data class HistoryRow(val id: String, val label: String, val who: String, val at: String, val restorable: Boolean)

    /** The History list (`rowsCases`): the events in the server's order
     *  (newest first); Restore on every row but the newest, never on a delete. */
    fun historyRows(events: List<RecordEvent>, names: Map<String, String>, fallback: String): List<HistoryRow> =
        events.mapIndexed { i, event ->
            HistoryRow(
                id = event.id,
                label = eventLabel(event),
                who = event.actor?.let { names[it] } ?: fallback,
                at = event.createdAt,
                restorable = i > 0 && event.op != "delete",
            )
        }

    /** The Undo toast's title for a UI write (`create` | `update` | `delete` | `batch`). */
    fun undoTitle(write: String): String = when (write) {
        "create" -> HistoryCopy.UNDO_CREATE
        "update" -> HistoryCopy.UNDO_UPDATE
        "delete" -> HistoryCopy.UNDO_DELETE
        else -> HistoryCopy.UNDO_BATCH
    }

    // VAPP-235 (O1) THE OFFLINE OUTBOX (fixture `outbox`, locked by
    // AppRecordsTest): every host record write is ONE entry, laid over the
    // synced rows and replayed in order with an idempotency key (TS
    // `outboxWrites`, `outboxRequest`, `outboxRows`, `outboxOutcome`,
    // `outboxVerb`, `outboxFlush`, `outboxReflected`, `outboxPrune`,
    // `outboxPendingLabel`).
    object Outbox {
        const val ACK_GRACE_SECONDS = 30
        val RETRY_CODES: List<String> = listOf(
            "UNAUTHORIZED", "TIMEOUT", "TOO_MANY_REQUESTS", "CLIENT_CLOSED_REQUEST",
            "INTERNAL_SERVER_ERROR", "BAD_GATEWAY", "SERVICE_UNAVAILABLE", "GATEWAY_TIMEOUT",
        )
        const val PENDING_ONE = "1 change waiting to sync"
        const val PENDING = "{n} changes waiting to sync"

        const val PENDING_STATE = "pending"
        const val ACKED_STATE = "acked"
    }

    /** One write of an entry: `create` | `update` (with [kind] + [data]) or `delete`. */
    data class OutboxWrite(
        val op: String,
        val id: String,
        val kind: String = "",
        val data: Map<String, JsonValue> = emptyMap(),
    )

    /** One outbox entry; [call] = `upsert` | `delete` | `batch`, [state] = `pending` | `acked`. */
    data class OutboxEntry(
        val writeId: String,
        val appId: String,
        val call: String,
        val writes: List<OutboxWrite>,
        /** ISO time the write was made. */
        val at: String,
        val state: String = Outbox.PENDING_STATE,
        val ackedAt: String? = null,
    )

    /** A replay's answer: ok, no network, or the server's tRPC code. */
    sealed class OutboxResult {
        data object Ok : OutboxResult()
        data object Transport : OutboxResult()
        data class Code(val code: String) : OutboxResult()
    }

    object OutboxOutcome {
        const val DONE = "done"
        const val RETRY = "retry"
        const val DROPPED = "dropped"
        const val REFUSED = "refused"
    }

    /** The replay request of an entry: the call and its input (the host adds the app). */
    data class OutboxRequest(val call: String, val input: Map<String, JsonValue>)

    data class OutboxRefusal(val writeId: String, val verb: String)

    data class OutboxFlush(val sent: List<String>, val refused: List<OutboxRefusal>, val after: List<OutboxEntry>)

    /** A write as stored / in the fixture (`{op, id, kind?, data?}`). */
    fun outboxWrite(v: JsonValue): OutboxWrite {
        val op = v["op"]?.string.orEmpty()
        val id = v["id"]?.string.orEmpty()
        return if (op == "delete") OutboxWrite(op, id) else OutboxWrite(op, id, v["kind"]?.string.orEmpty(), v["data"]?.obj ?: emptyMap())
    }

    fun outboxWriteJson(w: OutboxWrite): JsonValue = JsonValue.Obj(
        if (w.op == "delete") {
            linkedMapOf("op" to JsonValue.Str(w.op), "id" to JsonValue.Str(w.id))
        } else {
            linkedMapOf("op" to JsonValue.Str(w.op), "id" to JsonValue.Str(w.id), "kind" to JsonValue.Str(w.kind), "data" to JsonValue.Obj(w.data))
        },
    )

    /** The entry's writes as the stored JSON text, and back. */
    fun outboxWritesJson(writes: List<OutboxWrite>): String = JsonValue.Arr(writes.map(::outboxWriteJson)).orderedJson

    fun outboxWritesOf(text: String): List<OutboxWrite> =
        runCatching { JsonValue.parse(text).array.orEmpty().map(::outboxWrite) }.getOrDefault(emptyList())

    /** A host call's writes (`writesCases`): a create takes the next client id. */
    fun outboxWrites(call: String, args: Map<String, JsonValue>, newIds: List<String>): List<OutboxWrite> {
        var next = 0
        fun upsert(w: Map<String, JsonValue>): OutboxWrite {
            val data = (w["data"] as? JsonValue.Obj)?.v ?: emptyMap()
            val kind = w["kind"]?.string.orEmpty()
            val id = w["id"]?.string
            return if (id != null) OutboxWrite("update", id, kind, data) else OutboxWrite("create", newIds.getOrNull(next++).orEmpty(), kind, data)
        }
        return when (call) {
            "upsert" -> listOf(upsert(args))
            "delete" -> listOf(OutboxWrite("delete", args["id"]?.string.orEmpty()))
            else -> (args["writes"]?.array.orEmpty()).mapNotNull { it.obj }.map { w ->
                if (w["op"]?.string == "delete") OutboxWrite("delete", w["id"]?.string.orEmpty()) else upsert(w)
            }
        }
    }

    /** How many client ids a call's writes take (its creates). */
    fun outboxCreates(call: String, args: Map<String, JsonValue>): Int = when (call) {
        "upsert" -> if (args["id"]?.string == null) 1 else 0
        "delete" -> 0
        else -> (args["writes"]?.array.orEmpty()).mapNotNull { it.obj }.count { it["op"]?.string != "delete" && it["id"]?.string == null }
    }

    /** The replay request of an entry (`requestCases`). */
    fun outboxRequest(entry: OutboxEntry): OutboxRequest {
        val writeId = JsonValue.Str(entry.writeId)
        if (entry.call == "batch") {
            val writes = entry.writes.map { w ->
                JsonValue.Obj(
                    when (w.op) {
                        "delete" -> linkedMapOf("op" to JsonValue.Str("delete"), "id" to JsonValue.Str(w.id))
                        "create" -> linkedMapOf("op" to JsonValue.Str("upsert"), "kind" to JsonValue.Str(w.kind), "data" to JsonValue.Obj(w.data), "newId" to JsonValue.Str(w.id))
                        else -> linkedMapOf("op" to JsonValue.Str("upsert"), "kind" to JsonValue.Str(w.kind), "data" to JsonValue.Obj(w.data), "id" to JsonValue.Str(w.id))
                    },
                )
            }
            return OutboxRequest("batch", linkedMapOf("writes" to JsonValue.Arr(writes), "writeId" to writeId))
        }
        val w = entry.writes.first()
        if (w.op == "delete") return OutboxRequest("delete", linkedMapOf("recordId" to JsonValue.Str(w.id), "writeId" to writeId))
        return OutboxRequest(
            "upsert",
            linkedMapOf(
                "kind" to JsonValue.Str(w.kind),
                "data" to JsonValue.Obj(w.data),
                (if (w.op == "create") "newId" else "recordId") to JsonValue.Str(w.id),
                "writeId" to writeId,
            ),
        )
    }

    /** The visible rows: the synced rows with every entry laid over them,
     *  oldest first (`rowsCases`). */
    fun outboxRows(rows: List<Row>, entries: List<OutboxEntry>, me: String?): List<Row> {
        val out = rows.toMutableList()
        for (entry in entries) {
            for (w in entry.writes) {
                val i = out.indexOfFirst { it.id == w.id }
                when {
                    w.op == "delete" -> if (i >= 0) out.removeAt(i)
                    i >= 0 -> out[i] = out[i].copy(data = out[i].data + w.data)
                    w.op == "create" -> out += Row(
                        id = w.id, kind = w.kind, data = w.data, createdBy = me,
                        createdAt = entry.at, updatedAt = entry.at, authorId = me,
                    )
                }
            }
        }
        return out
    }

    /** What a replay's answer does to its entry (`outcomeCases`). */
    fun outboxOutcome(entry: OutboxEntry, result: OutboxResult): String = when (result) {
        OutboxResult.Ok -> OutboxOutcome.DONE
        OutboxResult.Transport -> OutboxOutcome.RETRY
        is OutboxResult.Code -> when {
            result.code in Outbox.RETRY_CODES -> OutboxOutcome.RETRY
            result.code == "NOT_FOUND" && entry.call != "batch" && entry.writes.none { it.op == "create" } -> OutboxOutcome.DROPPED
            else -> OutboxOutcome.REFUSED
        }
    }

    /** The failed-action toast's verb for a refused entry (`save` | `delete`). */
    fun outboxVerb(entry: OutboxEntry): String = if (entry.writes.all { it.op == "delete" }) "delete" else "save"

    /** One flush (`flushCases`): pending entries oldest first, one at a time;
     *  a retry stops it; [results] = each write id's answer (none = no network). */
    fun outboxFlush(entries: List<OutboxEntry>, results: Map<String, OutboxResult>, now: String): OutboxFlush {
        val sent = mutableListOf<String>()
        val refused = mutableListOf<OutboxRefusal>()
        val after = mutableListOf<OutboxEntry>()
        var stopped = false
        for (entry in entries) {
            if (stopped || entry.state == Outbox.ACKED_STATE) {
                after += entry
                continue
            }
            sent += entry.writeId
            when (outboxOutcome(entry, results[entry.writeId] ?: OutboxResult.Transport)) {
                OutboxOutcome.DONE -> after += entry.copy(state = Outbox.ACKED_STATE, ackedAt = now)
                OutboxOutcome.RETRY -> {
                    stopped = true
                    after += entry
                }
                OutboxOutcome.REFUSED -> refused += OutboxRefusal(entry.writeId, outboxVerb(entry))
            }
        }
        return OutboxFlush(sent, refused, after)
    }

    /** Do the synced rows reflect an entry? */
    fun outboxReflected(entry: OutboxEntry, rows: List<Row>): Boolean = entry.writes.all { w ->
        val row = rows.firstOrNull { it.id == w.id }
        if (w.op == "delete") row == null else row != null && w.data.all { (k, v) -> jsonEqual(row.data[k], v) }
    }

    private fun epochMs(iso: String?): Long? = iso?.let { runCatching { Instant.parse(it).toEpochMilli() }.getOrNull() ?: WireTimestamps.parseEpochMs(it) }

    /** The entries that stay (`pruneCases`): pending ones, and acked ones the
     *  rows don't reflect yet within the grace. */
    fun outboxPrune(entries: List<OutboxEntry>, rows: List<Row>, now: String): List<OutboxEntry> {
        val grace = Outbox.ACK_GRACE_SECONDS * 1000L
        val nowMs = epochMs(now) ?: return entries
        return entries.filter { e ->
            if (e.state != Outbox.ACKED_STATE) return@filter true
            if (outboxReflected(e, rows)) return@filter false
            val acked = epochMs(e.ackedAt ?: e.at) ?: return@filter false
            nowMs - acked < grace
        }
    }

    /** The header glyph's label for an app's pending entries
     *  (`pendingCases`); null = no glyph. */
    fun outboxPendingLabel(entries: List<OutboxEntry>, appId: String): String? {
        val n = entries.count { it.appId == appId && it.state == Outbox.PENDING_STATE }
        if (n == 0) return null
        return if (n == 1) Outbox.PENDING_ONE else Outbox.PENDING.replace("{n}", n.toString())
    }
}
