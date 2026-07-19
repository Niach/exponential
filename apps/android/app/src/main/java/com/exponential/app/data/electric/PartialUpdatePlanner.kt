package com.exponential.app.data.electric

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

/**
 * Result of planning a partial `update` against the local schema. [setClause]
 * is the `"col" = ?` fragment (empty when every wire column is unknown
 * locally), [args] are the bound values in the same order (never includes the
 * id), and [droppedColumns] are wire columns absent from the local table.
 */
internal data class PartialUpdatePlan(
    val setClause: String,
    val args: List<Any?>,
    val droppedColumns: Set<String>,
)

/**
 * Pure core of the tolerant partial-apply. Electric delivers a partial `update`
 * as a bag of changed columns keyed on the row's `id`. Two ways a naive
 * `UPDATE … SET "<wire-col>" = ?` bricks the sync loop, both handled here:
 *
 *  - The table's PK isn't a lone `id` (e.g. issue_labels' composite
 *    `(issue_id, label_id)`): there is no single-column `WHERE "id" = ?` to
 *    target, so partials are skipped entirely (return null).
 *  - The server ships a column the local schema predates (an older client
 *    against a newer server): the unknown column is filtered out of the SET
 *    list rather than thrown on. When EVERY column is unknown the plan is empty
 *    and the caller no-ops so the shape offset still advances.
 */
internal fun planPartialUpdate(
    pkColumns: List<String>,
    knownColumns: Set<String>,
    wireColumns: Map<String, JsonElement>,
    integerColumns: Set<String> = emptySet(),
): PartialUpdatePlan? {
    if (pkColumns != listOf("id")) return null
    val candidates = wireColumns.filterKeys { it != "id" }
    val known = candidates.filterKeys { it in knownColumns }
    return PartialUpdatePlan(
        setClause = known.keys.joinToString(", ") { "\"$it\" = ?" },
        args = known.map { (col, value) -> bindJsonValue(value, col in integerColumns) },
        droppedColumns = candidates.keys - known.keys,
    )
}

/**
 * Result of planning a delete for a `delete` message whose value payload failed
 * to decode (Electric sends PK-only — or partial — payloads for deletes and
 * move-outs, so full-entity decode routinely fails). [whereClause] targets the
 * row by primary key; [args] are the PK values parsed from the Electric key.
 */
internal data class DeleteByKeyPlan(
    val whereClause: String,
    val args: List<String>,
)

/**
 * Plan a delete from the Electric key alone (iOS `deleteByKey` parity). The key
 * encodes each PK value as a `/`-separated, quoted segment after the table
 * segment, in PK-column order — `"public"."issue_labels"/"<issue_id>"/"<label_id>"`
 * — which also covers composite-PK tables that have no surrogate `id`. Returns
 * null when the key's value count doesn't match the table's PK arity (nothing
 * safe to target).
 */
internal fun planDeleteByKey(pkColumns: List<String>, key: String): DeleteByKeyPlan? {
    val values = parseKeyComponents(key)
    if (pkColumns.isEmpty() || values.size != pkColumns.size) return null
    return DeleteByKeyPlan(
        whereClause = pkColumns.joinToString(" AND ") { "\"$it\" = ?" },
        args = values,
    )
}

/** Value segments of an Electric key (everything after the table segment), unquoted. */
internal fun parseKeyComponents(key: String): List<String> {
    val parts = key.split("/")
    if (parts.size <= 1) return emptyList()
    return parts.drop(1).map { it.trim('"') }
}

/** Bind a wire JSON value to a SQLite arg: null for JSON null, the serialized
 *  form for objects/arrays, and for scalars the raw content — except booleans,
 *  which must bind as 1L/0L. Room's Boolean columns are INTEGER-affinity, so a
 *  "true"/"t" TEXT would read back as false (that's how the Support tab went
 *  missing: the server's `helpdesk_enabled` flip arrives as a partial update
 *  carrying the Postgres text form "t", EXP-185). An unquoted JSON boolean
 *  converts unconditionally; the quoted Postgres text forms (t/true/1 —
 *  PgBoolSerializer's vocabulary, iOS `sqlValue` parity) convert only when the
 *  target column is INTEGER-affinity — a TEXT column's literal "true" must
 *  stay the text "true". */
internal fun bindJsonValue(value: JsonElement, isIntegerColumn: Boolean = false): Any? = when (value) {
    is JsonNull -> null
    is JsonPrimitive ->
        if (!value.isString) value.booleanOrNull?.let { if (it) 1L else 0L } ?: value.content
        else if (isIntegerColumn) when (value.content.lowercase()) {
            "t", "true", "1" -> 1L
            "f", "false", "0" -> 0L
            else -> value.content
        }
        else value.content
    else -> value.toString()
}
