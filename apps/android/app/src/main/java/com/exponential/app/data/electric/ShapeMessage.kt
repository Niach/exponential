package com.exponential.app.data.electric

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

sealed interface ShapeMessage<out T> {
    data class Insert<T>(val key: String, val value: T) : ShapeMessage<T>
    data class Update<T>(val key: String, val value: T) : ShapeMessage<T>
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
