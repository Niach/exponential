package at.exponential.ui.host

import at.exponential.ui.json.JsonValue

/**
 * VAPP-189: THE records query, the core's one evaluator
 * (`@exponential_app/eui/host/query`, eui-core `host::query`,
 * `fixtures/records-query.json`). A binding's URL sugar parses INTO the
 * structured query; [value] evaluates it over the rows of whatever store the
 * host has.
 */
class RecordsQuery private constructor(
    /** The structured query as the core spells it (JSON). */
    val json: String,
) {
    /** `{from, app, mine, user, where, or, sort, limit, sum, month, group, aggregate, running, before}`. */
    val structured: JsonValue = JsonValue.parse(json)

    /** The kind (null = no kind: the value is null). */
    val from: String? get() = structured["from"]?.string

    /** Another app's slug; the HOST decides whose rows that is. */
    val app: String? get() = structured["app"]?.string

    /**
     * The value the binding emits over [rows] (every row of the app, any
     * kind): [JsonValue.Null] without a kind, else `{rows, count, sum?,
     * month?, groups?, totals?, before?}`. [me] = the viewer (null: `mine` matches nothing), [now] =
     * ISO 8601 (the month view's default month), [names] = user id →
     * display name. VAPP-214: [computed] = the queried kind's computed
     * fields (name → CEL, [computed] in the companion), evaluated on read.
     * [today] = the host's calendar day in the team zone (`YYYY-MM-DD`): the
     * month view's `today` and default month; null = [now]'s UTC day.

     * VAPP-219 (R5): [refs] = the queried kind's refs ([refs] in the
     * companion), what `expand` follows; [issues] = identifier → the issue
     * summary `{id, identifier, title, status}` an `issue` ref inlines
     * (members resolve through [names]).
     */
    fun value(rows: List<RecordRow>, me: String?, now: String, names: Map<String, String> = emptyMap(), computed: Map<String, String> = emptyMap(), today: String? = null, refs: JsonValue = JsonValue.Null, issues: Map<String, JsonValue> = emptyMap()): JsonValue {
        val rowsJson = JsonValue.Arr(rows.map { it.toJson() }).json
        val context = JsonValue.Obj(
            mapOf(
                "me" to (me?.let { JsonValue.Str(it) } ?: JsonValue.Null),
                "now" to JsonValue.Str(now),
                "today" to (today?.let { JsonValue.Str(it) } ?: JsonValue.Null),
                "names" to JsonValue.Obj(names.mapValues { JsonValue.Str(it.value) }),
                "computed" to JsonValue.Obj(computed.mapValues { JsonValue.Str(it.value) }),
                "refs" to refs,
                "issues" to JsonValue.Obj(issues),
            ),
        ).json
        val out = runCatching { at.exponential.ui.ffi.recordsValueJson(rowsJson, json, context) }.getOrNull() ?: return JsonValue.Null
        return JsonValue.parse(out)
    }

    override fun equals(other: Any?): Boolean = other is RecordsQuery && other.json == json

    override fun hashCode(): Int = json.hashCode()

    override fun toString(): String = json

    companion object {
        /** VAPP-219 (R5): a package's refs of [kind] (`records.<kind>.refs`); what a host passes as `refs` (Null = none). */
        fun refs(records: JsonValue?, kind: String?): JsonValue = kind?.let { records?.get(it)?.get("refs") }?.takeIf { it.obj != null } ?: JsonValue.Null

        /** VAPP-214: a package's computed fields of [kind] (`records.<kind>.computed`, the string ones); what a host passes as `computed`. */
        fun computed(records: JsonValue?, kind: String?): Map<String, String> {
            val fields = kind?.let { records?.get(it)?.get("computed")?.obj } ?: return emptyMap()
            return fields.mapNotNull { (k, v) -> v.string?.let { k to it } }.toMap()
        }

        /** Query params (as [ParsedSource.params] gives them) → the query. [kind] = the source's own name (`records:<kind>`); a `kind` param is a field filter. */
        fun parse(params: Map<String, String>, kind: String? = null): RecordsQuery {
            val paramsJson = JsonValue.Obj(params.mapValues { JsonValue.Str(it.value) }).json
            return RecordsQuery(runCatching { at.exponential.ui.ffi.parseRecordsQueryJson(paramsJson, kind) }.getOrNull() ?: """{"from":null}""")
        }

        /** `records:<kind>?…` → the query; null = not a records source. */
        fun parse(source: String): RecordsQuery? = at.exponential.ui.ffi.parseRecordsSourceJson(source)?.let { RecordsQuery(it) }

        /** `records:<kind>?…` (parsed) → the query; null = another scheme. */
        fun parse(source: ParsedSource): RecordsQuery? = if (source.scheme == "records") parse(source.params, source.name) else null

        /**
         * VAPP-190: the JSON query form (a package's `queries`, the `q`
         * param: group, aggregate, running, before, in / between / or) →
         * the query and every problem with it (`[{path, message}]`; a
         * problem drops the part, the rest still runs). [kind] wins over `from`.
         */
        fun parseJson(json: String, kind: String? = null): Parsed {
            val out = runCatching { at.exponential.ui.ffi.parseRecordsJson(json, kind) }.getOrNull()
                ?: return Parsed(RecordsQuery("""{"from":null}"""), listOf(JsonValue.Obj(mapOf("path" to JsonValue.Str(""), "message" to JsonValue.Str("a query is JSON")))))
            val parsed = JsonValue.parse(out)
            return Parsed(RecordsQuery(parsed["query"]?.json ?: """{"from":null}"""), parsed["issues"]?.array.orEmpty())
        }

        /** VAPP-190: a date's bucket key (day `2026-10-05`, week `2026-W41`, month `2026-10`, quarter `2026-Q4`, year `2026`); null = no date that precise. */
        fun dateBucket(date: String, bucket: String): String? = at.exponential.ui.ffi.dateBucket(date, bucket)
    }

    /** [parseJson]'s answer. */
    data class Parsed(val query: RecordsQuery, val issues: List<JsonValue>)
}

/** One stored record as a host hands it to the evaluator. [authorId] = whose record it is (#VAPP-187); null = [createdBy]. */
data class RecordRow(
    val id: String,
    val kind: String,
    /** The record's data (an object; anything else reads as `{}`). */
    val data: JsonValue,
    val createdBy: String?,
    val authorId: String? = null,
    val createdAt: String,
    val updatedAt: String,
) {
    internal fun toJson(): JsonValue = JsonValue.Obj(
        mapOf(
            "id" to JsonValue.Str(id),
            "kind" to JsonValue.Str(kind),
            "data" to (data as? JsonValue.Obj ?: JsonValue.Obj(emptyMap())),
            "createdBy" to (createdBy?.let { JsonValue.Str(it) } ?: JsonValue.Null),
            "authorId" to (authorId?.let { JsonValue.Str(it) } ?: JsonValue.Null),
            "createdAt" to JsonValue.Str(createdAt),
            "updatedAt" to JsonValue.Str(updatedAt),
        ),
    )
}
