package com.exponential.app.data.electric

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

sealed interface ShapeMessage<out T> {
    data class Insert<T>(val key: String, val value: T) : ShapeMessage<T>
    // EXP-985: EVERY wire `update` is a partial. Electric (default replica
    // mode) ships only the changed columns plus the PK, so there is no full
    // row to decode — the tolerant column-wise apply is the one update path.
    data class PartialUpdate(val key: String, val columns: String) : ShapeMessage<Nothing>
    data class Delete<T>(val key: String, val value: T?) : ShapeMessage<T>
    data object UpToDate : ShapeMessage<Nothing>
    data object MustRefetch : ShapeMessage<Nothing>
}

@Serializable
internal data class RawMessage(
    val headers: Map<String, JsonElement> = emptyMap(),
    val key: String? = null,
    val value: JsonElement? = null,
)
