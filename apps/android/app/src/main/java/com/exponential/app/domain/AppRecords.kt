package com.exponential.app.domain

import at.exponential.ui.host.ParsedSource
import at.exponential.ui.json.JsonValue
import com.exponential.app.data.db.AppRecordEntity
import java.text.NumberFormat
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.LocalDate
import java.time.YearMonth
import java.util.Locale
import kotlin.math.floor

// VAPP-122: app RECORDS, the Android mirror of the pure reference
// `packages/domain-contract/src/app-records.ts` (fixtures/app-records.json is
// the spec; `AppRecordsTest` replays its `queryCases`, `valueCases` and
// `countLabelCases` (and VAPP-161 `sourceCases`) and locks the constants below to the fixture). The
// `exp:records?…` binding's query grammar and the value it emits over the
// synced `app_records` rows.
object AppRecords {
    // Fixture `query`, `rowKeys`, `weekdays`, `months` (locked by AppRecordsTest).
    val RESERVED: List<String> = listOf("kind", "app", "mine", "user", "sort", "order", "limit", "sum", "by", "month")
    const val SORT_DEFAULT = "createdAt"
    const val ORDER_DEFAULT = "desc"
    const val LIMIT_DEFAULT = 100
    const val LIMIT_MAX = 500
    val OPERATORS: List<String> = listOf("eq", "gt", "gte", "lt", "lte")
    val ROW_KEYS: List<String> = listOf("id", "createdAt", "updatedAt", "createdBy", "author")
    val WEEKDAYS: List<String> = listOf("Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat")
    val MONTHS: List<String> = listOf(
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    )

    data class Filter(val field: String, val op: String, val value: String)

    data class RecordsQuery(
        val kind: String?,
        /** Another app's slug; null = the hosting app. */
        val app: String?,
        val mine: Boolean,
        val user: String?,
        val sort: String,
        /** `asc` | `desc`. */
        val order: String,
        val limit: Int,
        /** By field, then operator. */
        val filters: List<Filter>,
        val sum: String?,
        val by: String?,
        val month: String?,
    )

    /** One synced `app_records` row as the host reads it ([data] = the parsed object). */
    data class Row(
        val id: String,
        val kind: String,
        val data: Map<String, JsonValue>,
        val createdBy: String?,
        val createdAt: String,
        val updatedAt: String,
    )

    data class Day(val date: String, val day: Int, val weekday: String, var count: Int = 0, var sum: Double = 0.0)

    private val MONTH = Regex("^\\d{4}-(0[1-9]|1[0-2])$")
    private val RANGE_OPS = listOf("gt", "gte", "lt", "lte")

