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
): PartialUpdatePlan? {
    if (pkColumns != listOf("id")) return null
    val candidates = wireColumns.filterKeys { it != "id" }
    val known = candidates.filterKeys { it in knownColumns }
    return PartialUpdatePlan(
        setClause = known.keys.joinToString(", ") { "\"$it\" = ?" },
        args = known.values.map(::bindJsonValue),
        droppedColumns = candidates.keys - known.keys,
    )
}

/** Bind a wire JSON value to a SQLite arg: null for JSON null, the serialized
 *  form for objects/arrays, and for scalars the raw content — except an
 *  unquoted JSON boolean, which binds as 1L/0L. Room's Boolean columns are
 *  INTEGER-affinity, so a "true"/"false" TEXT would read back as false (a web
 *  unsubscribe → partial `{unsubscribed:true}` would leave the bell on). Only
 *  unquoted booleans are converted — a quoted string that happens to be "true"
 *  stays the text "true". */
internal fun bindJsonValue(value: JsonElement): Any? = when (value) {
    is JsonNull -> null
    is JsonPrimitive ->
        if (!value.isString) value.booleanOrNull?.let { if (it) 1L else 0L } ?: value.content
        else value.content
    else -> value.toString()
}