    /** VAPP-161: a parsed binding source → the params [parseRecordsQuery]
     *  takes, for both spellings: the format's neutral `records:<kind>?q` (the
     *  name wins over a `kind` param) and `exp:records?kind=<kind>&q`; null =
     *  not a records source. */
    fun recordsSourceParams(source: ParsedSource): Map<String, String>? = when {
        source.scheme == "records" -> (source.params + ("kind" to source.name)).toSortedMap()
        source.scheme == "exp" && source.name == "records" -> source.params
        else -> null
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

    private fun String?.trimmedOrNull(): String? = this?.trim()?.takeIf { it.isNotEmpty() }

    /** The `exp:records` query params (as `ParsedSource` gives them) → the query. Unknown keys are data-field filters. */
    fun parseRecordsQuery(params: Map<String, String>): RecordsQuery {
        val limitRaw = jsNumber(params["limit"])
        val limit = if (limitRaw != null && limitRaw.isFinite() && limitRaw == floor(limitRaw) && limitRaw > 0) {
            minOf(limitRaw, LIMIT_MAX.toDouble()).toInt()
        } else {
            LIMIT_DEFAULT
        }
        val filters = ArrayList<Filter>()
        for (key in params.keys.sorted()) {
            if (key in RESERVED) continue
            val dot = key.lastIndexOf('.')
            val suffix = if (dot >= 0) key.substring(dot + 1) else ""
            if (dot > 0 && suffix in RANGE_OPS) filters.add(Filter(key.substring(0, dot), suffix, params.getValue(key)))
            else if (key.isNotEmpty()) filters.add(Filter(key, "eq", params.getValue(key)))
        }
        filters.sortWith(compareBy<Filter> { it.field }.thenBy { it.op })
        val order = params["order"]
        val month = params["month"]
        return RecordsQuery(
            kind = params["kind"].trimmedOrNull(),
            app = params["app"].trimmedOrNull(),
            mine = params["mine"] == "1" || params["mine"] == "true",
            user = params["user"].trimmedOrNull(),
            sort = params["sort"].trimmedOrNull() ?: SORT_DEFAULT,
            order = if (order == "asc" || order == "desc") order else ORDER_DEFAULT,
            limit = limit,
            filters = filters,
            sum = params["sum"].trimmedOrNull(),
            by = params["by"].trimmedOrNull(),
            month = if (month != null && MONTH.matches(month)) month else null,
        )
    }

    /** The reference's `num`: a finite number, or a non-blank numeric string; else null (booleans too). */
    private fun num(v: JsonValue?): Double? = when (v) {
        is JsonValue.Num -> v.v.takeIf { it.isFinite() }
        is JsonValue.Str -> if (v.v.isBlank()) null else jsNumber(v.v)?.takeIf { it.isFinite() }
        else -> null
    }

    /** The reference's `text`: "" for null/missing, a string as is, objects as JSON, else `String(v)`. */
    private fun text(v: JsonValue?): String = when (v) {
        null, JsonValue.Null -> ""
        is JsonValue.Str -> v.v
        is JsonValue.Arr, is JsonValue.Obj -> v.orderedJson
        else -> v.displayText
    }

    /** Numbers when both parse, else text (UTF-16 code units, no locale: the same ×4). */
    private fun compare(a: JsonValue?, b: JsonValue?): Int {
        val na = num(a)
        val nb = num(b)
        if (na != null && nb != null) return if (na < nb) -1 else if (na > nb) 1 else 0
        return text(a).compareTo(text(b)).coerceIn(-1, 1)
    }

    private fun matches(filter: Filter, value: JsonValue?): Boolean {
        if (filter.op == "eq") return text(value) == filter.value
        val c = compare(value, JsonValue.Str(filter.value))
        return when (filter.op) {
            "gt" -> c > 0
            "gte" -> c >= 0
            "lt" -> c < 0
            else -> c <= 0
        }
    }

    private fun yearMonth(key: String): Pair<Int, Int> {
        val parts = key.split("-")
        return parts[0].toInt() to parts[1].toInt()
    }

    /** `YYYY-MM` ± [by] months. */
    fun shiftMonth(key: String, by: Int): String {
        val (y, m) = yearMonth(key)
        val index = y * 12 + (m - 1) + by
        val year = Math.floorDiv(index, 12)
        val month = Math.floorMod(index, 12) + 1
        return "$year-${month.toString().padStart(2, '0')}"
    }

    fun monthLabel(key: String): String {
        val (y, m) = yearMonth(key)
        return "${MONTHS[m - 1]} $y"
    }

    /** Every day of [key] (`YYYY-MM`), empty. */
    fun monthDays(key: String): List<Day> {
        val (y, m) = yearMonth(key)
        val count = YearMonth.of(y, m).lengthOfMonth()
        return (1..count).map { day ->
            // java.time: MONDAY = 1 … SUNDAY = 7; the fixture's weekdays start on Sunday.
            val weekday = LocalDate.of(y, m, day).dayOfWeek.value % 7
            Day(date = "$key-${day.toString().padStart(2, '0')}", day = day, weekday = WEEKDAYS[weekday])
        }
    }

    /** Sums of hours like 0.1 + 0.2 stay readable (and identical ×4): JS `Math.round(n * 1e6) / 1e6`. */
    private fun round(n: Double): Double = floor(n * 1_000_000 + 0.5) / 1_000_000

    private fun field(row: Row, name: String): JsonValue? = when (name) {
        "createdAt" -> JsonValue.Str(row.createdAt)
        "updatedAt" -> JsonValue.Str(row.updatedAt)
        "id" -> JsonValue.Str(row.id)
        "createdBy" -> row.createdBy?.let { JsonValue.Str(it) } ?: JsonValue.Null
        else -> row.data[name]
    }

    /**
     * The value `exp:records` emits for [query] over [rows] (every synced row
     * of the app, any kind): null without a kind; else {rows, count, sum?,
     * month?} as the fixture spells out. [me] = the viewer (null = `mine`
     * matches nothing), [now] = ISO 8601 UTC, [names] = user id → name.
     */
    fun recordsValue(rows: List<Row>, query: RecordsQuery, me: String?, now: String, names: Map<String, String>): JsonValue? {
        val kind = query.kind ?: return null
        val by = query.by
        val monthKey = if (by != null) (query.month ?: now.take(7)) else null
        val matched = rows.filter { row ->
            if (row.kind != kind) return@filter false
            if (query.mine && (me == null || row.createdBy != me)) return@filter false
            if (query.user != null && row.createdBy != query.user) return@filter false
            if (monthKey != null && by != null && !text(row.data[by]).startsWith(monthKey)) return@filter false
            query.filters.all { matches(it, field(row, it.field)) }
        }.sortedWith { a, b ->
            val c = compare(field(a, query.sort), field(b, query.sort))
            if (c != 0) (if (query.order == "asc") c else -c) else a.id.compareTo(b.id)
        }
        val out = LinkedHashMap<String, JsonValue>()
        out["rows"] = JsonValue.Arr(
            matched.take(query.limit).map { row ->
                val o = LinkedHashMap<String, JsonValue>(row.data)
                o["id"] = JsonValue.Str(row.id)
                o["createdAt"] = JsonValue.Str(row.createdAt)
                o["updatedAt"] = JsonValue.Str(row.updatedAt)
                o["createdBy"] = row.createdBy?.let { JsonValue.Str(it) } ?: JsonValue.Null
                o["author"] = JsonValue.Str(row.createdBy?.let { names[it] }?.takeIf { it.isNotEmpty() } ?: "")
                JsonValue.Obj(o)
            },
        )
        out["count"] = JsonValue.Num(matched.size.toDouble())
        val sum = query.sum
        if (sum != null) out["sum"] = JsonValue.Num(round(matched.fold(0.0) { acc, row -> acc + (num(row.data[sum]) ?: 0.0) }))
        if (monthKey != null && by != null) {
            val days = monthDays(monthKey)
            val byDate = days.associateBy { it.date }
            for (row in matched) {
                val day = byDate[text(row.data[by]).take(10)] ?: continue
                day.count += 1
                if (sum != null) day.sum = round(day.sum + (num(row.data[sum]) ?: 0.0))
            }
            out["month"] = JsonValue.Obj(
                linkedMapOf(
                    "key" to JsonValue.Str(monthKey),
                    "label" to JsonValue.Str(monthLabel(monthKey)),
                    "prev" to JsonValue.Str(shiftMonth(monthKey, -1)),
                    "next" to JsonValue.Str(shiftMonth(monthKey, 1)),
                    "today" to JsonValue.Str(now.take(10)),
                    "days" to JsonValue.Arr(
                        days.map {
                            JsonValue.Obj(
                                linkedMapOf(
                                    "date" to JsonValue.Str(it.date),
                                    "day" to JsonValue.Num(it.day.toDouble()),
                                    "weekday" to JsonValue.Str(it.weekday),
                                    "count" to JsonValue.Num(it.count.toDouble()),
                                    "sum" to JsonValue.Num(it.sum),
                                ),
                            )
                        },
                    ),
                    "labels" to JsonValue.Arr(days.map { JsonValue.Str(it.day.toString()) }),
                    "counts" to JsonValue.Arr(days.map { JsonValue.Num(it.count.toDouble()) }),
                    "sums" to JsonValue.Arr(days.map { JsonValue.Num(it.sum) }),
                ),
            )
        }
        return JsonValue.Obj(out)
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
    )

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
